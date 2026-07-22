//! Instance-receiver tracking for `adapters::http_clients` — resolves a `<name>.<verb>(...)` receiver
//! against `<name>`'s MOST RECENT binding at or before the call site's line (LAST-WRITE-WINS, file-order
//! line granularity — see "Resolution contract" below). A name currently bound to a `requests`/`httpx`
//! client CONSTRUCTOR (`requests.Session()`, `httpx.Client()`/`AsyncClient()`) qualifies the call as HTTP
//! egress; a name most recently reassigned to anything else (a KILL) does not. This is backlog B14①'s
//! fix: `s = httpx.Client(); ...; s = load_config(); s.get("timeout")` no longer misreads `s.get` as an
//! HTTP GET, since the `load_config()` reassignment kills the binding before the `.get` call site. A
//! later reassignment BACK to a client construction revives the binding from that line on.
//!
//! ## Resolution contract (last-write-wins, file-order line)
//! For each tracked name, every relevant statement across the file — assignment, `AnnAssign` (with a
//! value; a bare `x: int` annotation binds nothing), `with`/`async with` binding, and `del` — is recorded
//! as `(line, BindingKind)`: `BindingKind::Client(ctor)` for a recognized client-constructor RHS/context
//! expression, `BindingKind::Killed` for anything else (a non-client RHS, a non-client `with` binding, or
//! a `del <name>`). A call site at line `L` resolves by finding the GREATEST recorded line `<= L` and
//! reading its `BindingKind`; no prior entry (or a `Killed` one) means the call does not qualify.
//! Same-line resolves to the assignment: this crate tracks only 1-based LINE numbers (see `LineIndex`'s
//! module doc), not columns, so a same-line `s = httpx.Client(); s.get(...)` cannot be ordered any finer
//! than "the same line" — treating it as already-bound is the more useful of the two readings and is
//! documented here rather than silently guessed. The symmetric hazard exists on the KILL side: in
//! `s = f(s.get("/x"))` the call reads the OLD client binding but resolves to that line's new `Killed`
//! entry, dropping a real consume (a niche same-line FN, accepted with the same one-line-granularity
//! rationale).
//!
//! ## Two-pass build (memory bound)
//! Pass 1 walks the file once collecting the SET of names EVER bound to a client constructor anywhere
//! (the pre-fix flat check, kept as-is). Pass 2 walks again and records a binding event only for names in
//! that set, so memory stays proportional to client-bound-name assignments, not every assignment in the
//! file — an unrelated name (e.g. a `cache` dict reassigned fifty times) contributes zero history entries.
//!
//! ## Remaining approximations (v1, documented not fixed)
//! - **Nested-function shadowing**: scope grain stays FILE-LEVEL line order, not a per-function scope
//!   tree (recursion into `def`/`class`/`with`/... bodies flattens into the same per-name history) — a
//!   name reused across an outer scope and a nested `def`/lambda shares one binding history, so an inner
//!   rebind can shadow (or revive) an outer name's binding at a line that, dynamically, never executes in
//!   the outer scope's control flow. Unchanged from the pre-fix flat approximation.
//! - **Augmented assignment** (`s += x`) is not modeled as either a bind or a kill — it leaves whatever
//!   binding state preceded it untouched. Usually the intended "still the same object" reading, but it is
//!   not a from-first-principles analysis of the RHS.
//! - **Unrecognized client forms kill**: a reassignment whose RHS constructs a client through a form the
//!   recognizer does not model — an aliased constructor (`C = httpx.Client; s = C()`), a factory
//!   (`s = build_client()`) — classifies as `Killed`, so later real consumes on that name are dropped.
//!   This is B14①'s deliberate trade (flat-FP removed at the cost of this narrower FN class): the kill
//!   side must stay RHS-shape-blind or the original dict-rebind FP returns.
//! - Multiple targets in one statement (`a = b = requests.Session()`) all resolve to the SAME line, so
//!   their relative order within that statement is not distinguished (same-line rule above applies).
//! - `del <name>` IS tracked as a `Killed` event (cheap — one more `Stmt` arm), so a `del`-then-reuse
//!   correctly reads as unbound until the next assignment.

use std::collections::{HashMap, HashSet};

