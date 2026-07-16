//! Static re-export (`export ... from`) and dynamic `import("literal")` specifier extraction.

use swc_core::ecma::ast::{CallExpr, Callee, ExportSpecifier, Expr, Lit, ModuleDecl, ModuleItem};
use swc_core::ecma::visit::{Visit, VisitWith};
use zzop_core::ReExport;

use crate::imports::export_name;
use crate::parse_module;

/// Extracts only `export { A as B } from "./y"` / `export * from "./y"` / `export * as ns from "./y"` — a
/// bare `export { x }` with no from-clause is a local declaration and is excluded. `type_only` is set from
/// the export clause's own `type_only`/`export type * from` flag AND, per named specifier, its own
/// per-specifier `export { type X } from "./y"` marker (mirrors `parse_imports`'s
/// `clause_type_only || n.is_type_only` combination). A type-only re-export is erased by TS at compile
/// time, so `lang::resolve::build_dep`/`build_dep_with_workspace` merge it into `resolved` as a real edge
/// (the target is still "used" — fan-in/dead-exports/metrics need it) but add it to the noncycle
/// exclusion set so it is never treated as a circular-dependency edge — the same treatment a type-only
/// import binding gets.
pub fn parse_re_exports(file: &str, source: &str) -> Vec<ReExport> {
    let mut out = Vec::new();
    let Some(module) = parse_module(file, source) else {
        return out;
    };
    for item in &module.body {
        let ModuleItem::ModuleDecl(decl) = item else {
            continue;
        };
        match decl {
            // `export * from "..."` / `export type * from "..."`
            ModuleDecl::ExportAll(all) => out.push(ReExport {
                specifier: all.src.value.as_str().unwrap_or_default().to_string(),
                original: "*".into(),
                local_alias: "*".into(),
                type_only: all.type_only,
            }),
            // `export { ... } from "..."` / `export * as ns from "..."` / `export type { ... } from "..."`
            ModuleDecl::ExportNamed(named) => {
                let Some(src) = &named.src else {
                    continue; // no from-clause -> local export, not a re-export
                };
                let specifier = src.value.as_str().unwrap_or_default().to_string();
                for spec in &named.specifiers {
                    match spec {
                        ExportSpecifier::Named(n) => {
                            let original = export_name(&n.orig);
                            let local_alias = n
                                .exported
                                .as_ref()
                                .map_or_else(|| original.clone(), export_name);
                            out.push(ReExport {
                                specifier: specifier.clone(),
                                original,
                                local_alias,
                                type_only: named.type_only || n.is_type_only,
                            });
                        }
                        ExportSpecifier::Namespace(ns) => out.push(ReExport {
                            specifier: specifier.clone(),
                            original: "*".into(),
                            local_alias: export_name(&ns.name),
                            type_only: named.type_only,
                        }),
                        ExportSpecifier::Default(_) => {}
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Extracts dynamic `import("./x")` / `await import("./x")` specifiers (recursive walk); which exports run at runtime is unknown, so the whole target file is treated as a wildcard.
pub fn parse_dynamic_imports(file: &str, source: &str) -> Vec<String> {
    let Some(module) = parse_module(file, source) else {
        return Vec::new();
    };
    let mut collector = DynImportCollector { out: Vec::new() };
    module.visit_with(&mut collector);
    collector.out
}

struct DynImportCollector {
    out: Vec<String>,
}

impl Visit for DynImportCollector {
    fn visit_call_expr(&mut self, call: &CallExpr) {
        if matches!(call.callee, Callee::Import(_)) {
            if let Some(first) = call.args.first() {
                if let Expr::Lit(Lit::Str(s)) = &*first.expr {
                    self.out
                        .push(s.value.as_str().unwrap_or_default().to_string());
                }
            }
        }
        call.visit_children_with(self); // recurse into nested calls (lazy(() => import()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- parseReExportsFromAst ---

    fn reexport(specifier: &str, original: &str, local_alias: &str) -> ReExport {
        reexport_ex(specifier, original, local_alias, false)
    }

    fn reexport_ex(
        specifier: &str,
        original: &str,
        local_alias: &str,
        type_only: bool,
    ) -> ReExport {
        ReExport {
            specifier: specifier.into(),
            original: original.into(),
            local_alias: local_alias.into(),
            type_only,
        }
    }

    #[test]
    fn named_re_export() {
        assert_eq!(
            parse_re_exports("x.ts", "export { A } from \"./a\";\n"),
            vec![reexport("./a", "A", "A")]
        );
    }

    #[test]
    fn aliased_re_export() {
        assert_eq!(
            parse_re_exports("x.ts", "export { A as B } from \"./a\";\n"),
            vec![reexport("./a", "A", "B")]
        );
    }

    #[test]
    fn star_re_export() {
        assert_eq!(
            parse_re_exports("x.ts", "export * from \"./a\";\n"),
            vec![reexport("./a", "*", "*")]
        );
    }

    #[test]
    fn namespace_re_export() {
        assert_eq!(
            parse_re_exports("x.ts", "export * as ns from \"./a\";\n"),
            vec![reexport("./a", "*", "ns")]
        );
    }

    #[test]
    fn bare_export_is_not_re_export() {
        assert_eq!(
            parse_re_exports("x.ts", "const x = 1; export { x };\n"),
            Vec::<ReExport>::new()
        );
    }

    #[test]
    fn type_only_named_re_export_clause() {
        // `export type { X } from "./y"` — clause-level type-only.
        assert_eq!(
            parse_re_exports("x.ts", "export type { X } from \"./a\";\n"),
            vec![reexport_ex("./a", "X", "X", true)]
        );
    }

    #[test]
    fn type_only_per_specifier_re_export() {
        // `export { type X, y } from "./a"` — only `X` is type-only; `y` is still a runtime re-export.
        let out = parse_re_exports("x.ts", "export { type X, y } from \"./a\";\n");
        assert_eq!(
            out,
            vec![
                reexport_ex("./a", "X", "X", true),
                reexport("./a", "y", "y"),
            ]
        );
    }

    #[test]
    fn type_only_star_re_export() {
        // `export type * from "./a"` — the whole re-export is type-only.
        assert_eq!(
            parse_re_exports("x.ts", "export type * from \"./a\";\n"),
            vec![reexport_ex("./a", "*", "*", true)]
        );
    }

    #[test]
    fn type_only_namespace_re_export() {
        // `export type * as ns from "./a"` — clause-level type-only applies to the namespace alias too.
        assert_eq!(
            parse_re_exports("x.ts", "export type * as ns from \"./a\";\n"),
            vec![reexport_ex("./a", "*", "ns", true)]
        );
    }

    // --- parseDynamicImportsFromAst ---

    #[test]
    fn dynamic_import_single() {
        assert_eq!(
            parse_dynamic_imports("x.ts", "const m = import(\"./x\");\n"),
            vec!["./x".to_string()]
        );
    }

    #[test]
    fn await_import_in_chain_captured() {
        assert_eq!(
            parse_dynamic_imports(
                "x.ts",
                "async function f() { const m = await import(\"./deep\"); }\n"
            ),
            vec!["./deep".to_string()]
        );
    }

    #[test]
    fn lazy_import_multiple() {
        assert_eq!(
            parse_dynamic_imports(
                "x.ts",
                "const A = lazy(() => import(\"./a\"));\nconst B = lazy(() => import(\"./b\"));\n"
            ),
            vec!["./a".to_string(), "./b".to_string()]
        );
    }

    #[test]
    fn non_string_literal_argument_skipped() {
        assert_eq!(
            parse_dynamic_imports("x.ts", "const n = \"x\"; const m = import(n);\n"),
            Vec::<String>::new()
        );
    }
}
