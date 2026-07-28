//! Local export RENAMES — `export { X as Y }` / `export type { X as Y }` with NO from-clause.
//!
//! ## Why this is a separate fact
//! The two existing projectors both drop the public name on the floor, each for a good local reason:
//! - `re_exports.rs` skips a from-less export clause entirely ("not a re-export" — correct, there is
//!   no target module to resolve).
//! - `symbols.rs`' deferred-export pass sees the same clause but only flips the LOCAL declaration's
//!   `exported` flag; a `SourceSymbol` is keyed by its declaration name, so `Y` has nowhere to live.
//!
//! Nobody was left holding the local -> public mapping, and `unimported-export` matches an importer's
//! `file#Y` key against a candidate named `X`: the link breaks and a live export reads as dead.
//! Measured on mono-hub: `interface State` + `export type { State as MortgageState }` with four
//! importers of `MortgageState`, reported `in-file-only`.
//!
//! Scope: only a genuine RENAME is emitted. `export { X }` publishes `X` under its own name, which
//! every consumer already matches, so emitting it would be pure noise. `export { X } from "./y"`
//! stays `re_exports.rs`' business — that clause republishes someone ELSE's declaration.

use swc_core::ecma::ast::{ExportSpecifier, Module, ModuleDecl, ModuleItem};

use crate::imports::export_name;

/// `(local declaration name, public export name)` for every from-less rename in `module`, in source
/// order. `export { X as default }` is included: `default` is just another public name, and emitting
/// it keeps this fact complete rather than relying on a second mechanism.
///
/// Takes an ALREADY-PARSED `Module` because its one caller, `crate::dead_export_facts`, wants three
/// facts out of one parse. There is deliberately no `(file, source)` entrypoint beside it: the
/// standalone `parse_local_export_aliases` shell lost its last non-test caller when that bundle
/// landed, and a public entrypoint nobody calls is surface to keep true, not capability. (Its two
/// siblings `parse_re_exports`/`parse_dynamic_imports` keep theirs — `project.rs`'s Common-IR build
/// and `pipeline::fresh`'s projector table still call them by that signature.) The graceful degrade
/// on an unparseable file therefore lives in the bundle, which owns the `parse_module` call.
pub(crate) fn local_export_aliases_from_module(module: &Module) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for item in &module.body {
        let ModuleItem::ModuleDecl(ModuleDecl::ExportNamed(named)) = item else {
            continue;
        };
        if named.src.is_some() {
            continue; // `export { X as Y } from "./z"` — a re-export, not this file's declaration
        }
        for spec in &named.specifiers {
            // Only `Named` carries an `orig`/`exported` pair. A from-less clause cannot legally hold
            // a `Namespace` (`export * as ns` needs a source) or a `Default` specifier.
            let ExportSpecifier::Named(n) = spec else {
                continue;
            };
            let Some(exported) = n.exported.as_ref() else {
                continue; // `export { X }` — published under its own name, no mapping needed
            };
            let local = export_name(&n.orig);
            let public = export_name(exported);
            if public != local {
                out.push((local, public));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_module;

    // The unparseable-source case is NOT here: this walk takes a `Module`, so that degrade belongs to
    // whoever calls `parse_module` — `dead_export_facts`, which pins it on the same `function f( {`
    // source it always did.
    fn aliases(source: &str) -> Vec<(String, String)> {
        let module = parse_module("x.ts", source).expect("fixture must parse");
        local_export_aliases_from_module(&module)
    }

    fn pair(local: &str, public: &str) -> (String, String) {
        (local.to_string(), public.to_string())
    }

    #[test]
    fn value_rename() {
        assert_eq!(
            aliases("const x = 1;\nexport { x as publicX };\n"),
            vec![pair("x", "publicX")]
        );
    }

    #[test]
    fn type_only_rename() {
        // The measured mono-hub shape: `interface State` + `export type { State as MortgageState }`.
        assert_eq!(
            aliases("interface State { a: number }\nexport type { State as MortgageState };\n"),
            vec![pair("State", "MortgageState")]
        );
    }

    #[test]
    fn per_specifier_type_only_rename() {
        assert_eq!(
            aliases("interface S {}\nexport { type S as PublicS };\n"),
            vec![pair("S", "PublicS")]
        );
    }

    #[test]
    fn rename_to_default_is_included() {
        assert_eq!(
            aliases("function f() {}\nexport { f as default };\n"),
            vec![pair("f", "default")]
        );
    }

    #[test]
    fn several_renames_in_one_clause() {
        assert_eq!(
            aliases("const a = 1, b = 2;\nexport { a as A, b as B };\n"),
            vec![pair("a", "A"), pair("b", "B")]
        );
    }

    #[test]
    fn non_rename_export_emits_nothing() {
        assert_eq!(aliases("const x = 1;\nexport { x };\n"), Vec::new());
    }

    #[test]
    fn inline_export_emits_nothing() {
        assert_eq!(aliases("export const x = 1;\n"), Vec::new());
    }

    #[test]
    fn re_export_with_from_clause_is_not_a_local_alias() {
        // `re_exports.rs` owns this shape — it already carries `original`/`local_alias`, and the
        // rule resolves it through the re-export chain, not through this map.
        assert_eq!(aliases("export { A as B } from \"./a\";\n"), Vec::new());
        assert_eq!(aliases("export * as ns from \"./a\";\n"), Vec::new());
    }

    #[test]
    fn string_literal_export_name() {
        // `export { x as "public name" }` — arbitrary module-namespace names are valid ESM.
        assert_eq!(
            aliases("const x = 1;\nexport { x as \"public name\" };\n"),
            vec![pair("x", "public name")]
        );
    }
}
