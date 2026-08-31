use super::*;
use crate::graph::fold::Fold;
use serde_json::json;

/// The `circular` finding, as the RULE emits it — the one fixture both graph lanes' tests share.
///
/// # Why the fixture is built and not typed
/// [`collect`] read `data["members"]` until 2026-08-30, a key `circular_findings` has never emitted
/// (it emits `data["cycle"]`). The defect survived because BOTH lanes' test files hand-wrote a fixture
/// spelling the same invented key: the tests shared the implementation's wrong assumption, so no
/// assertion over them could see the producer and the consumer disagreeing. A test that authors its own
/// subject cannot notice that its subject does not exist.
///
/// So the fixture is BUILT BY THE RULE and serialized through the same `Finding` serde impl the analyze
/// reply uses. Rename the key in the rule and these tests go red by themselves — the one property the
/// old fixtures could not have. `zzop-rules-graph` is a DEV-dependency for exactly this: this crate's
/// layering forbids a shipped dependency below `zzop-facade`, so a producer/consumer relation it cannot
/// express in types is sealed by a test instead (same T2 shape as the `zzop-rules-http` dev-dependency
/// beside it).
///
/// The member list is passed in because a cycle fixture should state its own cycle; everything ELSE —
/// the anchor the rule picks, the sorted order, the message, `evidencePaths`, and above all the `data`
/// key — comes from `circular_findings`.
pub(in crate::graph) fn circular_finding(members: &[&str]) -> Value {
    let cycle: Vec<String> = members.iter().map(|m| (*m).to_string()).collect();
    let emitted = zzop_rules_graph::circular::circular_findings(std::slice::from_ref(&cycle));
    assert_eq!(
        emitted.len(),
        1,
        "the rule must emit exactly one finding for the cycle {cycle:?} — a fixture built from an \
         EMPTY producer result would make every test below vacuously green, which is the failure mode \
         this helper exists to remove"
    );
    let value = serde_json::to_value(&emitted[0]).expect("a Finding serializes to JSON");
    // A floor, not a restatement of the schema: these two are the only fields the graph lanes read off a
    // finding, and if the serde impl ever stops carrying either, a fixture with a missing `data` would
    // quietly become "this tree reported no cycle" rather than a failure.
    assert_eq!(value["ruleId"], "circular", "{value}");
    assert!(
        value.get("data").is_some_and(serde_json::Value::is_object),
        "the rule's finding must carry a `data` object for the graph lanes to read: {value}"
    );
    value
}

/// One tree, a 3-file cycle plus an unrelated leaf — enough to exercise every branch that shapes the
/// picture without a fixture nobody can read. The `circular` finding is produced BY THE RULE
/// ([`circular_finding`]); a hand-written one is what hid this lane's real defect for months.
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
                "findings": [circular_finding(&["src/a.ts", "src/b.ts", "src/c.ts"])]
            }
        }]
    })
}

/// THE REGRESSION, asserted against the rule's own output rather than against a fixture's spelling.
///
/// `collect` read `data["members"]`, which `circular_findings` has never emitted (it emits
/// `data["cycle"]`), so `cycle_files` collapsed to the ANCHOR — one representative file per cycle. The
/// old code was not silent, it was quietly HALF right, which is why nothing looked broken: on a real
/// tree a 510-file cycle became one cycle file, and both lanes' cycle signals
/// (`inCycle`/`endpointsInCycle`, the hexagon shape, the thick arrow) degraded with it.
#[test]
fn every_member_of_the_rules_own_circular_finding_is_a_cycle_file_not_just_the_anchor() {
    let u = collect(&one_tree());
    assert_eq!(
        u.cycle_files.iter().cloned().collect::<Vec<String>>(),
        vec![
            "src/a.ts".to_string(),
            "src/b.ts".to_string(),
            "src/c.ts".to_string()
        ],
        "every member the rule listed is a cycle file, not only the anchor the rule happened to pick"
    );
    assert_eq!(u.cycles, 1);
}

#[test]
fn every_file_and_edge_is_drawn_when_nothing_is_capped() {
    let m = project(&one_tree(), None, DEFAULT_DEP_TOP, Fold::of(None));
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
    let m = project(&one_tree(), None, DEFAULT_DEP_TOP, Fold::of(None));
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
    let m = project(&v, None, DEFAULT_DEP_TOP, Fold::of(None));
    assert!(!m.contains("circular finding"), "{m}");
    assert!(!m.contains("==>"), "{m}");
}

/// The fe-axios shape, minimized: two files that import EACH OTHER and no `circular` finding — what a
/// type-only import leaves behind after `zzop_core::noncycle` subtracts it from the cycle question and
/// from nothing else. Deliberately carries a third, ordinary edge so a test asserting on the pair
/// cannot pass by the graph being nothing but the pair.
fn mutual_pair_no_cycle() -> Value {
    json!({
        "trees": [{ "sourceId": "web", "output": {
            "ir": { "dep": {
                "src/app.slice.ts": ["src/types/user.ts"],
                "src/types/user.ts": ["src/app.slice.ts"],
                "src/leaf.ts": ["src/types/user.ts"]
            }},
            "findings": []
        }}]
    })
}

