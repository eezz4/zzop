//! CommonJS export-symbol extraction (`module.exports` / `exports.x`) — the counterpart to ESM
//! `export` parsing in `symbols`.

use std::collections::HashSet;

use swc_core::common::{SourceMap, Spanned};
use swc_core::ecma::ast::{
    AssignExpr, AssignOp, AssignTarget, BlockStmtOrExpr, Expr, MemberExpr, MemberProp, Module,
    ObjectLit, Prop, PropName, PropOrSpread, SimpleAssignTarget,
};
use swc_core::ecma::visit::{Visit, VisitWith};
use zzop_core::{SourceSymbol, SourceSymbolKind};

use crate::line_of;

/// CommonJS export-symbol extraction — the counterpart to ESM `export` parsing for `module.exports` /
/// `exports.x`. Recovers exports from the common `var Body = {}; module.exports = Body;
/// Body.create = ...;` shape, named by their bare member name, so symbol risk/hotspots/cycles aren't empty for CJS files.
pub(crate) fn collect_common_js_exports(
    cm: &SourceMap,
    file: &str,
    module: &Module,
) -> Vec<SourceSymbol> {
    let mut names = ExportObjNameCollector {
        names: HashSet::new(),
    };
    module.visit_with(&mut names);
    let mut collector = CjsExportCollector {
        cm,
        file,
        export_obj_names: names.names,
        seen: HashSet::new(),
        out: Vec::new(),
    };
    module.visit_with(&mut collector);
    collector.out
}

/// Identifiers assigned to `module.exports` (the module's export object) — e.g. `Body` in `module.exports = Body`.
struct ExportObjNameCollector {
    names: HashSet<String>,
}

impl Visit for ExportObjNameCollector {
    fn visit_assign_expr(&mut self, n: &AssignExpr) {
        if n.op == AssignOp::Assign {
            if let AssignTarget::Simple(SimpleAssignTarget::Member(m)) = &n.left {
                if is_module_exports(m) {
                    if let Expr::Ident(rhs) = &*n.right {
                        self.names.insert(rhs.sym.to_string());
                    }
                }
            }
        }
        n.visit_children_with(self);
    }
}

struct CjsExportCollector<'a> {
    cm: &'a SourceMap,
    file: &'a str,
    export_obj_names: HashSet<String>,
    seen: HashSet<String>,
    out: Vec<SourceSymbol>,
}

impl CjsExportCollector<'_> {
    fn add(&mut self, name: String, line: u32, rhs: Option<&Expr>) {
        if name.is_empty() || self.seen.contains(&name) {
            return;
        }
        self.seen.insert(name.clone());
        self.out
            .push(build_member_symbol(self.cm, self.file, &name, line, rhs));
    }
}

impl Visit for CjsExportCollector<'_> {
    fn visit_assign_expr(&mut self, n: &AssignExpr) {
        if n.op == AssignOp::Assign {
            if let AssignTarget::Simple(SimpleAssignTarget::Member(m)) = &n.left {
                // `module.exports = { ... }` — each property is an export.
                if is_module_exports(m) {
                    if let Expr::Object(obj) = &*n.right {
                        for (name, line, rhs) in object_literal_member_names(self.cm, obj) {
                            self.add(name, line, rhs);
                        }
                    }
                }
                if let Some(member) = export_member_name(m, &self.export_obj_names) {
                    self.add(member, line_of(self.cm, n.span.lo), Some(&n.right));
                }
            }
        }
        n.visit_children_with(self);
    }
}

/// `module.exports` property access.
fn is_module_exports(m: &MemberExpr) -> bool {
    matches!(&*m.obj, Expr::Ident(id) if id.sym == "module")
        && matches!(&m.prop, MemberProp::Ident(name) if name.sym == "exports")
}

/// The exported member name for an assignment LHS, or None when the LHS is not an export member. Handles `exports.x`, `module.exports.x`, and `<exportObj>.x`.
fn export_member_name(m: &MemberExpr, export_obj_names: &HashSet<String>) -> Option<String> {
    let MemberProp::Ident(name) = &m.prop else {
        return None;
    };
    match &*m.obj {
        Expr::Ident(recv) => {
            if recv.sym == "exports" || export_obj_names.contains(recv.sym.as_str()) {
                Some(name.sym.to_string())
            } else {
                None
            }
        }
        Expr::Member(inner) if is_module_exports(inner) => Some(name.sym.to_string()),
        _ => None,
    }
}

