//! Import extraction -> `zzop_core::ImportMap`. Specifier convention (the engine resolver depends on this
//! exactly, mirroring `zzop_parser_typescript::parse_imports`'s verbatim-specifier discipline):
//! - **Relative** (`from . import x`, `from .sib import y`, `from ..a.b import z`): SLASH-relative with
//!   explicit dots — level 1 -> `./...`, level 2 -> `../...`, level N -> `(N-1)` `../` segments. Kept out
//!   of the package-import census (which filters specifiers starting with `.`).
//! - **Absolute** (`import a.b.c`, `from a.b import c [as d]`): the DOTTED module path as written
//!   (`a.b.c` / `a.b`) — untouched, same as TS's verbatim `import "@/features/x"` specifier.
//!
//! `ImportBinding::local` is the name THIS FILE binds: a bare `import a.b.c` (no `as`) only binds the
//! top-level package name `a` in the importing scope (the rest is reached via attribute access on `a`,
//! per Python's own binding rule); an `as` alias binds exactly the name written, referencing the
//! submodule directly. `original` mirrors `zzop_parser_typescript`'s "*" = whole-namespace convention
//! (`ImportSpecifier::Namespace` -> `original: "*"`): `"*"` for every plain/aliased `import` statement
//! (the entire module namespace is what gets bound) and for `from x import *`; the plain imported name
//! for `from x import name [as alias]`.
//!
//! A star import (`from x import *`) binds no single local name Python-side, so — mirroring
//! `zzop_parser_typescript::RequireCollector`'s synthetic `__require{N}__` key for a require call with no
//! destructurable binding — each one gets a synthetic, collision-free map key (`__star_import_{N}__`,
//! sequential per file) so it still enters the map as a real edge instead of being silently dropped.
//!
//! `type_only`/`deferred` are always `false` — Python has neither concept.

use ruff_python_ast::{Alias, Stmt, StmtImport, StmtImportFrom};
use zzop_core::{ImportBinding, ImportMap};

/// Extract this file's import bindings. Returns an empty map on parse failure (never panics).
pub fn parse_imports(text: &str) -> ImportMap {
    let mut map = ImportMap::new();
    let Some(module) = crate::parse_module(text) else {
        return map;
    };
    let mut star_seq: u32 = 0;
    for stmt in &module.body {
        match stmt {
            Stmt::Import(imp) => collect_plain_import(imp, &mut map),
            Stmt::ImportFrom(imp) => collect_from_import(imp, &mut map, &mut star_seq),
            _ => {}
        }
    }
    map
}

/// `import a.b.c [as x]`, `import a, b.c as y` — one alias per module reference.
fn collect_plain_import(imp: &StmtImport, map: &mut ImportMap) {
    for alias in &imp.names {
        let dotted = alias.name.id.as_str();
        let local = match &alias.asname {
            Some(asname) => asname.id.to_string(),
            // No `as` — Python binds only the top-level package segment (`a` for `import a.b.c`).
            None => dotted.split('.').next().unwrap_or(dotted).to_string(),
        };
        map.insert(
            local,
            ImportBinding {
                specifier: dotted.to_string(),
                original: "*".to_string(),
                deferred: false,
                type_only: false,
            },
        );
    }
}

/// `from [.[.[...]]][module] import name [as alias], ...` / `... import *`.
fn collect_from_import(imp: &StmtImportFrom, map: &mut ImportMap, star_seq: &mut u32) {
    let level = imp.level;
    if level == 0 {
        // Absolute — grammar requires a module name when there are no leading dots.
        let Some(module) = &imp.module else { return };
        insert_from_aliases(&imp.names, module.id.as_ref(), map, star_seq);
        return;
    }

    let prefix = relative_prefix(level);
    match &imp.module {
        // `from .sib import y` / `from ..a.b import z` — module path present alongside the dots.
        Some(module) => {
            let base = format!("{prefix}{}", module.id.as_str().replace('.', "/"));
            insert_from_aliases(&imp.names, &base, map, star_seq);
        }
        // `from . import x, y` / `from .. import z` — no module path, so there is no single `base`
        // shared across every alias: each imported name IS the sibling-module reference (`from . import
        // x` -> `./x`), so the specifier is built per-alias here rather than via `insert_from_aliases`.
        None => {
            for alias in &imp.names {
                let original = alias.name.id.as_str();
                if original == "*" {
                    // `from . import *` — no named target at all; the dots alone name "this package".
                    insert_star(map, star_seq, prefix.trim_end_matches('/').to_string());
                    continue;
                }
                let local = local_name(alias);
                map.insert(
                    local,
                    ImportBinding {
                        specifier: format!("{prefix}{original}"),
                        original: original.to_string(),
                        deferred: false,
                        type_only: false,
                    },
                );
            }
        }
    }
}

/// Inserts one binding per alias in a `from <base> import ...` clause sharing one resolved `base`
/// specifier (already dot-or-slash-normalized by the caller) — the common case for both absolute and
/// module-bearing relative imports.
fn insert_from_aliases(names: &[Alias], base: &str, map: &mut ImportMap, star_seq: &mut u32) {
    for alias in names {
        let original = alias.name.id.as_str();
        if original == "*" {
            insert_star(map, star_seq, base.to_string());
            continue;
        }
        let local = local_name(alias);
        map.insert(
            local,
            ImportBinding {
                specifier: base.to_string(),
                original: original.to_string(),
                deferred: false,
                type_only: false,
            },
        );
    }
}