/// 🔴 THE DISAGREEMENT, on the surface that shows it most plainly. Before 2026-08-31 this picture drew
/// both arrows of the pair, drew both boxes as ordinary rectangles (no hexagon, no thick arrow), said
/// `cycles reported: 0`, and offered the reader no third line — so the only available reading was that
/// the tool had missed a cycle. Measured on `corpus/oss/fe-axios`, whose
/// `src/components/App/App.slice.ts <-> src/types/user.ts` pair is exactly this.
///
/// The note is gated on the PAIR COUNT and never on `cycles`, which is the whole point: a note that
/// fired only when a cycle was reported would be silent in precisely the run that needs it.
#[test]
fn a_two_way_import_pair_outside_every_cycle_is_disclosed_even_though_no_cycle_was_reported() {
    let u = collect(&mutual_pair_no_cycle());
    assert_eq!(
        u.cycles, 0,
        "the fixture must report NO cycle, or it is not the case under test"
    );
    assert_eq!(
        u.mutual_outside_cycles, 1,
        "one unordered pair, counted once — not twice for its two directions"
    );
    let m = project(
        &mutual_pair_no_cycle(),
        None,
        DEFAULT_DEP_TOP,
        Fold::of(None),
    );
    assert!(
        m.contains("1 file pair(s) import EACH OTHER but are in no reported cycle"),
        "the picture must carry the count, in a NODE rather than a `%%` comment — a comment does not \
         survive rendering, which is this lane's standing rule:\n{m}"
    );
    assert!(
        m.contains("subtracted before cycle detection"),
        "the count alone reads as an admission of a bug; the note has to say the subtraction is \
         deliberate and what it subtracts:\n{m}"
    );
    assert!(
        !m.contains("circular finding"),
        "and it must not invent a cycle legend the engine did not report:\n{m}"
    );
}

/// The other direction — the note is a MEASUREMENT, so a graph with no such pair must not carry it.
/// A disclosure that rides every run is one readers learn to skip.
#[test]
fn a_graph_with_no_two_way_pair_says_nothing_about_erased_imports() {
    let m = project(&one_tree(), None, DEFAULT_DEP_TOP, Fold::of(None));
    assert_eq!(collect(&one_tree()).mutual_outside_cycles, 0, "{m}");
    assert!(!m.contains("import EACH OTHER"), "{m}");
}

/// A pair whose BOTH ends the engine already reported as cycle members is explained by that finding and
/// is not a disagreement — the conservative direction, so this counter can only ever UNDERSTATE.
#[test]
fn a_two_way_pair_inside_a_reported_cycle_is_not_counted_as_a_disagreement() {
    let v = json!({
        "trees": [{ "sourceId": "web", "output": {
            "ir": { "dep": { "src/a.ts": ["src/b.ts"], "src/b.ts": ["src/a.ts"] } },
            "findings": [circular_finding(&["src/a.ts", "src/b.ts"])]
        }}]
    });
    assert_eq!(collect(&v).mutual_outside_cycles, 0);
    assert!(!project(&v, None, DEFAULT_DEP_TOP, Fold::of(None)).contains("import EACH OTHER"));
}

/// The cap drops the LEAST connected files, and says so twice — in the header and in a visible node.
#[test]
fn capping_keeps_the_most_connected_files_and_discloses_the_drop_in_the_picture() {
    let m = project(&one_tree(), None, 2, Fold::of(None));
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
    let m = project(&one_tree(), None, 2, Fold::of(None));
    let arrows = m.matches("-->").count() + m.matches("==>").count();
    assert!(arrows <= 1, "only edges with both ends kept: {arrows}\n{m}");
    assert!(m.contains("other end was dropped is not drawn"), "{m}");
}

#[test]
fn scope_filters_by_path_prefix_and_the_header_says_how_many_survived() {
    let m = project(&one_tree(), Some("src/a"), DEFAULT_DEP_TOP, Fold::of(None));
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
    let m = project(&v, None, DEFAULT_DEP_TOP, Fold::of(None));
    assert!(m.contains("nodes: drawn 2 / in-scope 2 / total 2"), "{m}");
}

/// Determinism is a contract for every serializer in this repo: the same analysis must render the same
/// bytes, or a committed diagram diffs against itself.
#[test]
fn the_same_analysis_renders_identical_bytes() {
    let v = one_tree();
    assert_eq!(
        project(&v, None, DEFAULT_DEP_TOP, Fold::of(None)),
        project(&v, None, DEFAULT_DEP_TOP, Fold::of(None))
    );
}

#[test]
fn a_run_with_no_dep_data_still_renders_a_valid_empty_flowchart() {
    let m = project(
        &json!({ "trees": [] }),
        None,
        DEFAULT_DEP_TOP,
        Fold::of(None),
    );
    assert!(m.contains("flowchart LR"), "{m}");
    assert!(m.contains("nodes: drawn 0"), "{m}");
}
