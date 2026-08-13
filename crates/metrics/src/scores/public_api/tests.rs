//! Covers the empty-graph baseline, same-module imports not counting as cross-module, barrel/root-file
//! imports not being flagged as deep, a deep import bypassing the barrel being flagged, and a mixed
//! case with one deep import among three cross-module imports.
//!
//! Split out of the parent `public_api.rs` on 2026-08-13 to stay under the per-file line cap. The seam
//! is the one the parent already argued for when it kept two test modules apart: this file asks what
//! the metric COMPUTES, while `legend_tests.rs` asks whether the sentence shipped beside the number
//! still describes that computation. They go stale for different reasons and are read by different
//! people.

use super::*;

fn dep(pairs: &[(&str, &[&str])]) -> DepGraph {
    pairs
        .iter()
        .map(|(k, vs)| (k.to_string(), vs.iter().map(|s| s.to_string()).collect()))
        .collect()
}

fn cfg() -> ScoresConfig {
    ScoresConfig::default()
}

#[test]
fn empty_graph_score_100() {
    let r = compute_public_api(&DepGraph::new(), &cfg(), &|_| true);
    assert_eq!(r.score, 100.0);
    assert_eq!(r.total_cross_module_imports, 0);
    assert!(r.deep_imports.is_empty());
}

#[test]
fn same_module_imports_are_not_cross_module_score_100() {
    let d = dep(&[
        ("features/auth/login.ts", &["features/auth/util.ts"]),
        ("features/auth/util.ts", &[]),
    ]);
    let r = compute_public_api(&d, &cfg(), &|_| true);
    assert_eq!(r.total_cross_module_imports, 0);
    assert_eq!(r.score, 100.0);
}

#[test]
fn cross_module_import_via_barrel_or_root_file_is_not_deep() {
    // afterRoot = "index.ts" and "login.ts" — BOTH root imports for the same reason, that neither
    // holds a slash. The comment here used to credit the barrel name for the first one, which
    // matched the (dead) `is_index_barrel` call the code then carried; the barrel spelling has
    // never been what decided this case. See `is_root_import`'s doc.
    let d = dep(&[(
        "features/cart/cart.ts",
        &["features/auth/index.ts", "features/auth/login.ts"],
    )]);
    let r = compute_public_api(&d, &cfg(), &|_| true);
    assert_eq!(r.total_cross_module_imports, 2);
    assert!(r.deep_imports.is_empty());
    assert_eq!(r.score, 100.0);
}

/// A NESTED barrel is a deep import here, and that is a decision rather than an oversight — pinned
/// so the next reader meets it as one. `shared::is_upward_import` (hierarchy) reads the identical
/// file as a barrel and exempts it, so the two metrics disagree about `pkg/sub/index.ts` on
/// purpose, pending evidence that is missing rather than negative (`is_root_import`'s doc carries
/// the corpus measurement and why it is too thin to move scores on).
///
/// Also pinned: the non-JS barrel spellings. `__init__.py` and `mod.rs` are deep imports on BOTH
/// metrics, because the one regex that knows what a barrel is knows only the JS/TS names — so a
/// future widening has to decide those too, not just the depth question.
#[test]
fn a_nested_barrel_is_a_deep_import_and_so_is_every_non_js_barrel_spelling() {
    for to in [
        "features/auth/ui/index.ts",
        "features/auth/a/b/index.ts",
        "features/auth/ui/__init__.py",
        "features/auth/ui/mod.rs",
    ] {
        let d = dep(&[("features/cart/cart.ts", &[to])]);
        let r = compute_public_api(&d, &cfg(), &|_| true);
        assert_eq!(
            r.deep_imports.len(),
            1,
            "{to} must still be judged a deep import — if this flipped, the widening deferred on \
             2026-08-12 was taken, and `is_root_import`'s doc plus the hierarchy sibling need \
             updating in the same commit"
        );
    }
}

#[test]
fn deep_cross_module_import_bypasses_barrel_is_flagged() {
    // afterRoot = "ui/Btn.ts" -> has slash, not a barrel -> deep
    let d = dep(&[("features/cart/cart.ts", &["features/auth/ui/Btn.ts"])]);
    let r = compute_public_api(&d, &cfg(), &|_| true);
    assert_eq!(r.total_cross_module_imports, 1);
    assert_eq!(r.deep_imports.len(), 1);
    assert_eq!(r.deep_imports[0].from, "features/cart/cart.ts");
    assert_eq!(r.deep_imports[0].to, "features/auth/ui/Btn.ts");
    assert_eq!(r.deep_imports[0].to_module, "features/auth");
    assert_eq!(r.score, 0.0);
}

#[test]
fn one_deep_of_three_cross_module_imports_score_67() {
    let d = dep(&[(
        "features/cart/cart.ts",
        &[
            "features/auth/index.ts",  // root (barrel)
            "features/auth/ui/Btn.ts", // deep
            "features/auth/login.ts",  // root (no slash)
            "react",                   // external, skipped
        ],
    )]);
    let r = compute_public_api(&d, &cfg(), &|_| true);
    assert_eq!(r.total_cross_module_imports, 3);
    assert_eq!(r.deep_imports.len(), 1);
    // 100 - (1/3)*100 = 66.67 -> 67
    assert_eq!(r.score, 67.0);
}
