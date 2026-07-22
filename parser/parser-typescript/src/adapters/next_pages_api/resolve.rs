//! Default-export resolution: locates the first `export default …` (or `export { x as default }`)
//! in a module and resolves it down to the underlying function-like value — an inline
//! `function`/arrow expression, or (for `export default <ident>` / `export { <ident> as default }`)
//! a same-file top-level `function <ident>(…) {…}` or `const <ident> = (…) => …` binding — that
//! [`super::collector`] then walks for verb witnesses.

use swc_core::common::SourceMap;
use swc_core::ecma::ast::{
    ArrowExpr, BlockStmt, BlockStmtOrExpr, Decl, DefaultDecl, ExportSpecifier, Expr, Function,
    Module, ModuleDecl, ModuleItem, NamedExport, Pat, Stmt,
};

use crate::imports::export_name;

/// One resolved function-like value: its first parameter's simple-ident name (the request-object
/// binding verbs are witnessed through — `None` when unresolvable, e.g. a destructured parameter)
/// and its body (`None` when the value has none reachable, e.g. an unresolved identifier).
pub(super) struct HandlerValue<'a> {
    pub(super) param_name: Option<String>,
    pub(super) body: Option<HandlerBody<'a>>,
}

/// The default export's line plus its resolved handler value.
pub(super) struct DefaultExport<'a> {
    pub(super) line: u32,
    pub(super) handler: HandlerValue<'a>,
}

/// A handler body to walk for verb witnesses: a block (`function`/braced-arrow), a bare arrow
/// expression body, or — for a export expression that isn't a function at all (e.g. the
/// `defaultHandler({…})` idiom, or a HOF-wrapped call) — the export expression itself, so its own
/// call-expression signal is still reachable even though no request-parameter is known.
pub(super) enum HandlerBody<'a> {
    Block(&'a BlockStmt),
    Expr(&'a Expr),
}

/// 1-based line of the first module item that is a default export, plus its resolved handler value —
/// source-ordered, so the first match is the earliest in the file (the same "first `export default`"
/// contract the line-only scan this replaced had).
pub(super) fn find_default_export<'a>(
    cm: &SourceMap,
    module: &'a Module,
) -> Option<DefaultExport<'a>> {
    for item in &module.body {
        let ModuleItem::ModuleDecl(decl) = item else {
            continue;
        };
        let (line, handler) = match decl {
            ModuleDecl::ExportDefaultDecl(e) => (
                crate::line_of(cm, e.span.lo),
                handler_value_from_decl(&e.decl),
            ),
            ModuleDecl::ExportDefaultExpr(e) => (
                crate::line_of(cm, e.span.lo),
                handler_value_from_expr(module, &e.expr),
            ),
            ModuleDecl::ExportNamed(named) if has_default_specifier(named) => (
                crate::line_of(cm, named.span.lo),
                default_specifier_local_name(named)
                    .map(|name| resolve_ident_handler(module, &name))
                    .unwrap_or(HandlerValue {
                        param_name: None,
                        body: None,
                    }),
            ),
            _ => continue,
        };
        return Some(DefaultExport { line, handler });
    }
    None
}

/// True when any specifier of `named` is aliased `as default` (`export { x as default }`).
fn has_default_specifier(named: &NamedExport) -> bool {
    named.specifiers.iter().any(|spec| {
        matches!(spec, ExportSpecifier::Named(n)
            if n.exported.as_ref().is_some_and(|alias| export_name(alias) == "default"))
    })
}

/// The LOCAL name behind `export { <name> as default }` — `None` for a re-export (`export { x as
/// default } from "./y"`, `named.src.is_some()`), which points at another file this scan never reads
/// (never-guess).
fn default_specifier_local_name(named: &NamedExport) -> Option<String> {
    if named.src.is_some() {
        return None;
    }
    named.specifiers.iter().find_map(|spec| match spec {
        ExportSpecifier::Named(n)
            if n.exported
                .as_ref()
                .is_some_and(|alias| export_name(alias) == "default") =>
        {
            Some(export_name(&n.orig))
        }
        _ => None,
    })
}

/// `export default function …` / `export default class …` — only the function shape carries a
/// param+body; a class or an ambient interface never does.
fn handler_value_from_decl(decl: &DefaultDecl) -> HandlerValue<'_> {
    match decl {
        DefaultDecl::Fn(fe) => function_value(&fe.function),
        _ => HandlerValue {
            param_name: None,
            body: None,
        },
    }
}

/// `export default <expr>` — an inline arrow/function IS the handler; a bare identifier resolves to
/// a same-file top-level binding; anything else (a HOF-wrapped call, a `defaultHandler({…})` map, …)
/// is walked as-is with no resolved parameter, so its own call-expression signal can still fire but
/// no method-member comparison can (never-guess: no known request-object identifier to match).
fn handler_value_from_expr<'a>(module: &'a Module, expr: &'a Expr) -> HandlerValue<'a> {
    match expr {
        Expr::Arrow(arrow) => arrow_value(arrow),
        Expr::Fn(fe) => function_value(&fe.function),
        Expr::Ident(id) => resolve_ident_handler(module, id.sym.as_str()),
        other => HandlerValue {
            param_name: None,
            body: Some(HandlerBody::Expr(other)),
        },
    }
}

/// Resolves `name` to a same-file top-level `function name(…) {…}` or `const name = (…) => …` /
/// `const name = function(…) {…}` binding (exported or not — the binding `export default name;`
/// refers to is often declared plain). First match in source order wins; no match is an honest miss.
fn resolve_ident_handler<'a>(module: &'a Module, name: &str) -> HandlerValue<'a> {
    for item in &module.body {
        let decl = match item {
            ModuleItem::Stmt(Stmt::Decl(d)) => d,
            ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(e)) => &e.decl,
            _ => continue,
        };
        match decl {
            Decl::Fn(fd) if fd.ident.sym == name => return function_value(&fd.function),
            Decl::Var(vd) => {
                for d in &vd.decls {
                    let Pat::Ident(bi) = &d.name else { continue };
                    if bi.id.sym != name {
                        continue;
                    }
                    return match d.init.as_deref() {
                        Some(Expr::Arrow(arrow)) => arrow_value(arrow),
                        Some(Expr::Fn(fe)) => function_value(&fe.function),
                        _ => HandlerValue {
                            param_name: None,
                            body: None,
                        },
                    };
                }
            }
            _ => {}
        }
    }
    HandlerValue {
        param_name: None,
        body: None,
    }
}

fn function_value(f: &Function) -> HandlerValue<'_> {
    HandlerValue {
        param_name: f.params.first().and_then(|p| pat_ident_name(&p.pat)),
        body: f.body.as_ref().map(HandlerBody::Block),
    }
}

fn arrow_value(arrow: &ArrowExpr) -> HandlerValue<'_> {
    HandlerValue {
        param_name: arrow.params.first().and_then(pat_ident_name),
        body: Some(match &*arrow.body {
            BlockStmtOrExpr::BlockStmt(b) => HandlerBody::Block(b),
            BlockStmtOrExpr::Expr(e) => HandlerBody::Expr(e),
        }),
    }
}

/// A simple identifier binding's name — `None` for a destructured/rest first parameter (never-guess:
/// no single request-object identifier to witness `.method` on).
fn pat_ident_name(pat: &Pat) -> Option<String> {
    match pat {
        Pat::Ident(bi) => Some(bi.id.sym.to_string()),
        _ => None,
    }
}
