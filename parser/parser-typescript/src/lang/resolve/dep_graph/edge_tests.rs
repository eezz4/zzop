//! Dep-graph edge construction: internal/external filtering, deferred exclusion, the Defect A
//! re-export/dynamic-import merge, and the workspace-aware builder.
use std::collections::HashMap;

use zzop_core::ImportMap;

use super::{build_dep, build_dep_with_workspace};
use crate::lang::resolve::test_util::{no_tsconfigs, paths, ws_pkgs};
use crate::parse_imports;

#[test]
fn barrel_index_js_specifier_reexport_chain_resolves_end_to_end() {
    // a.ts -> ./b/index.js (real: b/index.ts) -> ./c.js (real: c.ts), both hops using a literal
    // `.js` extension (NodeNext style), chained through `build_dep` to prove both hops resolve.
    // `b/index.ts` uses an import-then-local-export barrel (an `import` + a local, from-less
    // `export { c }`), not a bare `export { c } from './c.js'` re-export — that shape is covered
    // separately by `bare_named_re_export_creates_dep_edge` below, now that `build_dep` merges
    // `parse_re_exports` output into the graph too.
    let a = parse_imports("a.ts", "import { c } from './b/index.js';\n");
    let b = parse_imports(
        "b/index.ts",
        "import { c } from '../c.js';\nexport { c };\n",
    );
    let all = paths(&["a.ts", "b/index.ts", "c.ts"]);
    let (dep, _noncycle) = build_dep(
        &[("a.ts".to_string(), a), ("b/index.ts".to_string(), b)],
        &[],
        &[],
        &all,
    );
    assert_eq!(dep["a.ts"], vec!["b/index.ts".to_string()]);
    assert_eq!(dep["b/index.ts"], vec!["c.ts".to_string()]);
}

// --- Defect A: bare re-exports merge into the dep graph as runtime edges ---

#[test]
fn bare_named_re_export_creates_dep_edge() {
    // `export { x } from './b'` alone (no local import) used to be invisible to `build_dep` — the
    // whole point of Defect A's fix.
    let re_exports = vec![(
        "barrel.ts".to_string(),
        vec![zzop_core::ReExport {
            specifier: "./b".to_string(),
            original: "x".to_string(),
            local_alias: "x".to_string(),
            type_only: false,
        }],
    )];
    let all = paths(&["barrel.ts", "b.ts"]);
    let (dep, _noncycle) = build_dep(
        &[("barrel.ts".to_string(), ImportMap::new())],
        &re_exports,
        &[],
        &all,
    );
    assert_eq!(dep["barrel.ts"], vec!["b.ts".to_string()]);
}

#[test]
fn bare_star_re_export_creates_dep_edge() {
    // `export * from './z'` alone — same fix, the star-form re-export.
    let re_exports = vec![(
        "barrel.ts".to_string(),
        vec![zzop_core::ReExport {
            specifier: "./z".to_string(),
            original: "*".to_string(),
            local_alias: "*".to_string(),
            type_only: false,
        }],
    )];
    let all = paths(&["barrel.ts", "z.ts"]);
    let (dep, _noncycle) = build_dep(
        &[("barrel.ts".to_string(), ImportMap::new())],
        &re_exports,
        &[],
        &all,
    );
    assert_eq!(dep["barrel.ts"], vec!["z.ts".to_string()]);
}

#[test]
fn type_only_re_export_creates_excludable_dep_edge() {
    // `export type { X } from './y'` is erased by TS at compile time, so it must never form a real
    // runtime cycle — but (Defect 1) it now DOES gain a real dep edge (fan-in), same treatment as a
    // type-only import binding (see the Defect B / noncycle tests): the edge is present in
    // `dep` and the `(src, target)` pair lands in the returned noncycle-exclusion set.
    let re_exports = vec![(
        "barrel.ts".to_string(),
        vec![zzop_core::ReExport {
            specifier: "./y".to_string(),
            original: "X".to_string(),
            local_alias: "X".to_string(),
            type_only: true,
        }],
    )];
    let all = paths(&["barrel.ts", "y.ts"]);
    let (dep, noncycle_edges) = build_dep(
        &[("barrel.ts".to_string(), ImportMap::new())],
        &re_exports,
        &[],
        &all,
    );
    assert_eq!(dep["barrel.ts"], vec!["y.ts".to_string()]);
    assert!(noncycle_edges.contains(&("barrel.ts".to_string(), "y.ts".to_string())));
}

#[test]
fn dynamic_import_creates_excludable_dep_edge() {
    // Defect 2: `import('./MoodChart')` (including inside a `dynamic(() => import('./X'))`/
    // `lazy(() => import('./X'))` wrapper, per `parse_dynamic_imports`) used to create no dep edge at
    // all, so a code-split-only module looked dead. It now gains a real edge (fan-in) but is excluded
    // from circular detection (async — never a synchronous module-load cycle, and specifically how
    // people BREAK cycles).
    let dynamic_imports = vec![("page.ts".to_string(), vec!["./MoodChart".to_string()])];
    let all = paths(&["page.ts", "MoodChart.ts"]);
    let (dep, noncycle_edges) = build_dep(
        &[("page.ts".to_string(), ImportMap::new())],
        &[],
        &dynamic_imports,
        &all,
    );
    assert_eq!(dep["page.ts"], vec!["MoodChart.ts".to_string()]);
    assert!(noncycle_edges.contains(&("page.ts".to_string(), "MoodChart.ts".to_string())));
}

