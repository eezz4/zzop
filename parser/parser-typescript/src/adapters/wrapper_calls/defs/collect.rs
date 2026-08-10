//! AST collectors for [`super`]: the two top-level scans `classify_def` runs against — every
//! function-like binding (its one-hop sink lookup) and every binding whose initializer states an
//! absolute URL (its external-host veto). Split out of `defs.rs` because the pair would exceed the
//! 300-line file budget; the classifier itself stays there.

use swc_core::common::{SourceMap, SourceMapper, Spanned};
use swc_core::ecma::ast::{
    BlockStmtOrExpr, Decl, Expr, FnDecl, Module, ModuleDecl, ModuleItem, Pat, Stmt, VarDecl,
};

/// Every top-level function-like binding — declarations and `const` arrow/function expressions —
/// regardless of export status, keyed to its body text. Feeds `reaches_sink`'s one-hop check: the
/// sink-holding helper is typically NOT exported, so both must be collected the same way.
pub(crate) fn collect_top_level_functions(
    module: &Module,
    cm: &SourceMap,
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for item in &module.body {
        match item {
            ModuleItem::Stmt(Stmt::Decl(Decl::Fn(f))) => push_fn_decl(f, cm, &mut out),
            ModuleItem::Stmt(Stmt::Decl(Decl::Var(v))) => collect_var_fns(v, cm, &mut out),
            ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(export)) => match &export.decl {
                Decl::Fn(f) => push_fn_decl(f, cm, &mut out),
                Decl::Var(v) => collect_var_fns(v, cm, &mut out),
                _ => {}
            },
            _ => {}
        }
    }
    out
}

/// Every top-level binding whose initializer TEXT states an absolute URL — the names that make the
/// external-host veto in [`classify_def`] reachable one hop later. Collected the same way as
/// [`collect_top_level_functions`] (declaration or export, either form) so the two agree on what
/// "top level" means.
///
/// The initializer is read as SNIPPET rather than walked as AST on purpose: this feeds a veto, and a
/// veto's failure directions are not symmetric. Over-matching costs recall on one wrapper; under-
/// matching mints an internal-looking consume key for a call that leaves the system, which is the
/// defect this exists to stop. A string literal, a no-substitution template, a `+`-chain and a config
/// object that merely CONTAINS a URL all read alike here, and all four are fine to veto.
pub(crate) fn collect_absolute_url_consts(module: &Module, cm: &SourceMap) -> Vec<String> {
    let mut out = Vec::new();
    for item in &module.body {
        match item {
            ModuleItem::Stmt(Stmt::Decl(Decl::Var(v))) => collect_url_vars(v, cm, &mut out),
            ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(export)) => {
                if let Decl::Var(v) = &export.decl {
                    collect_url_vars(v, cm, &mut out);
                }
            }
            _ => {}
        }
    }
    out
}

fn collect_url_vars(v: &VarDecl, cm: &SourceMap, out: &mut Vec<String>) {
    for d in &v.decls {
        let Pat::Ident(bi) = &d.name else { continue };
        let Some(init) = d.init.as_deref() else {
            continue;
        };
        // A function-valued binding is a wrapper candidate, not a URL binding — its body is scanned
        // as a sink by the veto itself, so folding it in here would double-count and, worse, veto
        // every caller of any helper whose body mentions a URL in a comment.
        if matches!(init, Expr::Arrow(_) | Expr::Fn(_)) {
            continue;
        }
        let text = cm.span_to_snippet(init.span()).unwrap_or_default();
        if states_absolute_url(&text) {
            out.push(bi.id.sym.to_string());
        }
    }
}

pub(super) fn states_absolute_url(text: &str) -> bool {
    text.contains("http://") || text.contains("https://")
}

/// Does `text` reference `name` as a whole identifier? Used only by the external-host veto, where a
/// bare `contains` would let `baseline` shadow `base` — and shadowing in THIS direction is the unsafe
/// one, because it decides whether an absolute-URL binding is considered reached.
pub(super) fn mentions_identifier(text: &str, name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let is_ident_char = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    let bytes = text.as_bytes();
    let mut from = 0usize;
    while let Some(rel) = text[from..].find(name) {
        let start = from + rel;
        let end = start + name.len();
        let left_ok = start == 0 || !is_ident_char(text[..start].chars().next_back().unwrap());
        let right_ok = end == bytes.len() || !is_ident_char(text[end..].chars().next().unwrap());
        if left_ok && right_ok {
            return true;
        }
        from = start + name.len();
    }
    false
}

fn push_fn_decl(f: &FnDecl, cm: &SourceMap, out: &mut Vec<(String, String)>) {
    if let Some(body) = &f.function.body {
        out.push((
            f.ident.sym.to_string(),
            cm.span_to_snippet(body.span).unwrap_or_default(),
        ));
    }
}

fn collect_var_fns(v: &VarDecl, cm: &SourceMap, out: &mut Vec<(String, String)>) {
    for d in &v.decls {
        let Pat::Ident(bi) = &d.name else { continue };
        let Some(init) = d.init.as_deref() else {
            continue;
        };
        match init {
            Expr::Arrow(a) => {
                let span = match &*a.body {
                    BlockStmtOrExpr::BlockStmt(b) => b.span,
                    BlockStmtOrExpr::Expr(e) => e.span(),
                };
                out.push((
                    bi.id.sym.to_string(),
                    cm.span_to_snippet(span).unwrap_or_default(),
                ));
            }
            Expr::Fn(f) => {
                if let Some(body) = &f.function.body {
                    out.push((
                        bi.id.sym.to_string(),
                        cm.span_to_snippet(body.span).unwrap_or_default(),
                    ));
                }
            }
            _ => {}
        }
    }
}
