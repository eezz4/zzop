//! ESM `import` declaration bindings -> `ImportMap` (plus the CommonJS `require` walk, which lives
//! in `cjs_require`).

use swc_core::ecma::ast::{ImportSpecifier, ModuleDecl, ModuleExportName, ModuleItem};
use swc_core::ecma::visit::VisitWith;
use zzop_core::{ImportBinding, ImportMap};

use crate::cjs_require::RequireCollector;
use crate::parse_module;

/// ModuleExportName -> name string (Ident or Str).
pub(crate) fn export_name(n: &ModuleExportName) -> String {
    match n {
        ModuleExportName::Ident(id) => id.sym.to_string(),
        ModuleExportName::Str(s) => s.value.as_str().unwrap_or_default().to_string(),
    }
}

/// The collision-free key of the `n`-th specifier-less `import "y"` in a file — the shared
/// binds-no-name convention documented in `zzop_core::ImportBinding`'s per-language key table. The one
/// place this spelling is written: `crates/core/tests/envelope_schema_parity/import_key_table.rs` reads
/// the literal out of THIS file and holds it against the table, and it cannot tell a test's copy of the
/// string from an emitted one — so tests ask for the key here rather than respelling it.
pub(crate) fn side_effect_key(n: u32) -> String {
    format!("__side_effect_import_{n}__")
}

