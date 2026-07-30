//! Covers every `scores.*` list this module filters, the two shapes it deliberately does NOT filter
//! (slice/module-keyed rollup rows), the empty-config no-op, and — via a serialized walk rather than a
//! hand-written field list — the drift guard for a list added to `Scores` later.

use super::*;
use crate::scores::types::*;

/// The excluded file. `zzdrop` is a token that appears nowhere else in the fixture (no slice, module, or
/// kept path contains it), so the whole-document check below cannot pass by accident.
const DROP: &str = "vendor/zzdrop.ts";
/// The kept file — every list carries one of each, so a filter that empties a list fails just as loudly as
/// one that filters nothing.
const KEEP: &str = "src/zzkeep.ts";

fn excludes() -> Vec<GlobalExclude> {
    vec![GlobalExclude {
        path: None,
        glob: Some("vendor/**".to_string()),
    }]
}

/// A `Scores` with one excluded and one kept row in EVERY path-valued list, plus rows in the three
/// slice/module-keyed lists that this module must leave alone. The scores themselves are arbitrary — this
/// module never reads them, which is exactly what `scores_and_counts_are_untouched` pins.
fn sample_scores() -> Scores {
    Scores {
        fsd: FsdScore {
            score: 50.0,
            total_imports: 4,
            violations: vec![
                fsd_violation(DROP, KEEP),
                fsd_violation(KEEP, DROP),
                fsd_violation(KEEP, "src/other.ts"),
            ],
        },
        cohesion: CohesionScore {
            score: 60.0,
            slices: vec![SliceCohesion {
                slice: "core".to_string(),
                file_count: 2,
                internal_edges: 1,
                outgoing_edges: 1,
                incoming_edges: 1,
                cohesion: 0.5,
                instability: 0.5,
            }],
        },
        coupling: CouplingScore {
            score: 70.0,
            avg_fan_out: 1.0,
            max_fan_out: 2.0,
            circular_count: 1,
        },
        sdp: SdpScore {
            score: 71.0,
            total_cross_slice_edges: 2,
            violations: vec![SdpViolation {
                from_slice: "core".to_string(),
                to_slice: "shared".to_string(),
                from_i: 0.2,
                to_i: 0.8,
                edge_count: 1,
            }],
        },
        hierarchy: HierarchyScore {
            score: 72.0,
            total_intra_module_edges: 2,
            violations: vec![
                HierarchyViolation {
                    from: DROP.to_string(),
                    to: KEEP.to_string(),
                    module: "core".to_string(),
                },
                HierarchyViolation {
                    from: KEEP.to_string(),
                    to: "src/other.ts".to_string(),
                    module: "core".to_string(),
                },
            ],
        },
        public_api: PublicApiScore {
            score: 73.0,
            total_cross_module_imports: 2,
            deep_imports: vec![
                DeepImport {
                    from: KEEP.to_string(),
                    to: DROP.to_string(),
                    to_module: "core".to_string(),
                },
                DeepImport {
                    from: KEEP.to_string(),
                    to: "src/other.ts".to_string(),
                    to_module: "core".to_string(),
                },
            ],
        },
        sfc: SfcScore {
            score: 74.0,
            limit: 150,
            compliant: 1,
            total: 3,
            violations: vec![
                SfcViolation {
                    path: DROP.to_string(),
                    loc: 400,
                    limit: 150,
                },
                SfcViolation {
                    path: KEEP.to_string(),
                    loc: 400,
                    limit: 150,
                },
            ],
        },
        main_sequence: MainSequenceScore {
            score: 75.0,
            avg_distance: 0.3,
            modules: vec![ModuleMainSeq {
                module: "core".to_string(),
                file_count: 2,
                abstractness: 0.5,
                instability: 0.5,
                distance: 0.0,
            }],
        },
        modularity: ModularityScore {
            score: 76.0,
            q: 0.4,
            edge_count: 3,
            slice_count: 2,
        },
        god_file: GodFileScore {
            score: 77.0,
            limit: 300,
            files: vec![
                GodFile {
                    path: DROP.to_string(),
                    loc: 400,
                },
                GodFile {
                    path: KEEP.to_string(),
                    loc: 400,
                },
            ],
        },
        sibling_cross: SiblingCrossScore {
            score: 78.0,
            total_intra_module_edges: 2,
            violations: vec![
                SiblingCross {
                    from: DROP.to_string(),
                    to: KEEP.to_string(),
                    module: "core".to_string(),
                    from_subdir: "a".to_string(),
                    to_subdir: "b".to_string(),
                },
                SiblingCross {
                    from: KEEP.to_string(),
                    to: "src/other.ts".to_string(),
                    module: "core".to_string(),
                    from_subdir: "a".to_string(),
                    to_subdir: "b".to_string(),
                },
            ],
        },
        diamond: DiamondScore {
            score: 79.0,
            pairs: vec![
                // Excluded only in the MIDDLE of the path — `through` is as much a named path as the
                // endpoints are.
                DiamondPair {
                    root: KEEP.to_string(),
                    leaf: "src/other.ts".to_string(),
                    through: vec![DROP.to_string(), "src/mid.ts".to_string()],
                },
                DiamondPair {
                    root: KEEP.to_string(),
                    leaf: "src/other.ts".to_string(),
                    through: vec!["src/mid.ts".to_string()],
                },
            ],
        },
        rename_instability: RenameScore {
            score: 80.0,
            renamed: 2,
            total: 3,
            files: vec![
                RenamedFile {
                    path: DROP.to_string(),
                    rename_count: 2,
                },
                RenamedFile {
                    path: KEEP.to_string(),
                    rename_count: 2,
                },
            ],
        },
        bus_factor: BusFactorScore {
            score: 81.0,
            risky: 2,
            files: vec![
                BusFactorFile {
                    path: DROP.to_string(),
                    change_count: 9,
                    authors: 1,
                },
                BusFactorFile {
                    path: KEEP.to_string(),
                    change_count: 9,
                    authors: 1,
                },
            ],
        },
        fix_ratio: FixRatioScore {
            score: 82.0,
            fix: 1,
            total: 4,
            ratio: 0.25,
        },
        type_safety: TypeSafetyScore {
            score: 83.0,
            total_as_cast: 2,
            total_any_type: 2,
            violations: vec![
                TypeSafetyViolation {
                    path: DROP.to_string(),
                    as_cast: 1,
                    any_type: 1,
                    loc: 400,
                    density: 0.005,
                },
                TypeSafetyViolation {
                    path: KEEP.to_string(),
                    as_cast: 1,
                    any_type: 1,
                    loc: 400,
                    density: 0.005,
                },
            ],
        },
        lod: LodScore {
            score: 84.0,
            total_violations: 2,
            violations: vec![
                LodFileSummary {
                    path: DROP.to_string(),
                    count: 1,
                    max_depth: 3,
                    loc: 400,
                    density: 0.0025,
                },
                LodFileSummary {
                    path: KEEP.to_string(),
                    count: 1,
                    max_depth: 3,
                    loc: 400,
                    density: 0.0025,
                },
            ],
        },
    }
}

