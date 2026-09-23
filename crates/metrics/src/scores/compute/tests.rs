//! These tests assert ORCHESTRATION (every field present, ranges, config/target routing), not the individual
//! formulas (already unit-tested per metric). Config is threaded explicitly through `cfg` rather than via
//! global mutable state, so "config-driven routing" tests build a custom `ScoresConfig` per case.
use std::collections::HashMap;

use super::*;
use crate::scores::config::{FeatureSlicedDesignConfig, FeatureSlicedDesignMatcher};
use crate::scores::types::{FeatureSlicedDesignViolationKind, FileKind};

fn node(path: &str) -> FileNode {
    FileNode {
        id: path.to_string(),
        path: path.to_string(),
        change_count: 0,
        churn: 0,
        last_modified: None,
        author_count: 1,
        loc: 10,
        tag_counts: HashMap::new(),
        fan_in: 0,
        fan_out: 0,
        total_connections: 0,
        risk_score: 0.0,
        ..Default::default()
    }
}

fn dep(pairs: &[(&str, &[&str])]) -> DepGraph {
    pairs
        .iter()
        .map(|(k, vs)| (k.to_string(), vs.iter().map(|s| s.to_string()).collect()))
        .collect()
}

/// Convenience wrapper over `compute_scores` for tests that don't exercise `file_kinds`,
/// `type_safety_counts`, or `lod_by_file` — passes an empty collection for each.
fn compute(
    nodes: &[FileNode],
    dep: &DepGraph,
    circular: &[Vec<String>],
    target: Option<&str>,
    cfg: &ScoresConfig,
) -> Scores {
    compute_scores(
        &ScoresInput {
            nodes,
            dep,
            circular,
            target,
            file_kinds: &FileKinds::new(),
            is_source: &|_| true,
            is_scored: &|_| true,
        },
        cfg,
    )
}

/// Every metric's `.score`, paired with its field name — read out of the SERIALIZED `Scores` rather
/// than dot-accessed field by field. Rust has no dynamic key iteration over a struct, but `Scores` is
/// `Serialize`, and walking the resulting JSON map is the same trick
/// `crates/engine/src/cross_layer_findings/merge_config.rs` uses for the same reason: a hand-written
/// field list only ever covers the struct of the day it was written, so an 18th metric shipped with an
/// out-of-range or non-baseline score while all three callers below stayed green (measured 2026-07-28).
///
/// Every top-level field is REQUIRED to be an object carrying a numeric `score`, which is what `Scores`
/// is documented to be ("one field per structural-health metric"). A field that is not fails here
/// rather than being skipped — a walk that silently ignores what it does not understand is the same
/// blindness in a new shape.
fn all_scores(s: &Scores) -> Vec<(String, f64)> {
    let value = serde_json::to_value(s).expect("Scores is Serialize");
    let obj = value
        .as_object()
        .expect("Scores must serialize to a JSON object");
    let out: Vec<(String, f64)> = obj
        .iter()
        .map(|(name, metric)| {
            let score = metric
                .get("score")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or_else(|| {
                    panic!(
                        "Scores field {name:?} carries no numeric `score` — every field of Scores is \
                         one structural-health metric; if that stopped being true, say so here rather \
                         than letting the walk skip it"
                    )
                });
            (name.clone(), score)
        })
        .collect();
    assert!(
        out.len() >= 10,
        "the Scores walk found {} metric(s) — an empty or collapsed subject set must be RED, never a \
         silent pass",
        out.len()
    );
    out
}

#[test]
fn empty_input_fully_populated_scores_with_every_field_a_0_100_score() {
    let s = compute(&[], &DepGraph::new(), &[], None, &ScoresConfig::default());
    for (k, score) in all_scores(&s) {
        assert!(score.is_finite(), "{k}.score finite");
        assert!(score >= 0.0, "{k}.score >= 0");
        assert!(score <= 100.0, "{k}.score <= 100");
    }
}

#[test]
fn empty_input_every_metric_reports_its_empty_clean_baseline_score_of_100() {
    let s = compute(&[], &DepGraph::new(), &[], None, &ScoresConfig::default());
    for (k, score) in all_scores(&s) {
        assert_eq!(score, 100.0, "{k} empty baseline");
    }
}

