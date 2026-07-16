//! Defect B: type-only bindings stay in the DepGraph, but excluded from circular only.
use zzop_core::ImportMap;

use super::build_dep;
use crate::lang::resolve::test_util::paths;
use crate::parse_imports;

#[test]
fn type_only_binding_stays_in_dep_graph_but_is_flagged_excludable() {
    use zzop_core::ImportBinding;
    let mut imports = ImportMap::new();
    imports.insert(
        "T".to_string(),
        ImportBinding {
            specifier: "./y".into(),
            original: "T".into(),
            deferred: false,
            type_only: true,
        },
    );
    let all = paths(&["x.ts", "y.ts"]);
    let (dep, type_only_edges) = build_dep(&[("x.ts".to_string(), imports)], &[], &[], &all);
    // Fan-in/metrics still see the edge — a type import is still a "use" of the target.
    assert_eq!(dep["x.ts"], vec!["y.ts".to_string()]);
    // But circular detection's exclusion set flags it.
    assert!(type_only_edges.contains(&("x.ts".to_string(), "y.ts".to_string())));
}

#[test]
fn value_and_type_only_binding_to_same_target_is_not_excluded() {
    // A real value import to the same target as a type-only one means a genuine runtime edge exists
    // — the pair must not be excluded from circular even though a type-only binding also targets it.
    use zzop_core::ImportBinding;
    let mut imports = ImportMap::new();
    imports.insert(
        "T".to_string(),
        ImportBinding {
            specifier: "./y".into(),
            original: "T".into(),
            deferred: false,
            type_only: true,
        },
    );
    imports.insert(
        "v".to_string(),
        ImportBinding {
            specifier: "./y".into(),
            original: "v".into(),
            deferred: false,
            type_only: false,
        },
    );
    let all = paths(&["x.ts", "y.ts"]);
    let (dep, type_only_edges) = build_dep(&[("x.ts".to_string(), imports)], &[], &[], &all);
    assert_eq!(dep["x.ts"], vec!["y.ts".to_string()]);
    assert!(!type_only_edges.contains(&("x.ts".to_string(), "y.ts".to_string())));
}

#[test]
fn import_type_only_pair_does_not_form_a_circular_dependency() {
    // Two files linked ONLY by `import type` (both directions) must not read as a cycle; a value
    // import between the same two files still must.
    use zzop_core::{circular_from_dep_excluding, ImportBinding};
    let mut a_imports = ImportMap::new();
    a_imports.insert(
        "B".to_string(),
        ImportBinding {
            specifier: "./b".into(),
            original: "B".into(),
            deferred: false,
            type_only: true,
        },
    );
    let mut b_imports = ImportMap::new();
    b_imports.insert(
        "A".to_string(),
        ImportBinding {
            specifier: "./a".into(),
            original: "A".into(),
            deferred: false,
            type_only: true,
        },
    );
    let all = paths(&["a.ts", "b.ts"]);
    let (dep, type_only_edges) = build_dep(
        &[
            ("a.ts".to_string(), a_imports),
            ("b.ts".to_string(), b_imports),
        ],
        &[],
        &[],
        &all,
    );
    assert!(circular_from_dep_excluding(&dep, &type_only_edges).is_empty());
}

#[test]
fn value_import_pair_still_forms_a_circular_dependency() {
    // Same shape as above, but with plain value imports both ways — must still be a cycle.
    use zzop_core::{circular_from_dep_excluding, ImportBinding};
    let mut a_imports = ImportMap::new();
    a_imports.insert(
        "B".to_string(),
        ImportBinding {
            specifier: "./b".into(),
            original: "B".into(),
            deferred: false,
            type_only: false,
        },
    );
    let mut b_imports = ImportMap::new();
    b_imports.insert(
        "A".to_string(),
        ImportBinding {
            specifier: "./a".into(),
            original: "A".into(),
            deferred: false,
            type_only: false,
        },
    );
    let all = paths(&["a.ts", "b.ts"]);
    let (dep, type_only_edges) = build_dep(
        &[
            ("a.ts".to_string(), a_imports),
            ("b.ts".to_string(), b_imports),
        ],
        &[],
        &[],
        &all,
    );
    assert_eq!(circular_from_dep_excluding(&dep, &type_only_edges).len(), 1);
}

#[test]
fn per_specifier_type_only_import_is_also_excluded_from_circular() {
    // `import { type X } from './y'` (per-specifier, not a whole `import type` clause) — parsed via
    // `parse_imports`, proving the exclusion set works end-to-end from real TS source, not just a
    // hand-built `ImportBinding`.
    let a = parse_imports("a.ts", "import { type B } from './b';\n");
    let b = parse_imports("b.ts", "import { type A } from './a';\n");
    let all = paths(&["a.ts", "b.ts"]);
    let (dep, type_only_edges) = build_dep(
        &[("a.ts".to_string(), a), ("b.ts".to_string(), b)],
        &[],
        &[],
        &all,
    );
    assert!(zzop_core::circular_from_dep_excluding(&dep, &type_only_edges).is_empty());
}
