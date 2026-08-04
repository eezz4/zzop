//! Per-file CALL-SITE projection for Python — the `console-write` and `env-read` families of
//! [`zzop_core::CallSite`], the substrate `zzop_core::dsl::Matcher::CallScan` reads.
//!
//! The channel's own contract (what a site carries, and why there is no `level`/`stream` field) is
//! `zzop_core::call_sites`'s to state. What that contract delegates to each PRODUCER is the boundary:
//! which spellings in THIS language are the family and which are deliberately not. That list is the
//! second half of this doc, and it is the whole of this module's judgment — the code below only
//! matches the spellings named here.
//!
//! ## What is recognized (wave 1)
//! - **`console-write`** — a call to the built-in `print(...)`. `callee` = `print`.
//! - **`env-read`** — three module-qualified idioms, `callee` spelled exactly as written:
//!   `os.getenv(...)` → `os.getenv` · `os.environ.get(...)` → `os.environ.get` ·
//!   `os.environ[...]` (subscript READ) → `os.environ`.
//!
//! ## What is recognized (wave 3)
//! - **`process-exec`** — the stdlib process APIs, `callee` spelled exactly as written:
//!   `subprocess.run` · `subprocess.call` · `subprocess.check_call` · `subprocess.check_output` ·
//!   `subprocess.Popen` (a CLASS construction — Python spells it as a call and it launches the
//!   process, which is exactly what the family claims, so no `new`-vs-call distinction arises here
//!   the way it does in Java/C#) · `os.system` · `os.popen` ([`PROCESS_EXEC_CALLEES`]).
//!   Deliberately NOT this family: `os.fork`/`os.exec*` (raw syscall wrappers no consuming rule asks
//!   about), `multiprocessing` (in-process Python workers, not an OS command), and any third-party
//!   wrapper (`sh`, `plumbum`, `invoke`) — not the platform's API, so its argv/shell semantics are
//!   not the ones the consuming rules state claims about. As with `env-read`, a bare-name import
//!   (`from subprocess import run` → `run(...)`) spells `run`, not `subprocess.run`, and is silent:
//!   the recognized set is exactly what the consuming rule pins, and widening it is a rule-side
//!   change rather than a quiet producer-side one.
//!
//! ## What is recognized (wave 4)
//! - **`hash-call`** — `hashlib`'s constructors. Two spellings, and they differ in exactly the way
//!   `CallSite::algorithm`'s never-guess rule is about: a per-algorithm constructor
//!   (`hash_call::HASHLIB_CONSTRUCTORS`, `hashlib.md5()`) names the algorithm in the FUNCTION name, so
//!   `algorithm` is `Some("md5")` with no argument read at all; the generic `hash_call::HASHLIB_NEW`
//!   (`hashlib.new(...)`) names it in an argument, so `algorithm` is `Some` only for a plain string
//!   LITERAL first positional argument and `None` for `hashlib.new(name)` / an f-string / a
//!   keyword-only spelling. A `None` site is still a site — the digest construction happened, and only
//!   the `algorithm_pattern` filter loses it. Deliberately NOT this family: `hashlib.pbkdf2_hmac` and
//!   `scrypt` (KDFs, whose parameters decide strength, not their name), `hmac` (an HMAC's strength is
//!   its inner hash's, which this site does not spell), and every third-party digest package. A
//!   bare-name import (`from hashlib import md5` → `md5(...)`) spells `md5`, not `hashlib.md5`, and is
//!   silent — the same line the `env-read` and `process-exec` families draw.
//!
//! `callee` is the dotted spelling of the callee expression, receiver included and un-normalized —
//! the one departure being intra-chain whitespace (`os . getenv` yields `os.getenv`), because the
//! spelling is reassembled from the attribute chain rather than sliced out of the source. A callee
//! that is not a plain `Name`/`Attribute` chain (`get_module().getenv(...)`, `handlers[i](...)`)
//! resolves to nothing and emits NO site, per the channel's never-guess rule.
//!
//! ## Deliberate silences — every one of these is a decision, not a gap
//! - **Structured loggers** (`logging.info`, `logger.warning`, `structlog`) are NOT `console-write`.
//!   The channel doc calls folding them in a FALSE FOLD, and it would be: a logger call is configured
//!   output with levels and sinks, and a rule banning console writes in a backend is not banning
//!   logging. No `logging` spelling appears below.
//! - **The stream of a `print`** (`print(..., file=sys.stderr)`) is an ARGUMENT fact, and wave 1
//!   carries no argument facts. The consequence is worth saying plainly because a reader will assume
//!   the opposite: this channel does NOT claim `print` writes to stdout. It says a `print` call
//!   happened at line N. A rule that needs the stream cannot get it here.
//! - **`self.print(...)` / `logger.print(...)`** — member calls. Their dotted spelling is
//!   `self.print` / `logger.print`, which is not `print`, so they are excluded by construction rather
//!   than by a special case.
//! - **A file that REBINDS `print`** (`def print(...)`, `print = ...`, `from rich import print`, a
//!   parameter named `print`) emits no `console-write` at all — see [`rebinds`]. The family claims the
//!   BUILT-IN, and a file that shadows the name has taken that claim away; going quiet is the
//!   never-guess answer and it degrades in this channel's declared RECALL direction. The check is
//!   file-scoped with no scope tracking, so a `print` parameter on one nested function silences the
//!   whole file — over-silence on purpose, never over-claim. `os` gets no such check: the evidence
//!   there is a module-qualified two-segment spelling, not a bare built-in name, and nothing shadows
//!   `os.getenv` the way a helper shadows `print`.
//! - **Bare-name env reads** (`from os import getenv` → `getenv("X")`) are silent. Their spelling is
//!   `getenv`, not `os.getenv`, and the recognized set above is exactly what the consuming rule pins;
//!   widening it is a rule-side change, not a quiet producer-side one.
//! - **`os.environ[...]` in a WRITE or DELETE position** (`os.environ["X"] = v`, `del os.environ["X"]`)
//!   is not a read. Only `ExprContext::Load` subscripts emit.
//! - **Bare `os.environ`** used as a mapping (`for k in os.environ`, `"X" in os.environ`,
//!   `os.environ.keys()`) is not one of the three idioms and emits nothing.
//! - A **dynamic key** (`os.environ[key]`, `os.getenv(name)`) DOES emit — the read point is real and
//!   statically witnessed. Only the key would be a guess, and the key is not a field.
//! - **String and comment text** never fires, which is the point of projecting instead of regexing:
//!   `"print(x)"` and `f"os.getenv(A)"` are literals. A real call inside an f-string INTERPOLATION
//!   (`f"{os.getenv('X')}"`) is a real call and does emit.
//!
//! ## Order and degrade
//! Sites come out in SOURCE order (ascending start offset), sorted rather than trusted from the walk —
//! the AST field order of a few nodes (a ternary's `test` before its `body`) does not match source
//! order, and the channel's determinism contract is stated in terms of the source. A parse failure
//! yields an empty vec, never a panic — the same degrade-to-nothing contract every `extract_*` in this
//! crate upholds, and the RECALL direction `crates/engine/src/pipeline/fresh/call_sites.rs` documents.