#[test]
fn realistic_input_all_sub_scores_within_0_100_and_arrays_well_formed() {
    let d = dep(&[
        ("pages/home.ts", &["features/auth/login.ts", "core/util.ts"]),
        (
            "features/auth/login.ts",
            &["features/cart/cart.ts", "core/util.ts"],
        ),
        ("features/cart/cart.ts", &["core/util.ts"]),
        ("core/util.ts", &[]),
    ]);
    let nodes = [
        FileNode {
            loc: 80,
            fan_out: 2,
            change_count: 5,
            author_count: 1,
            ..node("pages/home.ts")
        },
        FileNode {
            loc: 400,
            fan_out: 2,
            change_count: 12,
            author_count: 1,
            tag_counts: HashMap::from([("FIX".to_string(), 4)]),
            rename_count: Some(3),
            ..node("features/auth/login.ts")
        },
        FileNode {
            loc: 60,
            fan_out: 1,
            change_count: 2,
            author_count: 3,
            ..node("features/cart/cart.ts")
        },
        FileNode {
            loc: 30,
            fan_in: 3,
            fan_out: 0,
            ..node("core/util.ts")
        },
    ];
    let circular = vec![vec!["a".to_string(), "b".to_string(), "a".to_string()]];
    let s = compute(&nodes, &d, &circular, None, &ScoresConfig::default());

    for (k, score) in all_scores(&s) {
        assert!(score.is_finite(), "{k}.score finite");
        assert!((0.0..=100.0).contains(&score), "{k}.score range");
    }

    // fix_ratio.fixShareOfTaggedTouches is a documented 0..1 fraction (not 0..100).
    assert!(s.fix_ratio.fix_share_of_tagged_touches >= 0.0);
    assert!(s.fix_ratio.fix_share_of_tagged_touches <= 1.0);

    // Orchestrator wired the right structural fields through.
    assert_eq!(s.coupling.circular_count, 1); // circular.len() passed through
}

#[test]
fn passes_circular_len_not_the_array_into_coupling() {
    let nodes = [node("x")];
    let circular = vec![
        vec!["a".to_string(), "b".to_string()],
        vec!["c".to_string(), "d".to_string()],
        vec!["e".to_string(), "f".to_string()],
    ];
    let s = compute(
        &nodes,
        &DepGraph::new(),
        &circular,
        None,
        &ScoresConfig::default(),
    );
    assert_eq!(s.coupling.circular_count, 3);
}

#[test]
fn routes_target_into_the_sfc_and_god_file_loc_limits_default_vs_fe_vs_be() {
    let nodes = [node("x")];
    let cfg = ScoresConfig::default();

    let def = compute(&nodes, &DepGraph::new(), &[], None, &cfg);
    assert_eq!(def.file_size_compliance.limit, 150); // default when target omitted
    assert_eq!(def.god_file.limit, 300); // 2x file_size_compliance

    let fe = compute(&nodes, &DepGraph::new(), &[], Some("fe"), &cfg);
    assert_eq!(fe.file_size_compliance.limit, 100);
    assert_eq!(fe.god_file.limit, 200);

    let be = compute(&nodes, &DepGraph::new(), &[], Some("be"), &cfg);
    assert_eq!(be.file_size_compliance.limit, 200);
    assert_eq!(be.god_file.limit, 400);
}

#[test]
fn file_kinds_routes_into_main_sequence_without_panicking_and_stays_in_range() {
    let d = dep(&[
        ("features/auth/login.ts", &["core/util.ts"]),
        ("core/util.ts", &[]),
    ]);
    let mut kinds = FileKinds::new();
    kinds.insert("core/util.ts".to_string(), FileKind::Abstract);
    kinds.insert("features/auth/login.ts".to_string(), FileKind::Concrete);

    let s = compute_scores(
        &ScoresInput {
            nodes: &[],
            dep: &d,
            circular: &[],
            target: None,
            file_kinds: &kinds,
            is_source: &|_| true,
            is_scored: &|_| true,
        },
        &ScoresConfig::default(),
    );

    assert!(s.main_sequence.score >= 0.0);
    assert!(s.main_sequence.score <= 100.0);
}

