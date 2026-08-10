//! `SourceSymbol` constructors and shape helpers shared by the symbol-extraction passes
//! (`symbols` / `factory` / `cjs_exports`).

use swc_core::common::{BytePos, SourceMap};
use swc_core::ecma::ast::{Callee, Expr, Function, Lit, ObjectPatProp, Pat, VarDeclarator};
use zzop_core::{SourceSymbol, SourceSymbolKind};

use crate::line_of;

mod class;

pub(crate) use class::emit_class;

pub(crate) fn fn_symbol(
    cm: &SourceMap,
    file: &str,
    name: String,
    function: &Function,
    exported: bool,
    is_default: bool,
) -> SourceSymbol {
    // `body_start` is the DECLARATION's line, not the body block's — `zzop_core::SourceSymbol`'s "Body
    // span contract". An overload signature (no `body`) keeps `None`/`None`: there is no region.
    let line = line_of(cm, function.span.lo);
    let (body_start, body_end) = match &function.body {
        Some(b) => (Some(line), Some(line_of(cm, b.span.hi))),
        None => (None, None),
    };
    SourceSymbol {
        id: format!("{file}#{name}"),
        file: file.into(),
        name,
        kind: SourceSymbolKind::Function,
        line,
        exported,
        is_default,
        body_start,
        body_end,
        write_sites: Vec::new(),
    }
}

pub(crate) fn simple_symbol(
    cm: &SourceMap,
    file: &str,
    name: String,
    kind: SourceSymbolKind,
    lo: BytePos,
    exported: bool,
) -> SourceSymbol {
    SourceSymbol {
        id: format!("{file}#{name}"),
        file: file.into(),
        name,
        kind,
        line: line_of(cm, lo),
        exported,
        is_default: false,
        body_start: None,
        body_end: None,
        write_sites: Vec::new(),
    }
}

/// A `require('...')` / `require('...').x` initializer — a CJS import alias (not a declared symbol).
pub(crate) fn is_require_init(d: &VarDeclarator) -> bool {
    let Some(e) = d.init.as_deref() else {
        return false;
    };
    let e = if let Expr::Member(m) = e { &*m.obj } else { e };
    let Expr::Call(c) = e else {
        return false;
    };
    let Callee::Expr(callee) = &c.callee else {
        return false;
    };
    let Expr::Ident(id) = &**callee else {
        return false;
    };
    id.sym == "require"
        && c.args
            .first()
            .is_some_and(|a| matches!(&*a.expr, Expr::Lit(Lit::Str(_))))
}

/// Flattens a binding pattern (`{a, b}` / `[x]`, incl. nested) into its bound identifier names, in source order (omitted array slots and rest elements' own patterns are handled).
pub(crate) fn collect_binding_names(pat: &Pat) -> Vec<String> {
    let mut names = Vec::new();
    collect_binding_names_into(pat, &mut names);
    names
}

fn collect_binding_names_into(pat: &Pat, out: &mut Vec<String>) {
    match pat {
        Pat::Ident(bi) => out.push(bi.id.sym.to_string()),
        Pat::Array(a) => {
            for elem in a.elems.iter().flatten() {
                collect_binding_names_into(elem, out);
            }
        }
        Pat::Object(o) => {
            for prop in &o.props {
                match prop {
                    ObjectPatProp::Assign(a) => out.push(a.key.id.sym.to_string()),
                    ObjectPatProp::KeyValue(kv) => collect_binding_names_into(&kv.value, out),
                    ObjectPatProp::Rest(r) => collect_binding_names_into(&r.arg, out),
                }
            }
        }
        Pat::Rest(r) => collect_binding_names_into(&r.arg, out),
        Pat::Assign(a) => collect_binding_names_into(&a.left, out),
        Pat::Invalid(_) | Pat::Expr(_) => {}
    }
}
