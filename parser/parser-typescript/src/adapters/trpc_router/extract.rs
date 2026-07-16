//! The extraction walk itself — top-level `const` classification, object-entry parsing, and the
//! builder-chain verb scan. See the parent module's doc for the recognized vocabulary.

use swc_core::common::SourceMap;
use swc_core::ecma::ast::{
    Callee, Decl, Expr, MemberProp, ModuleDecl, ModuleItem, ObjectLit, Pat, Prop, PropName,
    PropOrSpread, Stmt,
};
use zzop_core::{ImportMap, ProcedureRouterEntry, ProcedureRouterFragment};

/// Extract every top-level tRPC router fragment from one file's raw source. Returns an empty `Vec`
/// (never panics) when the file fails to parse at all.
pub fn extract_procedure_router_fragments(rel: &str, text: &str) -> Vec<ProcedureRouterFragment> {
    let Some((cm, module)) = crate::parse_with_cm(rel, text) else {
        return Vec::new();
    };
    let imports = crate::parse_imports(rel, text);
    let mut out = Vec::new();
    for item in &module.body {
        let decl = match item {
            ModuleItem::Stmt(Stmt::Decl(d)) => Some(d),
            ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(e)) => Some(&e.decl),
            _ => None,
        };
        let Some(Decl::Var(v)) = decl else { continue };
        for d in &v.decls {
            let Pat::Ident(bi) = &d.name else { continue };
            let Some(init) = &d.init else { continue };
            let name = bi.id.sym.to_string();
            if let Some(fragment) = classify_top_level_init(name, init, &imports, &cm) {
                out.push(fragment);
            }
        }
    }
    out
}

/// Classifies one top-level `const <name> = <init>` binding as a router fragment, or `None` when `init`
/// is neither a recognized router-factory call nor `mergeRouters(...)` — see module doc.
fn classify_top_level_init(
    name: String,
    init: &Expr,
    imports: &ImportMap,
    cm: &SourceMap,
) -> Option<ProcedureRouterFragment> {
    let Expr::Call(call) = unwrap_expr(init) else {
        return None;
    };
    let Callee::Expr(callee) = &call.callee else {
        return None;
    };
    let Expr::Ident(id) = unwrap_expr(callee) else {
        return None;
    };
    if is_router_factory(&id.sym) {
        let entries = call
            .args
            .first()
            .and_then(|a| match unwrap_expr(&a.expr) {
                Expr::Object(o) => Some(o),
                _ => None,
            })
            .map(|o| parse_object_entries(o, imports, cm))
            .unwrap_or_default();
        Some(ProcedureRouterFragment { name, entries })
    } else if id.sym == "mergeRouters" {
        let mut entries = Vec::new();
        for arg in &call.args {
            if arg.spread.is_some() {
                continue; // a spread argument — not a plain sub-router ident, never guessed
            }
            if let Expr::Ident(aid) = unwrap_expr(&arg.expr) {
                entries.push(ProcedureRouterEntry::Ref {
                    key: String::new(),
                    ident: aid.sym.to_string(),
                    specifier: resolve_specifier(&aid.sym, imports),
                });
            }
            // a non-identifier argument (inline router(...), nested mergeRouters(...), literal, ...) is skipped.
        }
        Some(ProcedureRouterFragment { name, entries })
    } else {
        None
    }
}

fn is_router_factory(name: &str) -> bool {
    matches!(name, "router" | "createTRPCRouter")
}

/// `ident`'s import source when it is one of this file's own import bindings, `None` otherwise
/// (assumed same-file local).
fn resolve_specifier(ident: &str, imports: &ImportMap) -> Option<String> {
    imports.get(ident).map(|b| b.specifier.clone())
}