fn object_literal_member_names<'e>(
    cm: &SourceMap,
    obj: &'e ObjectLit,
) -> Vec<(String, u32, Option<&'e Expr>)> {
    let mut out = Vec::new();
    for p in &obj.props {
        let PropOrSpread::Prop(prop) = p else {
            continue;
        };
        match &**prop {
            Prop::KeyValue(kv) => {
                if let PropName::Ident(id) = &kv.key {
                    out.push((
                        id.sym.to_string(),
                        line_of(cm, id.span.lo),
                        Some(&*kv.value),
                    ));
                }
            }
            Prop::Shorthand(id) => out.push((id.sym.to_string(), line_of(cm, id.span.lo), None)),
            Prop::Method(mp) => {
                if let PropName::Ident(id) = &mp.key {
                    out.push((id.sym.to_string(), line_of(cm, id.span.lo), None));
                }
            }
            _ => {}
        }
    }
    out
}

fn build_member_symbol(
    cm: &SourceMap,
    file: &str,
    name: &str,
    line: u32,
    rhs: Option<&Expr>,
) -> SourceSymbol {
    let (is_fn, body_start, body_end) = match rhs {
        Some(Expr::Fn(f)) => (
            true,
            f.function.body.as_ref().map(|b| line_of(cm, b.span.lo)),
            f.function.body.as_ref().map(|b| line_of(cm, b.span.hi)),
        ),
        Some(Expr::Arrow(a)) => {
            let (lo, hi) = match &*a.body {
                BlockStmtOrExpr::BlockStmt(b) => (b.span.lo, b.span.hi),
                BlockStmtOrExpr::Expr(e) => (e.span().lo, e.span().hi),
            };
            (true, Some(line_of(cm, lo)), Some(line_of(cm, hi)))
        }
        _ => (false, None, None),
    };
    SourceSymbol {
        id: format!("{file}#{name}"),
        file: file.into(),
        name: name.into(),
        kind: if is_fn {
            SourceSymbolKind::Function
        } else {
            SourceSymbolKind::Const
        },
        line,
        exported: true,
        is_default: false,
        body_start,
        body_end,
        write_sites: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use crate::parse_symbols;
    use zzop_core::SourceSymbolKind as K;

    // --- parseSymbols CommonJS exports (module.exports / exports.x) ---

    #[test]
    fn cjs_module_exports_obj_then_member_assignment_bare_name() {
        let src =
            "var Body = {};\nmodule.exports = Body;\nBody.create = function (o) { return o; };\n";
        let s = parse_symbols("body/Body.js", src);
        let create = s
            .iter()
            .find(|s| s.name == "create")
            .expect("create exported");
        assert_eq!(create.id, "body/Body.js#create");
        assert_eq!(create.kind, K::Function);
        assert!(create.exported);
        assert!(create.body_start.is_some());
    }

    #[test]
    fn cjs_members_inside_iife_are_found() {
        let src = "var Body = {};\nmodule.exports = Body;\n(function () { Body.update = function () {}; Body.scale = function () {}; })();\n";
        let syms = parse_symbols("x.js", src);
        assert!(syms.iter().any(|s| s.name == "update"));
        assert!(syms.iter().any(|s| s.name == "scale"));
    }

    #[test]
    fn cjs_exports_x_and_module_exports_y_are_members() {
        let src = "exports.foo = function () {};\nmodule.exports.bar = 1;\n";
        let syms = parse_symbols("x.js", src);
        assert!(syms.iter().any(|s| s.name == "foo"));
        assert!(syms.iter().any(|s| s.name == "bar"));
    }

    #[test]
    fn cjs_module_exports_object_literal_members() {
        let src = "function fn(){}\nmodule.exports = { a: 1, b: fn };\n";
        let syms = parse_symbols("x.js", src);
        assert!(syms.iter().any(|s| s.name == "a"));
        assert!(syms.iter().any(|s| s.name == "b"));
    }

    #[test]
    fn cjs_non_export_object_property_assignment_ignored() {
        let src = "var local = {};\nlocal.foo = function () {};\n";
        let syms = parse_symbols("x.js", src);
        assert!(!syms.iter().any(|s| s.name == "foo"));
    }

    #[test]
    fn cjs_require_alias_vars_not_symbols_export_members_are() {
        let src = "var Vector = require('../geometry/Vector');\nvar Body = {};\nmodule.exports = Body;\nBody.create = function () {};\n";
        let syms = parse_symbols("body/Body.js", src);
        assert!(!syms.iter().any(|s| s.name == "Vector")); // import alias, not a symbol
        assert!(syms.iter().any(|s| s.name == "create")); // CJS export member
    }
}