mod hash_call;

use hash_call::hash_algorithm;
use ruff_python_ast::visitor::{walk_expr, walk_stmt, Visitor};
use ruff_python_ast::{Expr, ExprContext, Parameter, Stmt};
use ruff_text_size::{Ranged, TextSize};
use zzop_core::{
    CallSite, CALL_KIND_CONSOLE_WRITE, CALL_KIND_ENV_READ, CALL_KIND_HASH_CALL,
    CALL_KIND_PROCESS_EXEC,
};

/// The built-in whose calls are console writes. A bare name, which is exactly why [`rebinds`] exists.
const BUILTIN_PRINT: &str = "print";

/// The CALLED env-read idioms, spelled as the callee is spelled. `os.environ.get` is here and not
/// derived from `os.environ` + `.get`: the subscript form below is a different node shape, and listing
/// both spellings is what makes the recognized set readable in one place.
const ENV_READ_CALLEES: &[&str] = &["os.getenv", "os.environ.get"];

/// The SUBSCRIPTED env-read idiom — `os.environ[...]`, whose callee is the mapping itself.
const ENV_READ_MAPPING: &str = "os.environ";

/// The PROCESS-EXEC idioms, spelled as the callee is spelled — Python stdlib names (`subprocess`'s
/// five launch entry points plus `os`'s two shell helpers), fixed by the language rather than by a
/// project. Scope and every exclusion are argued in the module doc.
const PROCESS_EXEC_CALLEES: &[&str] = &[
    "subprocess.run",
    "subprocess.call",
    "subprocess.check_call",
    "subprocess.check_output",
    "subprocess.Popen",
    "os.system",
    "os.popen",
];

/// Extract this file's call sites — see module doc for the recognized spellings and every deliberate
/// silence. Empty on parse failure (never panics). `_rel` is unused (ruff parsing needs no filename),
/// kept to match the engine's uniform `(rel, text)` projection call convention.
pub fn extract_call_sites(_rel: &str, text: &str) -> Vec<CallSite> {
    let Some(module) = crate::parse_module(text) else {
        return Vec::new();
    };
    let idx = crate::LineIndex::new(text);
    let mut collector = CallSiteCollector {
        idx: &idx,
        print_is_builtin: !rebinds(&module, BUILTIN_PRINT),
        out: Vec::new(),
    };
    for stmt in &module.body {
        collector.visit_stmt(stmt);
    }
    // Source order (module doc): stable sort by start offset, so a walk that visits a node's fields
    // out of source order cannot leak that into the channel.
    collector.out.sort_by_key(|(offset, _)| *offset);
    collector.out.into_iter().map(|(_, site)| site).collect()
}