#[test]
fn config_driven_fsd_routing_custom_slice_container_makes_a_cross_slice_import_count() {
    // Default vocabulary does NOT treat `modules/` as L2 slices, so this import is not a cross-slice violation.
    let d = dep(&[("modules/auth/login.ts", &["modules/cart/cart.ts"])]);
    let default_cfg = ScoresConfig::default();
    let before = compute(&[], &d, &[], None, &default_cfg);
    assert!(before.feature_sliced_design.violations.is_empty());
    assert_eq!(before.feature_sliced_design.score, 100.0);

    // Teaching FSD that `modules/<slice>` is an L2 slice container makes the same import a cross-slice violation.
    let mut custom_cfg = ScoresConfig::default();
    custom_cfg.feature_sliced_design = FeatureSlicedDesignMatcher::new(FeatureSlicedDesignConfig {
        slice_containers: vec!["modules".to_string()],
        ..custom_cfg.feature_sliced_design.config.clone()
    });
    let after = compute(&[], &d, &[], None, &custom_cfg);
    assert_eq!(after.feature_sliced_design.violations.len(), 1);
    assert_eq!(
        after.feature_sliced_design.violations[0].kind,
        FeatureSlicedDesignViolationKind::CrossSlice
    );
    assert_eq!(after.feature_sliced_design.score, 0.0); // 100 - (1/1)*100
}

/// The scored POPULATION reaches the ELEVEN file-keyed metrics and STOPS THERE — both halves of that
/// sentence pinned on one graph carrying a test file beside a source file in the same slice.
///
/// The second half is the load-bearing one and it is asserted as a NON-effect, which is the shape this
/// repo keeps having to add after the fact: a first implementation narrowed the four slice/module-keyed
/// metrics too (by restricting their dep graph) and the 2026-09-11 user ruling reverted it. Without a
/// test that says "cohesion must NOT move", re-adding that restriction is a green diff.
///
/// Assertions are BEFORE/AFTER pairs on the same inputs rather than absolute numbers, because the
/// question is "did the population move", and a single post-state number cannot tell a working filter
/// from a coincidence.
#[test]
fn the_population_filter_moves_the_file_keyed_denominators_and_leaves_the_slice_keyed_ones_alone() {
    use crate::scores::config::PopulationFilter;

    let nodes = [
        node("features/auth/login.ts"),
        node("features/auth/login.test.ts"),
    ];
    // The test file imports its own slice-mate. That import is a real dependency and stays one: it is
    // still counted as INTERNAL cohesion after the filter is on.
    let d = dep(&[
        ("features/auth/login.ts", &[][..]),
        (
            "features/auth/login.test.ts",
            &["features/auth/login.ts"][..],
        ),
    ]);

    let on = ScoresConfig {
        population: PopulationFilter::excluding_test_paths(None),
        ..ScoresConfig::default()
    };
    let before = compute(&nodes, &d, &[], None, &ScoresConfig::default());
    let after = compute(&nodes, &d, &[], None, &on);

    // FILE-KEYED: `fileSizeCompliance`/`godFile` count live source files; the test file leaves both.
    assert_eq!(before.file_size_compliance.total, 2);
    assert_eq!(after.file_size_compliance.total, 1);
    assert_eq!(before.god_file.total, 2);
    assert_eq!(after.god_file.total, 1);

    // SLICE/MODULE-KEYED: untouched. The slice keeps its file-count weight and its internal edge,
    // because the population gate is a per-FILE subject gate and a slice rollup has no such subject —
    // narrowing it would require dropping the node from the graph, which is the thing the doctrine on
    // `ScoresInput::is_scored` forbids.
    assert_eq!(before.cohesion.slices, after.cohesion.slices);
    assert_eq!(after.cohesion.slices[0].file_count, 2);
    assert_eq!(after.cohesion.slices[0].internal_edges, 1);
    assert_eq!(before.cohesion.score, after.cohesion.score);
    assert_eq!(before.sdp.score, after.sdp.score);
    assert_eq!(before.main_sequence.score, after.main_sequence.score);
    assert_eq!(before.modularity.score, after.modularity.score);
}

/// The DEFAULT is the pre-existing population, asserted on the same inputs the test above moves: a
/// `ScoresConfig::default()` run must not be able to tell a test file from a source file.
#[test]
fn the_default_config_counts_test_files_exactly_as_it_always_did() {
    let nodes = [
        node("features/auth/login.ts"),
        node("features/auth/login.test.ts"),
    ];
    let d = dep(&[
        ("features/auth/login.ts", &[][..]),
        (
            "features/auth/login.test.ts",
            &["features/auth/login.ts"][..],
        ),
    ]);
    let s = compute(&nodes, &d, &[], None, &ScoresConfig::default());
    assert_eq!(s.file_size_compliance.total, 2);
    assert_eq!(s.god_file.total, 2);
    assert_eq!(s.cohesion.slices[0].file_count, 2);
}
