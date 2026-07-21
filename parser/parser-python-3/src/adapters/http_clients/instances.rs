//! Instance-receiver tracking for `adapters::http_clients` — the file-wide first pass that binds a local
//! name to a `requests`/`httpx` client CONSTRUCTOR (`requests.Session()`, `httpx.Client()`/
//! `AsyncClient()`) so a later `.get()`/`.post()`/... on that name reads as egress. The `ruff` counterpart
//! of `zzop_parser_rust::adapters::http_clients`'s `BindingCollector`; see the parent module doc's
//! "Instance receivers" bullet for the shapes recognized and the shadowing approximation.
//!
//! Blast-radius caveat (accepted v1 approximation): names are collected file-wide into a flat set with no
//! scope or last-write tracking, exactly like the Rust `BindingCollector` this mirrors — a name bound to a
//! client anywhere in the file qualifies EVERY `.get`/`.post`/... on that name as egress. Python amplifies
//! this over the Rust/Go siblings because `.get` is also the ubiquitous `dict.get`: if a name is rebound
//! from a client to a mapping (`s = httpx.Client()` … `s = load_config()` … `s.get("timeout")`), the
//! `dict.get` false-reads as an HTTP consume. Gated behind a requests/httpx import AND an actual client
//! construction in the same file, so the surface is narrow; a full fix needs last-write-wins scoping and
//! is deferred (see backlog). The Go/Rust `.Get`/`.Post` verbs don't collide with a common stdlib method,
//! so their identical flat-binding approximation carries a smaller FP surface.

use std::collections::HashSet;

use ruff_python_ast::visitor::{walk_stmt, Visitor};
use ruff_python_ast::{Expr, ExprCall, Stmt};
use zzop_core::ImportMap;

/// The client CONSTRUCTOR names a binding recognizes.
const CLIENT_CTORS: &[&str] = &["Session", "Client", "AsyncClient"];

/// Every local name a `requests`/`httpx` client constructor is assigned or `with`-bound to across `body`
/// (recursing into nested function/class scopes). Empty when the file constructs no such client.
pub(super) fn instance_names(
    body: &[Stmt],
    client_names: &HashSet<String>,
    imports: &ImportMap,
) -> HashSet<String> {
    let ctor_direct = ctor_direct_names(imports);
    let mut collector = InstanceCollector {
        client_names,
        ctor_direct: &ctor_direct,
        out: HashSet::new(),
    };
    for stmt in body {
        collector.visit_stmt(stmt);
    }
    collector.out
}

/// Local names bound to a directly-imported client constructor (`from httpx import AsyncClient` ->
/// `AsyncClient`, incl. an `as` alias since the binding's `original` imported name, not the local name,
/// is tested). Only constructors imported from the `requests`/`httpx` module count — for a `from a.b
/// import c` the specifier is the DOTTED MODULE (`httpx`) and `original` is the imported name (`c`).
fn ctor_direct_names(imports: &ImportMap) -> HashSet<String> {
    imports
        .iter()
        .filter(|(_, b)| {
            (b.specifier == "requests" || b.specifier == "httpx")
                && CLIENT_CTORS.contains(&b.original.as_str())
        })
        .map(|(local, _)| local.clone())
        .collect()
}

/// True when `call` constructs a `requests`/`httpx` client: `<module>.Session/Client/AsyncClient(...)`
/// (module in `client_names`) or a directly-imported `Session/Client/AsyncClient(...)` (`ctor_direct`).
fn is_client_ctor_call(
    call: &ExprCall,
    client_names: &HashSet<String>,
    ctor_direct: &HashSet<String>,
) -> bool {
    match &*call.func {
        Expr::Attribute(attr) => {
            matches!(&*attr.value, Expr::Name(m) if client_names.contains(m.id.as_str()))
                && CLIENT_CTORS.contains(&attr.attr.as_str())
        }
        Expr::Name(n) => ctor_direct.contains(n.id.as_str()),
        _ => false,
    }
}

struct InstanceCollector<'a> {
    client_names: &'a HashSet<String>,
    ctor_direct: &'a HashSet<String>,
    out: HashSet<String>,
}

impl<'a> InstanceCollector<'a> {
    fn is_ctor(&self, call: &ExprCall) -> bool {
        is_client_ctor_call(call, self.client_names, self.ctor_direct)
    }
}

impl<'a> Visitor<'a> for InstanceCollector<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            // `s = requests.Session()` / `client = httpx.AsyncClient()` (any bare-Name target).
            Stmt::Assign(a) => {
                if let Expr::Call(call) = &*a.value {
                    if self.is_ctor(call) {
                        for t in &a.targets {
                            if let Expr::Name(n) = t {
                                self.out.insert(n.id.to_string());
                            }
                        }
                    }
                }
            }
            // `client: httpx.AsyncClient = httpx.AsyncClient()`.
            Stmt::AnnAssign(a) => {
                if let (Expr::Name(n), Some(Expr::Call(call))) = (&*a.target, a.value.as_deref()) {
                    if self.is_ctor(call) {
                        self.out.insert(n.id.to_string());
                    }
                }
            }
            // `with httpx.Client() as c:` / `async with httpx.AsyncClient() as client:`.
            Stmt::With(w) => {
                for item in &w.items {
                    if let (Expr::Call(call), Some(Expr::Name(n))) =
                        (&item.context_expr, item.optional_vars.as_deref())
                    {
                        if self.is_ctor(call) {
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