use ruff_python_ast::visitor::{walk_stmt, Visitor};
use ruff_python_ast::{Expr, ExprCall, Stmt};
use ruff_text_size::Ranged;
use zzop_core::ImportMap;

use crate::LineIndex;

/// The client CONSTRUCTOR names a binding recognizes.
const CLIENT_CTORS: &[&str] = &["Session", "Client", "AsyncClient"];

/// A name's resolution state as of one recorded line — see module doc's last-write-wins contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BindingKind {
    /// Bound to a client constructor of the given kind (`"Session"`/`"Client"`/`"AsyncClient"`).
    Client(&'static str),
    /// Reassigned (or `del`-ed) to something that is not a client construction — kills a prior binding.
    Killed,
}

/// Every tracked name's binding history, ascending by line — see module doc.
pub(super) struct Bindings {
    history: HashMap<String, Vec<(u32, BindingKind)>>,
}

impl Bindings {
    /// True when `name`'s most recent binding at or before `line` (module doc's same-line rule) is a
    /// client binding. `false` for an untracked name (never client-bound anywhere in the file) or one
    /// whose latest applicable entry is `Killed`.
    pub(super) fn is_client_at(&self, name: &str, line: u32) -> bool {
        let Some(entries) = self.history.get(name) else {
            return false;
        };
        let idx = entries.partition_point(|(l, _)| *l <= line);
        idx > 0 && matches!(entries[idx - 1].1, BindingKind::Client(_))
    }
}

/// Builds the last-write-wins binding history (module doc's two-pass build) for every local name bound to
/// a `requests`/`httpx` client constructor anywhere in `body`.
pub(super) fn instance_bindings(
    body: &[Stmt],
    client_names: &HashSet<String>,
    imports: &ImportMap,
    idx: &LineIndex,
) -> Bindings {
    let ctor_direct = ctor_direct_names(imports);
    // Pass 1: which names are EVER client-bound (the pre-fix flat check) — bounds pass 2's memory.
    let mut tracked = NameCollector {
        client_names,
        ctor_direct: &ctor_direct,
        out: HashSet::new(),
    };
    for stmt in body {
        tracked.visit_stmt(stmt);
    }
    // Pass 2: full bind+kill history, recorded only for names pass 1 found relevant.
    let mut history = HistoryCollector {
        client_names,
        ctor_direct: &ctor_direct,
        tracked: &tracked.out,
        idx,
        out: HashMap::new(),
    };
    for stmt in body {
        history.visit_stmt(stmt);
    }
    for entries in history.out.values_mut() {
        entries.sort_by_key(|&(line, _)| line);
    }
    Bindings {
        history: history.out,
    }
}

/// Local names bound to a directly-imported client constructor (`from httpx import AsyncClient` ->
/// `AsyncClient`, incl. an `as` alias since the binding's `original` imported name, not the local name, is
/// tested), mapped to WHICH constructor it is. Only constructors imported from the `requests`/`httpx`
/// module count — for a `from a.b import c` the specifier is the DOTTED MODULE (`httpx`) and `original`
/// is the imported name (`c`).
fn ctor_direct_names(imports: &ImportMap) -> HashMap<String, &'static str> {
    imports
        .iter()
        .filter(|(_, b)| b.specifier == "requests" || b.specifier == "httpx")
        .filter_map(|(local, b)| {
            CLIENT_CTORS
                .iter()
                .find(|&&c| c == b.original.as_str())
                .map(|&c| (local.clone(), c))
        })
        .collect()
}

/// The client-constructor kind `call` constructs, if any: `<module>.Session/Client/AsyncClient(...)`
/// (module in `client_names`) or a directly-imported `Session/Client/AsyncClient(...)` (`ctor_direct`).
fn client_ctor_kind(
    call: &ExprCall,
    client_names: &HashSet<String>,
    ctor_direct: &HashMap<String, &'static str>,
) -> Option<&'static str> {
    match &*call.func {
        Expr::Attribute(attr) => {
            let recv_is_client =
                matches!(&*attr.value, Expr::Name(m) if client_names.contains(m.id.as_str()));
            if !recv_is_client {
                return None;
            }
            CLIENT_CTORS
                .iter()
                .copied()
                .find(|&c| c == attr.attr.as_str())
        }
        Expr::Name(n) => ctor_direct.get(n.id.as_str()).copied(),
        _ => None,
    }
}