/// The dotted spelling of a callee/receiver expression, or `None` when it is not a plain
/// `Name`/`Attribute` chain — never-guess (module doc).
fn dotted_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Name(n) => Some(n.id.as_str().to_string()),
        Expr::Attribute(a) => Some(format!("{}.{}", dotted_name(&a.value)?, a.attr.as_str())),
        _ => None,
    }
}

/// Does this file bind `name` anywhere, i.e. is the built-in of that name shadowed? File-scoped, no
/// scope tracking — see the module doc's rebinding bullet for why over-silence is the intended error.
///
/// Every binding form reduces to one of four things: a `Name` in a STORE/DELETE context (assignment,
/// `for` target, `with ... as`, walrus, comprehension target, tuple unpacking — ruff marks them all),
/// a `def`/`class` name, an `import` alias, or a parameter. `except E as name` is the one leftover and
/// gets its own arm.
fn rebinds(module: &ruff_python_ast::ModModule, name: &str) -> bool {
    let mut scan = RebindScan { name, found: false };
    for stmt in &module.body {
        scan.visit_stmt(stmt);
    }
    scan.found
}

struct RebindScan<'n> {
    name: &'n str,
    found: bool,
}

impl<'a> Visitor<'a> for RebindScan<'_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::FunctionDef(d) if d.name.as_str() == self.name => self.found = true,
            Stmt::ClassDef(d) if d.name.as_str() == self.name => self.found = true,
            Stmt::Try(t) => {
                for handler in &t.handlers {
                    let ruff_python_ast::ExceptHandler::ExceptHandler(h) = handler;
                    if h.name.as_ref().is_some_and(|n| n.as_str() == self.name) {
                        self.found = true;
                    }
                }
            }
            _ => {}
        }
        walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Expr::Name(n) = expr {
            if n.id.as_str() == self.name && n.ctx != ExprContext::Load {
                self.found = true;
            }
        }
        walk_expr(self, expr);
    }

    fn visit_alias(&mut self, alias: &'a ruff_python_ast::Alias) {
        // `import x as name` / `from m import name` — and plain `import name.sub`, whose bound name is
        // the first segment.
        let bound = alias.asname.as_ref().map_or_else(
            || alias.name.split('.').next().unwrap_or(""),
            |a| a.as_str(),
        );
        if bound == self.name {
            self.found = true;
        }
    }

    fn visit_parameter(&mut self, parameter: &'a Parameter) {
        if parameter.name.as_str() == self.name {
            self.found = true;
        }
        ruff_python_ast::visitor::walk_parameter(self, parameter);
    }
}

/// Preorder walk; the emitted order is fixed by the sort in [`extract_call_sites`], not by this walk.
struct CallSiteCollector<'a> {
    idx: &'a crate::LineIndex,
    /// False when the file shadows `print` — every `console-write` then goes silent (module doc).
    print_is_builtin: bool,
    out: Vec<(TextSize, CallSite)>,
}

impl CallSiteCollector<'_> {
    fn push(&mut self, start: TextSize, kind: &str, callee: String) {
        self.push_with_algorithm(start, kind, callee, None);
    }

    /// The one push that fills `CallSite::algorithm` — `hash-call` only, and only with a spelling the
    /// source wrote (module doc's never-guess list).
    fn push_with_algorithm(
        &mut self,
        start: TextSize,
        kind: &str,
        callee: String,
        algorithm: Option<String>,
    ) {
        self.out.push((
            start,
            CallSite {
                kind: kind.to_string(),
                line: self.idx.line_of(start),
                callee,
                algorithm,
            },
        ));
    }
}

impl<'a> Visitor<'a> for CallSiteCollector<'_> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Call(call) => {
                if let Some(callee) = dotted_name(&call.func) {
                    // A shadowed `print` falls through to the env test, which cannot match it — the
                    // two families are disjoint, so one `else if` chain is the whole dispatch.
                    if callee == BUILTIN_PRINT && self.print_is_builtin {
                        self.push(expr.start(), CALL_KIND_CONSOLE_WRITE, callee);
                    } else if ENV_READ_CALLEES.contains(&callee.as_str()) {
                        self.push(expr.start(), CALL_KIND_ENV_READ, callee);
                    } else if PROCESS_EXEC_CALLEES.contains(&callee.as_str()) {
                        self.push(expr.start(), CALL_KIND_PROCESS_EXEC, callee);
                    } else if let Some(algorithm) = hash_algorithm(&callee, call) {
                        self.push_with_algorithm(
                            expr.start(),
                            CALL_KIND_HASH_CALL,
                            callee,
                            algorithm,
                        );
                    }
                }
            }
            // Subscript READ only: a Store/Del context is a write or a delete, not a read (module doc).
            Expr::Subscript(sub)
                if sub.ctx == ExprContext::Load
                    && dotted_name(&sub.value).as_deref() == Some(ENV_READ_MAPPING) =>
            {
                self.push(
                    expr.start(),
                    CALL_KIND_ENV_READ,
                    ENV_READ_MAPPING.to_string(),
                );
            }
            _ => {}
        }
        walk_expr(self, expr);
    }
}

#[cfg(test)]
mod tests;