fn fsd_violation(from: &str, to: &str) -> FsdViolation {
    FsdViolation {
        from: from.to_string(),
        to: to.to_string(),
        kind: FsdViolationKind::LayerReverse,
        from_layer: 3,
        to_layer: 1,
        from_slice: None,
        to_slice: None,
    }
}

#[test]
fn every_single_file_list_drops_the_excluded_row_and_keeps_the_other() {
    let mut s = sample_scores();
    apply_excludes_to_scores(&mut s, &excludes());

    assert_eq!(
        s.sfc
            .violations
            .iter()
            .map(|v| v.path.as_str())
            .collect::<Vec<_>>(),
        vec![KEEP]
    );
    assert_eq!(
        s.god_file
            .files
            .iter()
            .map(|v| v.path.as_str())
            .collect::<Vec<_>>(),
        vec![KEEP]
    );
    assert_eq!(
        s.rename_instability
            .files
            .iter()
            .map(|v| v.path.as_str())
            .collect::<Vec<_>>(),
        vec![KEEP]
    );
    assert_eq!(
        s.bus_factor
            .files
            .iter()
            .map(|v| v.path.as_str())
            .collect::<Vec<_>>(),
        vec![KEEP]
    );
    assert_eq!(
        s.type_safety
            .violations
            .iter()
            .map(|v| v.path.as_str())
            .collect::<Vec<_>>(),
        vec![KEEP]
    );
    assert_eq!(
        s.lod
            .violations
            .iter()
            .map(|v| v.path.as_str())
            .collect::<Vec<_>>(),
        vec![KEEP]
    );
}