#[test]
fn dynamic_import_cycle_is_not_reported_as_circular() {
    // Two files linked ONLY by dynamic `import()` (both directions) must not read as a cycle —
    // mirrors `import_type_only_pair_does_not_form_a_circular_dependency` (noncycle tests), for Defect 2.
    use zzop_core::circular_from_dep_excluding;
    let dynamic_imports = vec![
        ("a.ts".to_string(), vec!["./b".to_string()]),
        ("b.ts".to_string(), vec!["./a".to_string()]),
    ];
    let all = paths(&["a.ts", "b.ts"]);
    let (dep, noncycle_edges) = build_dep(
        &[
            ("a.ts".to_string(), ImportMap::new()),
            ("b.ts".to_string(), ImportMap::new()),
        ],
        &[],
        &dynamic_imports,
        &all,
    );
    assert!(circular_from_dep_excluding(&dep, &noncycle_edges).is_empty());
}

#[test]
fn re_export_target_gains_fan_in_via_reverse_dep_edge() {
    // The dep-graph consumer side of Defect A: a barrel-only re-export gives its target an inbound
    // edge, which is exactly what `fan_in` (reverse-edge count, `analyze.rs`'s `dep_stats_from_dep`)
    // counts to avoid `dead-candidates` false-positiving a re-exported-only file.
    let re_exports = vec![(
        "barrel.ts".to_string(),
        vec![zzop_core::ReExport {
            specifier: "./impl".to_string(),
            original: "x".to_string(),
            local_alias: "x".to_string(),
            type_only: false,
        }],
    )];
    let all = paths(&["barrel.ts", "impl.ts"]);
    let (dep, _noncycle) = build_dep(
        &[("barrel.ts".to_string(), ImportMap::new())],
        &re_exports,
        &[],
        &all,
    );
    let fan_in = dep
        .values()
        .filter(|tos| tos.contains(&"impl.ts".to_string()))
        .count();
    assert_eq!(fan_in, 1);
}

#[test]
fn build_dep_keeps_internal_drops_external() {
    let imports = parse_imports("a.ts", "import { x } from './b';\nimport 'react';\n");
    let all = paths(&["a.ts", "b.ts"]);
    let (dep, _noncycle) = build_dep(&[("a.ts".to_string(), imports)], &[], &[], &all);
    assert_eq!(dep["a.ts"], vec!["b.ts".to_string()]);
}

#[test]
fn build_dep_excludes_deferred() {
    use zzop_core::ImportBinding;
    let mut imports = ImportMap::new();
    imports.insert(
        "Y".to_string(),
        ImportBinding {
            specifier: "./y".into(),
            original: "*".into(),
            deferred: true,
            type_only: false,
        },
    );
    let all = paths(&["x.js", "y.ts"]);
    let (dep, _noncycle) = build_dep(&[("x.js".to_string(), imports)], &[], &[], &all);
    assert!(dep["x.js"].is_empty());
}

// --- build_dep_with_workspace ---

#[test]
fn build_dep_with_workspace_resolves_cross_package_edge() {
    let mut imports = ImportMap::new();
    imports.insert(
        "utils".to_string(),
        zzop_core::ImportBinding {
            specifier: "@acme/utils-core".into(),
            original: "*".into(),
            deferred: false,
            type_only: false,
        },
    );
    let all = paths(&["a.ts", "packages/utils-core/src/index.ts"]);
    let (dep, _noncycle) = build_dep_with_workspace(
        &[("a.ts".to_string(), imports)],
        &[],
        &[],
        &all,
        &ws_pkgs(),
        &no_tsconfigs(),
    );
    assert_eq!(
        dep["a.ts"],
        vec!["packages/utils-core/src/index.ts".to_string()]
    );
}

#[test]
fn build_dep_with_workspace_matches_build_dep_when_no_workspace_pkgs() {
    let imports = parse_imports("a.ts", "import { x } from './b';\n");
    let all = paths(&["a.ts", "b.ts"]);
    let (dep, _noncycle) = build_dep_with_workspace(
        &[("a.ts".to_string(), imports)],
        &[],
        &[],
        &all,
        &HashMap::new(),
        &no_tsconfigs(),
    );
    assert_eq!(dep["a.ts"], vec!["b.ts".to_string()]);
}

#[test]
fn build_dep_with_workspace_merges_re_exports_too() {
    // Same Defect A fix as `bare_named_re_export_creates_dep_edge`, through the workspace-aware
    // entry point the engine's incremental path actually calls.
    let re_exports = vec![(
        "barrel.ts".to_string(),
        vec![zzop_core::ReExport {
            specifier: "./b".to_string(),
            original: "x".to_string(),
            local_alias: "x".to_string(),
            type_only: false,
        }],
    )];
    let all = paths(&["barrel.ts", "b.ts"]);
    let (dep, _noncycle) = build_dep_with_workspace(
        &[("barrel.ts".to_string(), ImportMap::new())],
        &re_exports,
        &[],
        &all,
        &HashMap::new(),
        &no_tsconfigs(),
    );
    assert_eq!(dep["barrel.ts"], vec!["b.ts".to_string()]);
}