/// The local name a `from`-import alias binds: the `as` alias when present, else the imported name
/// itself.
fn local_name(alias: &Alias) -> String {
    alias
        .asname
        .as_ref()
        .map_or_else(|| alias.name.id.to_string(), |a| a.id.to_string())
}

fn insert_star(map: &mut ImportMap, star_seq: &mut u32, specifier: String) {
    map.insert(
        format!("__star_import_{}__", *star_seq),
        ImportBinding {
            specifier,
            original: "*".to_string(),
            deferred: false,
            type_only: false,
        },
    );
    *star_seq += 1;
}

/// `level` (leading-dot count) -> its slash-relative prefix: 1 -> `"./"`, 2 -> `"../"`,
/// 3 -> `"../../"`, ... (level 1 is "this package", each extra level walks up one more parent).
fn relative_prefix(level: u32) -> String {
    if level <= 1 {
        "./".to_string()
    } else {
        "../".repeat((level - 1) as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding<'a>(map: &'a ImportMap, local: &str) -> &'a ImportBinding {
        map.get(local)
            .unwrap_or_else(|| panic!("no binding for {local:?} in {map:?}"))
    }

    #[test]
    fn plain_import_binds_the_top_level_segment() {
        let map = parse_imports("import a.b.c\n");
        let b = binding(&map, "a");
        assert_eq!(b.specifier, "a.b.c");
        assert_eq!(b.original, "*");
        assert!(!b.deferred && !b.type_only);
    }

    #[test]
    fn plain_import_single_segment() {
        let map = parse_imports("import os\n");
        let b = binding(&map, "os");
        assert_eq!(b.specifier, "os");
        assert_eq!(b.original, "*");
    }

    #[test]
    fn aliased_plain_import_binds_the_alias_directly_to_the_submodule() {
        let map = parse_imports("import a.b.c as x\n");
        let b = binding(&map, "x");
        assert_eq!(b.specifier, "a.b.c");
        assert_eq!(b.original, "*");
        assert!(!map.contains_key("a"));
    }

    #[test]
    fn multiple_targets_in_one_import_statement() {
        let map = parse_imports("import os, sys as s\n");
        assert_eq!(binding(&map, "os").specifier, "os");
        assert_eq!(binding(&map, "s").specifier, "sys");
    }

    #[test]
    fn from_import_absolute_binds_the_imported_name() {
        let map = parse_imports("from fastapi import FastAPI\n");
        let b = binding(&map, "FastAPI");
        assert_eq!(b.specifier, "fastapi");
        assert_eq!(b.original, "FastAPI");
    }

    #[test]
    fn from_import_absolute_with_alias() {
        let map = parse_imports("from a.b import c as d\n");
        let b = binding(&map, "d");
        assert_eq!(b.specifier, "a.b");
        assert_eq!(b.original, "c");
    }

    #[test]
    fn from_import_absolute_star() {
        let map = parse_imports("from a.b import *\n");
        assert_eq!(map.len(), 1);
        let (_, b) = map.iter().next().unwrap();
        assert_eq!(b.specifier, "a.b");
        assert_eq!(b.original, "*");
    }

    #[test]
    fn from_import_relative_level_1_bare_dot() {
        let map = parse_imports("from . import x\n");
        let b = binding(&map, "x");
        assert_eq!(b.specifier, "./x");
        assert_eq!(b.original, "x");
    }

    #[test]
    fn from_import_relative_level_1_with_module() {
        let map = parse_imports("from .sib import y\n");
        let b = binding(&map, "y");
        assert_eq!(b.specifier, "./sib");
        assert_eq!(b.original, "y");
    }

    #[test]
    fn from_import_relative_level_2_with_dotted_module() {
        let map = parse_imports("from ..a.b import z\n");
        let b = binding(&map, "z");
        assert_eq!(b.specifier, "../a/b");
        assert_eq!(b.original, "z");
    }

    #[test]
    fn from_import_relative_level_3() {
        let map = parse_imports("from ...pkg import z\n");
        let b = binding(&map, "z");
        assert_eq!(b.specifier, "../../pkg");
    }

    #[test]
    fn from_import_relative_bare_dot_multiple_names() {
        let map = parse_imports("from . import a, b as c\n");
        assert_eq!(binding(&map, "a").specifier, "./a");
        assert_eq!(binding(&map, "c").specifier, "./b");
        assert_eq!(binding(&map, "c").original, "b");
    }

    #[test]
    fn multiple_star_imports_get_distinct_synthetic_keys() {
        let map = parse_imports("from a import *\nfrom b import *\n");
        assert_eq!(map.len(), 2);
        let specifiers: Vec<&str> = map.values().map(|b| b.specifier.as_str()).collect();
        assert!(specifiers.contains(&"a"));
        assert!(specifiers.contains(&"b"));
    }

    #[test]
    fn parse_failure_yields_empty_map() {
        assert!(parse_imports("import (:\n").is_empty());
    }

    #[test]
    fn relative_specifiers_are_excluded_from_package_import_census_by_convention() {
        // Not a real census call — just pins the leading-dot invariant the resolver's filter depends on.
        let map = parse_imports("from .sib import y\n");
        assert!(binding(&map, "y").specifier.starts_with('.'));
    }
}