/// An edge row names TWO files and the two sides are not symmetric. The `from`/`root` side is the judged
/// SUBJECT: it can no longer be excluded here, because `compute_scores` never produced a row for an
/// excluded subject (`ScoresInput::is_scored`) — the `retain` that still drops it is belt-and-braces, and
/// this test exercises it directly by handing the function rows it would not receive in a real run.
///
/// The far side is different: that row IS counted against a scored file, so dropping it would delete the
/// evidence for a number the run still reports — the exact defect this seam exists to close. It is
/// REDACTED instead, the same treatment `zzop_core::registry::redact` gives excluded finding evidence.
///
/// Revised 2026-07-30. This test previously asserted that EITHER endpoint dropped the row, which was
/// coherent while an excluded file changed no score at all.
#[test]
fn an_edge_row_drops_on_its_subject_and_redacts_its_far_side() {
    let mut s = sample_scores();
    apply_excludes_to_scores(&mut s, &excludes());

    // The fixture's fsd list is (DROP -> KEEP), (KEEP -> DROP), (KEEP -> src/other.ts). The first loses its
    // subject and goes; the second keeps its subject and only its target is masked.
    assert_eq!(s.fsd.violations.len(), 2, "{:?}", s.fsd.violations);
    assert!(
        s.fsd.violations.iter().all(|v| v.from == KEEP),
        "no row may survive with an excluded subject: {:?}",
        s.fsd.violations
    );
    assert!(
        s.fsd.violations.iter().any(|v| v.to == zzop_core::REDACTED),
        "the excluded target must be redacted, not deleted with its row: {:?}",
        s.fsd.violations
    );
    assert!(
        s.fsd.violations.iter().any(|v| v.to == "src/other.ts"),
        "an untouched row must stay untouched: {:?}",
        s.fsd.violations
    );

    assert_eq!(
        s.hierarchy.violations.len(),
        1,
        "{:?}",
        s.hierarchy.violations
    );
    assert_eq!(s.hierarchy.violations[0].from, KEEP);

    assert_eq!(
        s.public_api.deep_imports.len(),
        2,
        "{:?}",
        s.public_api.deep_imports
    );
    assert!(s
        .public_api
        .deep_imports
        .iter()
        .any(|v| v.to == zzop_core::REDACTED));

    assert_eq!(
        s.sibling_cross.violations.len(),
        1,
        "{:?}",
        s.sibling_cross.violations
    );
    assert_eq!(s.sibling_cross.violations[0].from, KEEP);

    // `through` is support for a scored root, so an excluded hop is masked rather than deleting the pair.
    assert_eq!(s.diamond.pairs.len(), 2, "{:?}", s.diamond.pairs);
    assert!(
        s.diamond
            .pairs
            .iter()
            .any(|p| p.through.iter().any(|t| t == zzop_core::REDACTED)
                || p.leaf == zzop_core::REDACTED),
        "an excluded hop or leaf must be redacted, not drop the pair: {:?}",
        s.diamond.pairs
    );
}

/// Slice- and module-keyed rows are whole-directory rollups, not files the user excluded — the same reason
/// `pain` is not filtered. Pinned so a later "make it consistent" pass has to argue with this test.
#[test]
fn slice_and_module_keyed_rollup_rows_are_left_alone() {
    let mut s = sample_scores();
    apply_excludes_to_scores(&mut s, &excludes());

    assert_eq!(s.cohesion.slices.len(), 1);
    assert_eq!(s.sdp.violations.len(), 1);
    assert_eq!(s.main_sequence.modules.len(), 1);
}

/// Every metric field that is NOT a list, read out of the SERIALIZED report so a metric added later is
/// covered without editing this helper — the same reason `crate::scores::compute::tests::all_scores` walks
/// JSON instead of naming fields.
fn all_non_list_fields(s: &Scores) -> serde_json::Value {
    let mut value = serde_json::to_value(s).expect("Scores is Serialize");
    for (_metric_name, metric) in value
        .as_object_mut()
        .expect("Scores must serialize to a JSON object")
        .iter_mut()
    {
        metric
            .as_object_mut()
            .expect("every Scores field is one metric object")
            .retain(|_field, v| !v.is_array());
    }
    value
}

/// Every `.score` and every count/denominator behind it is computed over the whole tree. This module
/// shortens lists; it must never touch a number.
#[test]
fn scores_and_counts_are_untouched() {
    let before = sample_scores();
    let mut after = sample_scores();
    apply_excludes_to_scores(&mut after, &excludes());
    assert_eq!(all_non_list_fields(&before), all_non_list_fields(&after));
}

/// The default config carries no top-level `exclude`, so the overwhelmingly common run must be a provable
/// no-op — not "filters nothing because nothing matched", but structurally identical output.
#[test]
fn an_empty_exclude_list_changes_nothing() {
    let before = sample_scores();
    let mut after = sample_scores();
    apply_excludes_to_scores(&mut after, &[]);
    assert_eq!(before, after);
}

/// Drift guard. A hand-written field list only ever covers the struct of the day it was written: an 18th
/// metric shipping a new path-keyed list would silently escape [`apply_excludes_to_scores`] while every
/// assertion above stayed green. `Scores` is `Serialize`, so the filtered report is walked as JSON and the
/// excluded path is asserted absent from the WHOLE document — the same trick, for the same reason, as
/// `crate::scores::compute::tests::all_scores`.
///
/// `DROP`'s `zzdrop` token appears in no slice, module, or kept path in the fixture, so a hit is always a
/// real leak rather than an incidental substring.
#[test]
fn no_excluded_path_survives_anywhere_in_the_serialized_report() {
    let mut s = sample_scores();
    apply_excludes_to_scores(&mut s, &excludes());
    let json = serde_json::to_string(&s).expect("Scores is Serialize");
    assert!(
        !json.contains("zzdrop"),
        "an excluded path survived somewhere in the serialized scores report — a list this module does \
         not know about was added to `Scores`: {json}"
    );
    assert!(
        json.contains("zzkeep"),
        "the kept path must survive, else this guard would pass on an empty report: {json}"
    );
}