/// Pass 1: the set of names EVER bound to a client constructor anywhere in the file (recurses into
/// nested function/class scopes — module doc's flat scope grain).
struct NameCollector<'a> {
    client_names: &'a HashSet<String>,
    ctor_direct: &'a HashMap<String, &'static str>,
    out: HashSet<String>,
}

impl<'a> Visitor<'a> for NameCollector<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        let kind = |call: &ExprCall| client_ctor_kind(call, self.client_names, self.ctor_direct);
        match stmt {
            Stmt::Assign(a) => {
                if let Expr::Call(call) = &*a.value {
                    if kind(call).is_some() {
                        for t in &a.targets {
                            if let Expr::Name(n) = t {
                                self.out.insert(n.id.to_string());
                            }
                        }
                    }
                }
            }
            Stmt::AnnAssign(a) => {
                if let (Expr::Name(n), Some(Expr::Call(call))) = (&*a.target, a.value.as_deref()) {
                    if kind(call).is_some() {
                        self.out.insert(n.id.to_string());
                    }
                }
            }
            Stmt::With(w) => {
                for item in &w.items {
                    if let (Expr::Call(call), Some(Expr::Name(n))) =
                        (&item.context_expr, item.optional_vars.as_deref())
                    {
                        if kind(call).is_some() {
                            self.out.insert(n.id.to_string());
                        }
                    }
                }
            }
            _ => {}
        }
        walk_stmt(self, stmt);
    }
}

/// Pass 2: full bind/kill history for exactly the names pass 1 found relevant (module doc's memory
/// bound). Same statement shapes as pass 1, plus `del` (a kill).
struct HistoryCollector<'a> {
    client_names: &'a HashSet<String>,
    ctor_direct: &'a HashMap<String, &'static str>,
    tracked: &'a HashSet<String>,
    idx: &'a LineIndex,
    out: HashMap<String, Vec<(u32, BindingKind)>>,
}

impl<'a> HistoryCollector<'a> {
    fn record(&mut self, name: &str, line: u32, kind: BindingKind) {
        if self.tracked.contains(name) {
            self.out
                .entry(name.to_string())
                .or_default()
                .push((line, kind));
        }
    }

    /// `Client(ctor)` for a recognized constructor call, else `Killed` — the shared classification for
    /// every non-`del` binding event (an assignment/`AnnAssign` RHS, or a `with` context expression).
    fn classify(&self, expr: &Expr) -> BindingKind {
        match expr {
            Expr::Call(call) => client_ctor_kind(call, self.client_names, self.ctor_direct)
                .map(BindingKind::Client)
                .unwrap_or(BindingKind::Killed),
            _ => BindingKind::Killed,
        }
    }
}

impl<'a> Visitor<'a> for HistoryCollector<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::Assign(a) => {
                let binding = self.classify(&a.value);
                for t in &a.targets {
                    if let Expr::Name(n) = t {
                        let line = self.idx.line_of(t.start());
                        self.record(n.id.as_str(), line, binding);
                    }
                }
            }
            // A bare `x: int` (no value) is an annotation only — it does not bind `x` at runtime, so it
            // must not be recorded as either a bind or a kill.
            Stmt::AnnAssign(a) => {
                if let (Expr::Name(n), Some(value)) = (&*a.target, a.value.as_deref()) {
                    let binding = self.classify(value);
                    let line = self.idx.line_of(a.target.start());
                    self.record(n.id.as_str(), line, binding);
                }
            }
            Stmt::With(w) => {
                for item in &w.items {
                    if let Some(Expr::Name(n)) = item.optional_vars.as_deref() {
                        let binding = self.classify(&item.context_expr);
                        let line = self.idx.line_of(item.context_expr.start());
                        self.record(n.id.as_str(), line, binding);
                    }
                }
            }
            Stmt::Delete(d) => {
                for target in &d.targets {
                    if let Expr::Name(n) = target {
                        let line = self.idx.line_of(target.start());
                        self.record(n.id.as_str(), line, BindingKind::Killed);
                    }
                }
            }
            _ => {}
        }
        walk_stmt(self, stmt);
    }
}
