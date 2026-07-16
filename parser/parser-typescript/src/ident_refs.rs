//! Local identifier-reference collection — feeds dead-export analysis (exported but only
//! referenced within the same file).

use swc_core::ecma::ast::{
    FnDecl, Ident, ImportDecl, NamedExport, Prop, TsEnumDecl, TsEnumMember, TsInterfaceDecl,
    TsTypeAliasDecl,
};
use swc_core::ecma::visit::{Visit, VisitWith};

use crate::parse_module;

/// Collects all identifier references in a file, excluding import/export declarations, member-access
/// property names, and declaration names — used by dead-export analysis to find symbols that are
/// exported but only referenced within the same file. Each name appears once; scope shadowing is ignored
/// (names are compared statically only).
/// Property-like names are excluded for free (swc types them `IdentName`/`PropName`, not `Ident`, so they
/// never reach `visit_ident`); binding names are excluded via `visit_binding_ident`. Destructuring-
/// *assignment* targets (`[a, b] = arr;`) reuse the declaration `Pat` shape and are excluded too, even
/// though they're plain references rather than bindings — untested either way.
pub fn parse_local_identifier_refs(file: &str, source: &str) -> std::collections::BTreeSet<String> {
    let Some(module) = parse_module(file, source) else {
        return std::collections::BTreeSet::new();
    };
    let mut collector = LocalRefCollector {
        refs: std::collections::BTreeSet::new(),
    };
    module.visit_with(&mut collector);
    collector.refs
}

struct LocalRefCollector {
    refs: std::collections::BTreeSet<String>,
}

impl Visit for LocalRefCollector {
    fn visit_ident(&mut self, node: &Ident) {
        self.refs.insert(node.sym.to_string());
    }

    // Import/export declarations bind names — not references.
    fn visit_import_decl(&mut self, _n: &ImportDecl) {}
    fn visit_named_export(&mut self, _n: &NamedExport) {}

    // `id` here is the declared binding name (var/param/destructuring) — not a reference. Type annotations
    // may themselves reference other identifiers/types, so those are still visited.
    fn visit_binding_ident(&mut self, n: &swc_core::ecma::ast::BindingIdent) {
        if let Some(ann) = &n.type_ann {
            ann.visit_with(self);
        }
    }

    fn visit_fn_decl(&mut self, n: &FnDecl) {
        n.function.visit_with(self); // skip n.ident (declaration name)
    }
    fn visit_class_decl(&mut self, n: &swc_core::ecma::ast::ClassDecl) {
        n.class.visit_with(self); // skip n.ident (declaration name)
    }
    fn visit_ts_interface_decl(&mut self, n: &TsInterfaceDecl) {
        if let Some(tp) = &n.type_params {
            tp.visit_with(self);
        }
        n.extends.visit_with(self);
        n.body.visit_with(self); // skip n.id (declaration name)
    }
    fn visit_ts_type_alias_decl(&mut self, n: &TsTypeAliasDecl) {
        if let Some(tp) = &n.type_params {
            tp.visit_with(self);
        }
        n.type_ann.visit_with(self); // skip n.id (declaration name)
    }
    fn visit_ts_enum_decl(&mut self, n: &TsEnumDecl) {
        n.members.visit_with(self); // skip n.id (declaration name)
    }
    fn visit_ts_enum_member(&mut self, n: &TsEnumMember) {
        if let Some(init) = &n.init {
            init.visit_with(self); // skip n.id (declaration name / string literal)
        }
    }
    fn visit_prop(&mut self, n: &Prop) {
        match n {
            // `{ x }` — shorthand property name; excluded (not a name -> value reference).
            Prop::Shorthand(_) => {}
            other => other.visit_children_with(self),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- parseLocalIdentifierRefsFromAst ---

    #[test]
    fn local_identifier_refs_excludes_declarations_and_property_names() {
        let refs = parse_local_identifier_refs(
            "x.ts",
            "const X = 1;\nfunction foo() { return X + Y; }\nconst obj = { bar: 2 };\nconst z = obj.bar;\n",
        );
        assert!(refs.contains("X"));
        assert!(refs.contains("Y"));
        assert!(refs.contains("obj"));
        // X's own decl name, foo's own decl name, and `bar` (property key + property access) are excluded.
        assert!(!refs.contains("foo"));
        assert!(!refs.contains("bar"));
    }

    #[test]
    fn local_identifier_refs_excludes_import_export_names() {
        let refs = parse_local_identifier_refs(
            "x.ts",
            "import { A, B } from \"m\";\nexport { A } from \"m\";\nconst c = A + B;\n",
        );
        // only A / B referenced in `const c = A + B` should appear — import/export declarations are skipped.
        let got: Vec<&String> = refs.iter().collect();
        assert_eq!(got, vec!["A", "B"]);
    }

    #[test]
    fn local_identifier_refs_dedups_repeated_reference() {
        let refs = parse_local_identifier_refs(
            "x.ts",
            "const X = 1;\nfunction f() { return X + X + X; }\n",
        );
        assert!(refs.contains("X"));
        assert_eq!(refs.len(), 1);
    }
}