/// import declarations -> `{ localName -> ImportBinding }`. Specifiers are verbatim; path resolution is
/// the caller's responsibility. Also collects CommonJS `require("literal")` bindings (top-level +
/// function-body-nested) via a tree walk, so dep-graph / circular / call resolution work on CJS trees too.
///
/// A SIDE-EFFECT import (`import "./x";`, no specifier clause at all) binds no name, so it gets a
/// synthetic `__side_effect_import_{N}__` key — the same convention `zzop_core::ImportBinding`'s
/// per-language key table states for every import that binds nothing, so that the EDGE enters the map
/// instead of being dropped. It is a real synchronous module load (the target's top-level effects run),
/// which is how registry-style trees wire their pages; without the binding those targets had no importer
/// at all and false-fired `dead-candidates`/`unreachable`.
pub fn parse_imports(file: &str, source: &str) -> ImportMap {
    let mut map = ImportMap::new();
    let Some(module) = parse_module(file, source) else {
        return map;
    };
    let mut side_effect_seq: u32 = 0;
    for item in &module.body {
        let ModuleItem::ModuleDecl(ModuleDecl::Import(import)) = item else {
            continue;
        };
        let specifier = import.src.value.as_str().unwrap_or_default().to_string();
        let clause_type_only = import.type_only;
        if import.specifiers.is_empty() {
            map.insert(
                side_effect_key(side_effect_seq),
                ImportBinding {
                    specifier,
                    // `"_"` (the Go blank-import spelling), deliberately NOT `"*"`: a side-effect import
                    // consumes no named export, and `find_dead_exports` reads a `"*"` original as "every
                    // export of the target is used" — which would silently blank `unimported-export` out
                    // for the whole target file.
                    original: "_".into(),
                    deferred: false,
                    type_only: clause_type_only,
                },
            );
            side_effect_seq += 1;
            continue;
        }
        for spec in &import.specifiers {
            match spec {
                ImportSpecifier::Named(n) => {
                    let local = n.local.sym.to_string();
                    let original = n
                        .imported
                        .as_ref()
                        .map_or_else(|| local.clone(), export_name);
                    map.insert(
                        local,
                        ImportBinding {
                            specifier: specifier.clone(),
                            original,
                            deferred: false,
                            type_only: clause_type_only || n.is_type_only,
                        },
                    );
                }
                ImportSpecifier::Default(d) => {
                    map.insert(
                        d.local.sym.to_string(),
                        ImportBinding {
                            specifier: specifier.clone(),
                            original: "default".into(),
                            deferred: false,
                            type_only: clause_type_only,
                        },
                    );
                }
                ImportSpecifier::Namespace(ns) => {
                    map.insert(
                        ns.local.sym.to_string(),
                        ImportBinding {
                            specifier: specifier.clone(),
                            original: "*".into(),
                            deferred: false,
                            type_only: clause_type_only,
                        },
                    );
                }
            }
        }
    }
    let mut requires = RequireCollector {
        map: &mut map,
        deferred: false,
        side_effect_seq: 0,
    };
    module.visit_with(&mut requires);
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::binding;

    #[test]
    fn named_import() {
        let m = parse_imports("x.ts", "import { foo } from \"mod\";\n");
        assert_eq!(m["foo"], binding("mod", "foo", false));
    }

    #[test]
    fn aliased_named() {
        let m = parse_imports("x.ts", "import { foo as bar } from \"mod\";\n");
        assert_eq!(m["bar"], binding("mod", "foo", false));
        assert!(!m.contains_key("foo"));
    }

    #[test]
    fn default_import() {
        let m = parse_imports("x.ts", "import Foo from \"mod\";\n");
        assert_eq!(m["Foo"], binding("mod", "default", false));
    }

    #[test]
    fn namespace_import() {
        let m = parse_imports("x.ts", "import * as ns from \"mod\";\n");
        assert_eq!(m["ns"], binding("mod", "*", false));
    }

    #[test]
    fn default_plus_named_mixed() {
        let m = parse_imports("x.ts", "import React, { useState } from \"react\";\n");
        assert_eq!(m["React"].original, "default");
        assert_eq!(m["React"].specifier, "react");
        assert_eq!(m["useState"].original, "useState");
    }

    // --- Side-effect (specifier-less) `import "x"` ---
    //
    // These replace `side_effect_import_has_no_bindings`, which pinned the DEFECT: `import "./x";` is a
    // real module load (the target's top-level effects run), so dropping it left the target with no
    // importer at all and false-fired `dead-candidates`/`unreachable` on every registry-style tree. The
    // synthetic-key convention is the one `zzop_core::ImportBinding`'s per-language key table already
    // states for every import that binds no name — the edge enters the map, keyed collision-free.

    #[test]
    fn side_effect_import_records_edge_under_synthetic_key() {
        let m = parse_imports("x.ts", "import \"./side\";\n");
        assert_eq!(m.len(), 1);
        let (key, binding) = m.iter().next().unwrap();
        assert_eq!(*key, side_effect_key(0));
        assert_eq!(binding.specifier, "./side");
        // NOT "*": a side-effect import consumes no named export, and `find_dead_exports` reads a "*"
        // binding as "every export of the target is used" — which would silently suppress
        // `unimported-export` on the whole target file.
        assert_eq!(binding.original, "_");
        assert!(!binding.deferred);
        assert!(!binding.type_only);
    }

    #[test]
    fn multiple_side_effect_imports_get_distinct_keys() {
        let m = parse_imports("x.ts", "import \"./a\";\nimport \"./b\";\n");
        assert_eq!(m.len(), 2, "both edges must survive the BTreeMap: {m:?}");
        let specs: Vec<&str> = m.values().map(|b| b.specifier.as_str()).collect();
        assert_eq!(specs, vec!["./a", "./b"]);
    }

    #[test]
    fn side_effect_import_and_bare_require_keys_do_not_collide() {
        let m = parse_imports("x.js", "import \"./a\";\nrequire(\"./b\");\n");
        assert_eq!(m.len(), 2, "two independent synthetic namespaces: {m:?}");
        let mut specs: Vec<&str> = m.values().map(|b| b.specifier.as_str()).collect();
        specs.sort_unstable();
        assert_eq!(specs, vec!["./a", "./b"]);
    }

    #[test]
    fn side_effect_import_does_not_displace_a_named_binding() {
        let m = parse_imports(
            "x.ts",
            "import \"./css.css\";\nimport { named } from \"./named\";\n",
        );
        assert_eq!(m["named"], binding("./named", "named", false));
        assert_eq!(m[&side_effect_key(0)].specifier, "./css.css");
    }

    #[test]
    fn type_only_named_binding() {
        let m = parse_imports("x.ts", "import type { T } from \"mod\";\n");
        assert_eq!(m["T"], binding("mod", "T", true));
    }

    #[test]
    fn individual_specifier_type_only() {
        let m = parse_imports("x.ts", "import { type T } from \"mod\";\n");
        assert_eq!(m["T"], binding("mod", "T", true));
    }

    #[test]
    fn namespace_type_only() {
        let m = parse_imports("x.ts", "import type * as ns from \"mod\";\n");
        assert_eq!(m["ns"], binding("mod", "*", true));
    }

    #[test]
    fn default_type_only() {
        let m = parse_imports("x.ts", "import type Foo from \"mod\";\n");
        assert_eq!(m["Foo"], binding("mod", "default", true));
    }

    #[test]
    fn mixed_type_and_value_named() {
        let m = parse_imports("x.ts", "import { type T, val } from \"mod\";\n");
        assert!(m["T"].type_only);
        assert!(!m["val"].type_only);
    }

    #[test]
    fn plain_runtime_import_not_type_only() {
        let m = parse_imports("x.ts", "import { foo } from \"mod\";\n");
        assert!(!m["foo"].type_only);
    }
}
