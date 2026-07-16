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

/// import declarations -> `{ localName -> ImportBinding }`. Specifiers are verbatim; path resolution is
/// the caller's responsibility. Also collects CommonJS `require("literal")` bindings (top-level +
/// function-body-nested) via a tree walk, so dep-graph / circular / call resolution work on CJS trees too.
pub fn parse_imports(file: &str, source: &str) -> ImportMap {
    let mut map = ImportMap::new();
    let Some(module) = parse_module(file, source) else {
        return map;
    };
    for item in &module.body {
        let ModuleItem::ModuleDecl(ModuleDecl::Import(import)) = item else {
            continue;
        };
        let specifier = import.src.value.as_str().unwrap_or_default().to_string();
        let clause_type_only = import.type_only;
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

    #[test]
    fn side_effect_import_has_no_bindings() {
        let m = parse_imports("x.ts", "import \"side\";\n");
        assert!(m.is_empty());
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