/// Parses one `router({...})` call's object-literal argument into its ordered entries — recursed into
/// for inline `Nested` routers.
fn parse_object_entries(
    obj: &ObjectLit,
    imports: &ImportMap,
    cm: &SourceMap,
) -> Vec<ProcedureRouterEntry> {
    let mut out = Vec::new();
    for prop in &obj.props {
        let PropOrSpread::Prop(p) = prop else {
            continue; // `...spread` property — not expanded, see module doc
        };
        let entry = match &**p {
            Prop::KeyValue(kv) => {
                let Some(key) = prop_key_name(&kv.key) else {
                    continue; // computed/num/bigint key — never guessed, skip just this entry
                };
                classify_entry(key, &kv.value, imports, cm)
            }
            Prop::Shorthand(id) => {
                // `{ bookings }` sugar for `{ bookings: bookings }` — see module doc.
                classify_entry(id.sym.to_string(), &Expr::Ident(id.clone()), imports, cm)
            }
            _ => None, // method/getter/setter/assign shorthand — not a recognized entry shape
        };
        if let Some(entry) = entry {
            out.push(entry);
        }
    }
    out
}

fn prop_key_name(key: &PropName) -> Option<String> {
    match key {
        PropName::Ident(i) => Some(i.sym.to_string()),
        PropName::Str(s) => Some(s.value.as_str().unwrap_or_default().to_string()),
        _ => None,
    }
}

/// Classifies one object property's value expression into a `ProcedureRouterEntry` — see module doc.
fn classify_entry(
    key: String,
    value: &Expr,
    imports: &ImportMap,
    cm: &SourceMap,
) -> Option<ProcedureRouterEntry> {
    match unwrap_expr(value) {
        Expr::Ident(id) => Some(ProcedureRouterEntry::Ref {
            key,
            ident: id.sym.to_string(),
            specifier: resolve_specifier(&id.sym, imports),
        }),
        Expr::Call(call) => {
            let Callee::Expr(callee) = &call.callee else {
                return None;
            };
            if let Expr::Ident(id) = unwrap_expr(callee) {
                if is_router_factory(&id.sym) {
                    let entries = call
                        .args
                        .first()
                        .and_then(|a| match unwrap_expr(&a.expr) {
                            Expr::Object(o) => Some(o),
                            _ => None,
                        })
                        .map(|o| parse_object_entries(o, imports, cm))
                        .unwrap_or_default();
                    return Some(ProcedureRouterEntry::Nested { key, entries });
                }
            }
            let verb = verb_of_call_chain(call)?;
            Some(ProcedureRouterEntry::Leaf {
                key,
                verb,
                line: crate::line_of(cm, call.span.lo),
            })
        }
        _ => None, // never guess — literal, conditional, other expression shape
    }
}

/// Walks a builder-chain call's callee links (`x.a(...).b(...).c(...)`, outermost call first, since swc
/// nests each earlier step inside the next call's `callee.obj`) looking for a member call named
/// `query`/`mutation`/`subscription`. Returns the uppercase verb on the first match, or `None`.
fn verb_of_call_chain(call: &swc_core::ecma::ast::CallExpr) -> Option<String> {
    let Callee::Expr(callee) = &call.callee else {
        return None;
    };
    let Expr::Member(m) = unwrap_expr(callee) else {
        return None;
    };
    if let MemberProp::Ident(name) = &m.prop {
        if let Some(verb) = verb_name(&name.sym) {
            return Some(verb.to_string());
        }
    }
    match unwrap_expr(&m.obj) {
        Expr::Call(inner) => verb_of_call_chain(inner),
        _ => None,
    }
}

fn verb_name(s: &str) -> Option<&'static str> {
    match s {
        "query" => Some("QUERY"),
        "mutation" => Some("MUTATION"),
        "subscription" => Some("SUBSCRIPTION"),
        _ => None,
    }
}

/// Strip wrappers between a value position and its real expression: `... as const`/`... as T`, `(...)`,
/// `... satisfies T`, `...!` — identical set to `egress.rs`'s own `unwrap_expr`.
fn unwrap_expr(e: &Expr) -> &Expr {
    let mut n = e;
    loop {
        n = match n {
            Expr::TsAs(a) => &a.expr,
            Expr::TsConstAssertion(c) => &c.expr,
            Expr::Paren(p) => &p.expr,
            Expr::TsSatisfies(s) => &s.expr,
            Expr::TsNonNull(nn) => &nn.expr,
            other => return other,
        };
    }
}
