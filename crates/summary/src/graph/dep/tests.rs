use super::*;
use serde_json::json;

/// One tree, a 3-file cycle plus an unrelated leaf — enough to exercise every branch that shapes the
/// picture without a fixture nobody can read.
fn one_tree() -> Value {
    json!({
        "trees": [{
            "sourceId": "web",
            "output": {
                "ir": { "dep": {
                    "src/a.ts": ["src/b.ts"],
                    "src/b.ts": ["src/c.ts"],
                    "src/c.ts": ["src/a.ts"],
                    "src/leaf.ts": []
                }},
                "findings": [{
                    "ruleId": "circular", "severity": "warning", "file": "src/a.ts", "line": 1,
                    "message": "m", "data": { "members": ["src/a.ts", "src/b.ts", "src/c.ts"] }
                }]
            }
        }]
    })
}

#[test]
fn every_file_and_edge_is_drawn_when_nothing_is_capped() {
    let m = project(&one_tree(), None, DEFAULT_DEP_TOP);
    assert!(m.contains("flowchart LR"), "{m}");
    for f in ["src/a.ts", "src/b.ts", "src/c.ts", "src/leaf.ts"] {
        assert!(m.contains(f), "{f} missing from:\n{m}");
    }
    assert!(m.contains("nodes: drawn 4 / in-scope 4 / total 4"), "{m}");
    assert!(m.contains("edges: drawn 3 / total 3"), "{m}");
    assert!(m.contains("complete: all 4 files"), "{m}");
}

/// Cycle membership must come from the engine's `circular` finding, never from a second Tarjan here —
/// and it must be visible in the SHAPE, so a renderer with no styling still shows it.
#[test]
fn files_in_a_cycle_get_a_distinct_shape_and_a_thick_arrow() {
    let m = project(&one_tree(), None, DEFAULT_DEP_TOP);
    assert!(
        m.contains("{{\"src/a.ts\"}}"),
        "cycle member is a hexagon:\n{m}"
    );
    assert!(
        m.contains("[\"src/leaf.ts\"]"),
        "non-member stays a box:\n{m}"
    );
    assert!(m.contains("==>"), "cycle edges are thick:\n{m}");
    assert!(m.contains("1 circular finding(s)"), "{m}");
}

/// A tree with no cycle must not print the cycle legend — a legend for something absent is noise, and
/// noise in a disclosure channel is what makes real disclosures ignorable.
#[test]
fn a_tree_with_no_cycle_prints_no_cycle_legend() {
    let v = json!({
        "trees": [{ "sourceId": "web", "output": {
            "ir": { "dep": { "src/a.ts": ["src/b.ts"], "src/b.ts": [] } }, "findings": []
        }}]
    });
    let m = project(&v, None, DEFAULT_DEP_TOP);
    assert!(!m.contains("circular finding"), "{m}");
    assert!(!m.contains("==>"), "{m}");
}

/// The cap drops the LEAST connected files, and says so twice — in the header and in a visible node.
#[test]
fn capping_keeps_the_most_connected_files_and_discloses_the_drop_in_the_picture() {
    let m = project(&one_tree(), None, 2);
    assert!(m.contains("nodes: drawn 2 / in-scope 4 / total 4"), "{m}");
    assert!(
        m.contains("PARTIAL VIEW"),
        "the note node must carry it:\n{m}"
    );
    assert!(m.contains("2 dropped by --top 2"), "{m}");
    assert!(
        !m.contains("src/leaf.ts"),
        "the zero-degree file is the first to go:\n{m}"
    );
}

/// An edge whose other end was capped away must not be drawn as a dangling arrow — and the note says so,
/// because a reader counting arrows would otherwise conclude the file has fewer imports than it has.
#[test]
fn an_edge_to_a_dropped_node_is_not_drawn_and_the_note_explains_it() {
    let m = project(&one_tree(), None, 2);
    let arrows = m.matches("-->").count() + m.matches("==>").count();
    assert!(arrows <= 1, "only edges with both ends kept: {arrows}\n{m}");
    assert!(m.contains("other end was dropped is not drawn"), "{m}");
}

#[test]
fn scope_filters_by_path_prefix_and_the_header_says_how_many_survived() {
    let m = project(&one_tree(), Some("src/a"), DEFAULT_DEP_TOP);
    assert!(m.contains("in-scope 1 / total 4"), "{m}");
    assert!(m.contains("--scope src/a"), "{m}");
}

/// Two trees can both own `src/index.ts`; the ids must not collide, and the label must say which tree.
#[test]
fn two_trees_with_the_same_relative_path_do_not_collide() {
    let v = json!({
        "trees": [
            { "sourceId": "fe", "output": { "ir": { "dep": { "src/index.ts": [] } }, "findings": [] } },
            { "sourceId": "be", "output": { "ir": { "dep": { "src/index.ts": [] } }, "findings": [] } }
        ]
    });
    let m = project(&v, None, DEFAULT_DEP_TOP);
    assert!(m.contains("nodes: drawn 2 / in-scope 2 / total 2"), "{m}");
}

/// Determinism is a contract for every serializer in this repo: the same analysis must render the same
/// bytes, or a committed diagram diffs against itself.
#[test]
fn the_same_analysis_renders_identical_bytes() {
    let v = one_tree();
    assert_eq!(
        project(&v, None, DEFAULT_DEP_TOP),
        project(&v, None, DEFAULT_DEP_TOP)
    );
}

#[test]
fn a_run_with_no_dep_data_still_renders_a_valid_empty_flowchart() {
    let m = project(&json!({ "trees": [] }), None, DEFAULT_DEP_TOP);
    assert!(m.contains("flowchart LR"), "{m}");
    assert!(m.contains("nodes: drawn 0"), "{m}");
}
