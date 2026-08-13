use super::*;
use serde_json::json;

fn one_tree() -> Value {
    json!({
        "trees": [{
            "sourceId": "web",
            "output": {
                "scores": { "overall": 61 },
                "critical": [
                    { "path": "src/core/registry.ts", "blastRadius": 120, "loc": 300, "riskScore": 9.1 },
                    { "path": "src/util/log.ts", "blastRadius": 40, "loc": 20, "riskScore": 3.2 }
                ],
                "seams": [
                    { "folder": "src/core", "files": 12, "internalEdges": 30, "boundaryEdges": 4 }
                ]
            }
        }]
    })
}

#[test]
fn hubs_and_seams_are_drawn_with_their_numbers() {
    let m = project(&one_tree(), None, DEFAULT_RISK_TOP);
    assert!(m.contains("flowchart TD"), "{m}");
    assert!(m.contains("src/core/registry.ts"), "{m}");
    assert!(m.contains("blast 120"), "{m}");
    assert!(m.contains("12 files, 4 boundary edges"), "{m}");
    assert!(m.contains("complete: all 2 hubs and 1 seams"), "{m}");
}

/// The one relation the two lists share is containment. Drawing it as an import arrow would make the
/// picture lie, so the note says which meaning the arrow carries.
#[test]
fn an_arrow_means_containment_and_only_for_a_hub_inside_that_seam() {
    let m = project(&one_tree(), None, DEFAULT_RISK_TOP);
    assert!(
        m.contains("s0 --> h0"),
        "registry.ts is inside src/core:\n{m}"
    );
    assert!(
        !m.contains("s0 --> h1"),
        "log.ts is NOT inside src/core:\n{m}"
    );
}

/// The scores omission must be NAMED, not inferred — the same discipline the join map's header uses.
#[test]
fn the_scores_omission_is_named_in_the_document() {
    let m = project(&one_tree(), None, DEFAULT_RISK_TOP);
    // Derived on BOTH sides on purpose. The previous version of this test asserted the literal
    // "the 17 structural health scores", so when v0.30.0 deleted two scores the guard kept the
    // false sentence alive instead of failing — a test can only protect a number it recomputes.
    assert!(
        m.contains(&format!(
            "NOT drawn: the {} structural health scores",
            zzop_facade::SCORE_MEANINGS.len()
        )),
        "{m}"
    );
    assert!(m.contains("1 tree(s) computed them"), "{m}");
    // `zzop facts` carries no scores — measured 2026-08-11, its top-level keys are config,
    // configWarnings, crossLayer, disclosure, tool, trees, warnings. Pointing a reader there was
    // the second half of the same falsehood, so the absence is pinned, not just the presence.
    assert!(
        !m.contains("zzop facts"),
        "`zzop facts` carries no scores; do not send the reader there:\n{m}"
    );
    assert!(
        m.contains("architecture.pain"),
        "the reader must be told where they ARE:\n{m}"
    );
}

/// Engine order IS the ranking; a cap truncates rather than re-sorts, so this picture cannot disagree
/// with `zzop analyze`'s criticalTop about which hub is worst.
#[test]
fn capping_truncates_the_engine_ranking_rather_than_resorting_it() {
    let m = project(&one_tree(), None, 1);
    assert!(
        m.contains("src/core/registry.ts"),
        "worst hub survives:\n{m}"
    );
    assert!(!m.contains("src/util/log.ts"), "{m}");
    assert!(m.contains("PARTIAL VIEW"), "{m}");
    assert!(m.contains("CONTAINMENT, not imports"), "{m}");
}

/// Per-KIND cap, so a long hub list cannot push every seam out of the picture.
#[test]
fn the_cap_is_per_kind_so_seams_cannot_be_crowded_out_by_hubs() {
    let m = project(&one_tree(), None, 1);
    assert!(
        m.contains("src/core"),
        "the seam survives a hub-heavy cap:\n{m}"
    );
}

#[test]
fn scope_filters_both_kinds_by_prefix() {
    let m = project(&one_tree(), Some("src/util"), DEFAULT_RISK_TOP);
    assert!(m.contains("hubs: drawn 1 / in-scope 1 / total 2"), "{m}");
    assert!(m.contains("seams: drawn 0 / in-scope 0 / total 1"), "{m}");
}

/// "No risk computed" and "no risk" are different, and a picture that showed nothing would be read as
/// the second. This is the same silence-vs-clean rule every surface in this repo follows.
#[test]
fn a_run_with_no_risk_data_says_why_rather_than_drawing_an_empty_picture() {
    let m = project(
        &json!({ "trees": [{ "sourceId": "a", "output": {} }] }),
        None,
        5,
    );
    assert!(m.contains("No hubs or seams"), "{m}");
    assert!(m.contains("need git signals"), "{m}");
    assert!(m.contains("NOT the same as a repo with"), "{m}");
}

#[test]
fn the_same_analysis_renders_identical_bytes() {
    let v = one_tree();
    assert_eq!(
        project(&v, None, DEFAULT_RISK_TOP),
        project(&v, None, DEFAULT_RISK_TOP)
    );
}
