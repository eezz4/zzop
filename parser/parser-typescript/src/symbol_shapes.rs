//! `SourceSymbol` constructors and shape helpers shared by the symbol-extraction passes
//! (`symbols` / `factory` / `cjs_exports`).

use swc_core::common::{BytePos, SourceMap};
use swc_core::ecma::ast::{
    Callee, Class, ClassMember, Expr, Function, Lit, ObjectPatProp, Pat, PropName, VarDeclarator,
};
use zzop_core::{SourceSymbol, SourceSymbolKind};

use crate::line_of;

pub(crate) fn fn_symbol(
    cm: &SourceMap,
    file: &str,
    name: String,
    function: &Function,
    exported: bool,
    is_default: bool,
) -> SourceSymbol {
    let (body_start, body_end) = match &function.body {
        Some(b) => (Some(line_of(cm, b.span.lo)), Some(line_of(cm, b.span.hi))),
        None => (None, None),
    };
    SourceSymbol {
        id: format!("{file}#{name}"),
        file: file.into(),
        name,
        kind: SourceSymbolKind::Function,
        line: line_of(cm, function.span.lo),
        exported,
        is_default,
        body_start,
        body_end,
        write_sites: Vec::new(),
    }
}

fn class_symbol(
    cm: &SourceMap,
    file: &str,
    name: String,
    class: &Class,
    exported: bool,
    is_default: bool,
) -> SourceSymbol {
    let line = line_of(cm, class.span.lo);
    SourceSymbol {
        id: format!("{file}#{name}"),
        file: file.into(),
        name,
        kind: SourceSymbolKind::Class,
        line,
        exported,
        is_default,
        body_start: Some(line), // class bodyStart uses the node's own start line
        body_end: Some(line_of(cm, class.span.hi)),
        write_sites: Vec::new(),
    }
}

/// Class symbol + method sub-symbols (`Class.method`) — constructor/method/getter/setter/private-method
/// only, properties/computed/string-literal names skipped. Same-name pairs (e.g. get/set) emit once.
pub(crate) fn emit_class(
    cm: &SourceMap,
    file: &str,
    name: String,
    class: &Class,
    exported: bool,
    is_default: bool,
    out: &mut Vec<SourceSymbol>,
) {
    out.push(class_symbol(
        cm,
        file,
        name.clone(),
        class,
        exported,
        is_default,
    ));
    let mut seen = std::collections::HashSet::new();
    for member in &class.body {
        let (mname, lo, body_span) = match member {
            ClassMember::Constructor(c) => (
                "constructor".to_string(),
                c.span.lo,
                c.body.as_ref().map(|b| b.span),
            ),
            ClassMember::Method(m) => {
                let Some(n) = prop_name(&m.key) else { continue };
                (n, m.span.lo, m.function.body.as_ref().map(|b| b.span))
            }
            ClassMember::PrivateMethod(m) => (
                format!("#{}", m.key.name),
                m.span.lo,
                m.function.body.as_ref().map(|b| b.span),
            ),
            _ => continue, // properties / index signatures / etc.
        };
        if !seen.insert(mname.clone()) {
            continue;
        }
        let full = format!("{name}.{mname}");
        out.push(SourceSymbol {
            id: format!("{file}#{full}"),
            file: file.into(),
            name: full,
            kind: SourceSymbolKind::Function,
            line: line_of(cm, lo),
            exported: false,
            is_default: false,
            body_start: body_span.map(|s| line_of(cm, s.lo)),
            body_end: body_span.map(|s| line_of(cm, s.hi)),
            write_sites: Vec::new(),
        });
    }
}

/// PropName -> static name (Ident only; computed/string/num are not statically extractable -> None).
fn prop_name(key: &PropName) -> Option<String> {
    match key {
        PropName::Ident(i) => Some(i.sym.to_string()),
        _ => None,
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
