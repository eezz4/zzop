//! CommonJS `require("literal")` binding collection — the tree-walking counterpart to `imports`'s
//! ESM declaration parsing (`parse_imports` runs this collector after the ESM pass).

use swc_core::ecma::ast::{
    ArrowExpr, CallExpr, Callee, ClassMethod, Constructor, Expr, FnDecl, FnExpr, GetterProp, Lit,
    MethodProp, ObjectPatProp, Pat, PrivateMethod, PropName, SetterProp, VarDeclarator,
};
use swc_core::ecma::visit::{Visit, VisitWith};
use zzop_core::{ImportBinding, ImportMap};

/// Walks the whole tree collecting CommonJS `require("literal")` bindings.
/// - `const X = require("./y")` -> X bound as a namespace ("*") of the module.
/// - `const { a, b: c } = require("./y")` -> a / c bound to their original export names.
/// - inline `require("./y").foo()` / bare `require("./y")` -> a synthetic key so the edge still enters the graph.
///
/// `deferred` tracks whether the require sits inside a function/method/accessor body (lazy — no load-order edge).
pub(crate) struct RequireCollector<'a> {
    pub(crate) map: &'a mut ImportMap,
    pub(crate) deferred: bool,
    pub(crate) side_effect_seq: u32,
}

impl RequireCollector<'_> {
    /// Runs `f` with `deferred` forced true for its duration — used when descending into a function-like scope.
    fn with_deferred(&mut self, f: impl FnOnce(&mut Self)) {
        let prev = self.deferred;
        self.deferred = true;
        f(self);
        self.deferred = prev;
    }
}

impl Visit for RequireCollector<'_> {
    fn visit_var_declarator(&mut self, d: &VarDeclarator) {
        if let Some(Expr::Call(call)) = d.init.as_deref() {
            if let Some(specifier) = require_specifier(call) {
                bind_require_target(&d.name, &specifier, self.deferred, self.map);
                return; // fully handled — require("literal")'s own subtree has nothing further of interest
            }
        }
        d.visit_children_with(self);
    }

    fn visit_call_expr(&mut self, call: &CallExpr) {
        if let Some(specifier) = require_specifier(call) {
            let key = format!("__require{}__", self.side_effect_seq);
            self.side_effect_seq += 1;
            self.map.insert(
                key,
                ImportBinding {
                    specifier,
                    original: "*".into(),
                    deferred: self.deferred,
                    type_only: false,
                },
            );
        }
        call.visit_children_with(self);
    }

    fn visit_fn_decl(&mut self, n: &FnDecl) {
        self.with_deferred(|c| n.visit_children_with(c));
    }
    fn visit_fn_expr(&mut self, n: &FnExpr) {
        self.with_deferred(|c| n.visit_children_with(c));
    }
    fn visit_arrow_expr(&mut self, n: &ArrowExpr) {
        self.with_deferred(|c| n.visit_children_with(c));
    }
    fn visit_class_method(&mut self, n: &ClassMethod) {
        self.with_deferred(|c| n.visit_children_with(c));
    }
    fn visit_private_method(&mut self, n: &PrivateMethod) {
        self.with_deferred(|c| n.visit_children_with(c));
    }
    fn visit_constructor(&mut self, n: &Constructor) {
        self.with_deferred(|c| n.visit_children_with(c));
    }
    fn visit_getter_prop(&mut self, n: &GetterProp) {
        self.with_deferred(|c| n.visit_children_with(c));
    }
    fn visit_setter_prop(&mut self, n: &SetterProp) {
        self.with_deferred(|c| n.visit_children_with(c));
    }
    fn visit_method_prop(&mut self, n: &MethodProp) {
        self.with_deferred(|c| n.visit_children_with(c));
    }
}

/// The literal specifier of a `require("literal")` call, or None.
fn require_specifier(call: &CallExpr) -> Option<String> {
    let Callee::Expr(callee) = &call.callee else {
        return None;
    };
    let Expr::Ident(id) = &**callee else {
        return None;
    };
    if id.sym != "require" {
        return None;
    }
    let arg = call.args.first()?;
    match &*arg.expr {
        Expr::Lit(Lit::Str(s)) => Some(s.value.as_str().unwrap_or_default().to_string()),
        _ => None,
    }
}

/// Binds the LHS of a `... = require(spec)` declarator — identifier as namespace, or destructured names (array-destructured requires are not handled).
fn bind_require_target(name: &Pat, specifier: &str, deferred: bool, map: &mut ImportMap) {
    match name {
        Pat::Ident(bi) => {
            map.insert(
                bi.id.sym.to_string(),
                ImportBinding {
                    specifier: specifier.into(),
                    original: "*".into(),
                    deferred,
                    type_only: false,
                },
            );
        }
        Pat::Object(obj) => {
            for prop in &obj.props {
                match prop {
                    // `{ a }` — shorthand: local name doubles as the original export name.
                    ObjectPatProp::Assign(a) => {
                        let local = a.key.id.sym.to_string();
                        map.insert(
                            local.clone(),
                            ImportBinding {
                                specifier: specifier.into(),
                                original: local,
                                deferred,
                                type_only: false,
                            },
                        );
                    }
                    // `{ b: c }` — c is the local name, b is the original export name.
                    ObjectPatProp::KeyValue(kv) => {
                        let Pat::Ident(local) = &*kv.value else {
                            continue;
                        };
                        let PropName::Ident(original) = &kv.key else {
                            continue;
                        };
                        map.insert(
                            local.id.sym.to_string(),
                            ImportBinding {
                                specifier: specifier.into(),
                                original: original.sym.to_string(),
                                deferred,
                                type_only: false,
                            },
                        );
                    }
                    ObjectPatProp::Rest(_) => {}
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use crate::parse_imports;
    use crate::test_util::binding;

    // --- parseImports CommonJS require() ---

    #[test]
    fn top_level_var_require_binds_namespace() {
        let m = parse_imports("x.js", "var X = require('./y');\n");
        assert_eq!(m["X"], binding("./y", "*", false));
        assert!(!m["X"].deferred);
    }

    #[test]
    fn destructured_require_binds_original_names() {
        let m = parse_imports("x.js", "const { a, b: c } = require('./y');\n");
        assert_eq!(m["a"].specifier, "./y");
        assert_eq!(m["a"].original, "a");
        assert!(!m["a"].deferred);
        assert_eq!(m["c"].specifier, "./y");
        assert_eq!(m["c"].original, "b");
        assert!(!m["c"].deferred);
    }

    #[test]
    fn require_inside_function_body_is_deferred() {
        let m = parse_imports("x.js", "function f(){ require('./y').go(); }\n");
        assert_eq!(m.len(), 1);
        let entry = m.values().next().unwrap();
        assert_eq!(entry.specifier, "./y");
        assert!(entry.deferred);
    }

    #[test]
    fn top_level_and_nested_require_of_same_target_keep_both_flags() {
        let m = parse_imports(
            "x.js",
            "var Sleeping = require('../core/Sleeping');\nfunction set(){ require('../core/Sleeping').set(); }\n",
        );
        let deferred_flags: Vec<bool> = m.values().map(|b| b.deferred).collect();
        assert!(deferred_flags.contains(&false));
        assert!(deferred_flags.contains(&true));
    }

    #[test]
    fn bare_side_effect_require_records_edge() {
        let m = parse_imports("x.js", "require('./y');\n");
        assert_eq!(m.len(), 1);
        assert_eq!(m.values().next().unwrap().specifier, "./y");
    }
}
