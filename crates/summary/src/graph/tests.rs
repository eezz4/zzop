//! Unit tests for the `zzop graph` mermaid projection — driven from LITERAL `analyzeTrees` outputs,
//! with no filesystem and no engine, so the format contract is pinned independently of any corpus
//! (same route `facts`/`manifest` take).

use super::{project, DEFAULT_GRAPH_TOP};

/// One tree that serves a route and one that calls it (a joined edge), plus one member of every other
/// bucket, so a single fixture exercises all six graph-shaped buckets at once.
fn engine_output() -> serde_json::Value {
    serde_json::json!({
        "trees": [
            { "sourceId": "api", "output": { "coverage": { "joinContributionZero": false } } },
            { "sourceId": "web", "output": { "coverage": { "joinContributionZero": false } } },
            { "sourceId": "docs", "output": { "coverage": { "joinContributionZero": true } } }
        ],
        "crossLayer": {
            "edges": [{
                "kind": "http", "key": "GET /api/users",
                "from": { "source": "web", "file": "src/client.ts", "line": 3 },
                "to": { "source": "api", "file": "src/routes.ts", "line": 4 },
                "crossSource": true
            }],
            "unconsumedProvides": [{
                "source": "api", "kind": "http", "key": "DELETE /api/users/{}",
                "file": "src/routes.ts", "line": 9
            }],
            "unprovidedConsumes": [{
                "source": "web", "kind": "http", "key": "GET /api/orders",
                "file": "src/orders.ts", "line": 2
            }],
            "unresolvedConsumes": [{
                "source": "web", "kind": "http", "key": null,
                "file": "src/client.ts", "line": 9, "raw": "`${BASE}\n/users`"
            }],
            "externalConsumes": [{
                "source": "web", "kind": "http", "key": "GET https://vendor.com/v1/ping",
                "file": "src/vendor.ts", "line": 1
            }],
            "ambiguousConsumes": [{
                "source": "web", "kind": "http", "key": "GET /health",
                "file": "src/health.ts", "line": 1,
                "candidates": [
                    { "source": "api", "kind": "http", "key": "GET /health", "file": "a.ts", "line": 1 },
                    { "source": "docs", "kind": "http", "key": "GET /health", "file": "b.ts", "line": 1 }
                ]
            }]
        }
    })
}

fn rendered() -> String {
    project(&engine_output(), None, DEFAULT_GRAPH_TOP)
}

#[test]
fn the_document_is_a_mermaid_flowchart_with_one_subgraph_per_analyzed_source() {
    let out = rendered();
    assert!(
        out.starts_with("flowchart LR\n"),
        "the diagram declaration must LEAD the document — a renderer decides the diagram type from \
         it, so the `%%` census follows it inside the body: {out}"
    );
    for source in ["api", "web", "docs"] {
        assert!(
            out.contains(&format!("[\"{source}\"]")),
            "every analyzed source needs its own subgraph, missing {source}: {out}"
        );
    }
}

/// Every non-edge bucket the ENGINE ships has a declared role in this projection. Derived from
/// `crate::output::KEY_BUCKETS` — the same five-bucket vocabulary `distinct_bucket_keys`/`manifest` walk — so a
/// sixth bucket cannot reach the engine and skip the picture. Before `bucket_role` existed,
/// `collect_bucket`'s `_ => (CONSUME, "ambiguous")` wildcard absorbed exactly that case: a new bucket
/// was drawn under a role the test below already expected, and both stayed green.
#[test]
fn every_join_bucket_has_an_explicit_graph_role() {
    let buckets = crate::output::KEY_BUCKETS;
    assert!(
        !buckets.is_empty(),
        "KEY_BUCKETS is empty — an empty subject set must be RED, never a silent pass"
    );
    let unmapped: Vec<&str> = buckets
        .iter()
        .copied()
        .filter(|b| super::collect::bucket_role(b).is_none())
        .collect();
    assert!(
        unmapped.is_empty(),
        "these cross-layer buckets have no declared (side, role) in `collect::bucket_role`, so \
         `zzop graph` draws nothing for them: {unmapped:?}"
    );
}

/// Every one of the graph-shaped buckets reaches the picture. This is the "a diagram that silently
/// omits a bucket is not acceptable" contract, pinned per bucket rather than by total node count — and
/// per ENGINE bucket rather than per hand-listed role word, so the fixture above has to grow with the
/// vocabulary instead of the assertion quietly covering one bucket twice. `linked`/`candidate` are the
/// two roles that come from `edges` rather than a bucket, so they are named directly.
#[test]
fn every_join_bucket_is_represented_by_a_labelled_node() {
    let out = rendered();
    let bucket_roles: Vec<&str> = crate::output::KEY_BUCKETS
        .iter()
        .map(|b| {
            super::collect::bucket_role(b)
                .unwrap_or_else(|| panic!("{b} has no declared graph role"))
                .1
        })
        .collect();
    assert!(
        !bucket_roles.is_empty(),
        "KEY_BUCKETS is empty — an empty subject set must be RED, never a silent pass"
    );
    for role in bucket_roles.into_iter().chain(["linked", "candidate"]) {
        assert!(
            out.contains(&format!("{role} · http")),
            "role {role} has no node in the rendered graph — if a bucket was just added, the fixture \
             owes it a member: {out}"
        );
    }
}

#[test]
fn a_joined_edge_is_drawn_from_the_consumer_node_to_the_provider_node() {
    let out = rendered();
    // The consumer side is a stadium node, the provider side a rectangle — and exactly one plain
    // arrow (the ambiguous candidates are dotted).
    assert!(
        out.contains("([\"linked · http GET /api/users\"])"),
        "the consume side of a joined edge must be a stadium node: {out}"
    );
    assert!(
        out.contains("[\"linked · http GET /api/users\"]:::linked"),
        "the provide side of a joined edge must be a rectangle node: {out}"
    );
    assert_eq!(
        out.lines().filter(|l| l.contains(" --> ")).count(),
        1,
        "exactly one resolved arrow is expected from this fixture: {out}"
    );
    assert_eq!(
        out.lines().filter(|l| l.contains(" -. \"ambiguous\" .-> ")).count(),
        2,
        "each ambiguous candidate gets its own DOTTED arrow — a guess is never drawn like a resolved join: {out}"
    );
}

/// A tree that extracted nothing joinable must still appear, and must say WHICH zero it is — blindness
/// is not the same claim as an empty contract.
#[test]
fn a_source_that_contributed_nothing_to_the_join_still_appears_and_says_why() {
    let mut engine = engine_output();
    engine["crossLayer"]["ambiguousConsumes"] = serde_json::json!([]);
    let out = project(&engine, None, DEFAULT_GRAPH_TOP);
    assert!(
        out.contains("extracted no joinable io"),
        "a joinContributionZero source must disclose its blindness inside its own subgraph: {out}"
    );
}

/// The truncation contract: a capped graph says so in the `%%` census AND on the canvas, where a
/// comment does not survive rendering.
#[test]
fn truncation_is_disclosed_in_the_header_census_and_as_a_visible_node() {
    let mut engine = engine_output();
    engine["crossLayer"]["unprovidedConsumes"] = serde_json::json!([
        { "source": "web", "kind": "http", "key": "GET /a", "file": "src/a.ts", "line": 1 },
        { "source": "web", "kind": "http", "key": "GET /b", "file": "src/b.ts", "line": 1 },
        { "source": "web", "kind": "http", "key": "GET /c", "file": "src/c.ts", "line": 1 }
    ]);
    let out = project(&engine, None, 1);

    assert!(
        out.contains("%%   unprovidedConsumes: 1/3/3 from 3 site(s)"),
        "the header census must carry shown/inScope/total for the capped bucket: {out}"
    );
    let note = out
        .lines()
        .find(|l| l.contains("TRUNCATED"))
        .unwrap_or_else(|| panic!("no visible truncation node in the rendered graph: {out}"));
    assert!(
        note.contains(":::note") && note.contains("unprovidedConsumes 1/3"),
        "the truncation node must be a real graph node naming the capped bucket: {note}"
    );
    assert!(
        out.contains("GET /a") && !out.contains("GET /b"),
        "capping must keep the first row in sorted order and drop the rest: {out}"
    );
}

/// `--scope` is the other honesty knob: what it removed is stated in both channels too.
#[test]
fn scoping_is_disclosed_in_the_header_and_as_a_visible_node() {
    let out = project(&engine_output(), Some("api"), DEFAULT_GRAPH_TOP);
    assert!(
        out.contains("%% scope: api"),
        "the header must name the active scope: {out}"
    );
    let note = out
        .lines()
        .find(|l| l.contains("SCOPED to 'api'"))
        .unwrap_or_else(|| panic!("no visible scope node in the rendered graph: {out}"));
    assert!(
        note.contains("relation(s) outside the scope are not drawn"),
        "the scope node must state that relations were withheld: {note}"
    );
    assert!(
        !out.contains("GET /api/orders"),
        "a row whose source and file are both outside the scope must not be drawn: {out}"
    );
    assert!(
        out.contains("linked · http GET /api/users"),
        "an edge with a site inside the scope is kept: {out}"
    );
}

/// An item with neither `key` nor `raw` cannot be labelled. It is counted and disclosed, never guessed
/// and never silently dropped.
#[test]
fn an_unlabelable_item_is_counted_in_the_header_instead_of_being_silently_dropped() {
    let mut engine = engine_output();
    engine["crossLayer"]["unresolvedConsumes"] = serde_json::json!([
        { "source": "web", "kind": "http", "key": null, "file": "src/x.ts", "line": 1 }
    ]);
    let out = project(&engine, None, DEFAULT_GRAPH_TOP);
    assert!(
        out.contains("%%   unresolvedConsumes: 0/0/0 from 1 site(s) (+1 with neither key nor raw"),
        "an unlabelable item must appear in the census as a disclosed remainder: {out}"
    );
}

/// The four things this format structurally cannot carry are named in the document, so a reader never
/// has to infer completeness from a picture.
#[test]
fn the_header_names_what_the_format_cannot_render() {
    let out = rendered();
    for absent in [
        "crossLayerFindings",
        "hostRekeyCounts",
        "CALL SITES ARE AGGREGATED",
    ] {
        assert!(
            out.contains(absent),
            "the header must disclose that {absent} is not in this surface: {out}"
        );
    }
}

/// A label can carry arbitrary source text (a raw call-site expression). Newlines would break the
/// document itself, and mermaid's structural characters would corrupt the label.
#[test]
fn labels_are_sanitized_so_raw_source_text_cannot_break_the_document() {
    let mut engine = engine_output();
    engine["crossLayer"]["externalConsumes"] = serde_json::json!([{
        "source": "web", "kind": "http", "key": null,
        "file": "src/v.ts", "line": 1, "raw": "fetch(\"#<a>\"\n  + x)"
    }]);
    let out = project(&engine, None, DEFAULT_GRAPH_TOP);
    let line = out
        .lines()
        .find(|l| l.contains("external ·"))
        .unwrap_or_else(|| panic!("the external node vanished: {out}"));
    assert!(
        line.contains("fetch(#quot;#35;#lt;a#gt;#quot; + x)"),
        "quotes/#/<> must become mermaid entity codes and newlines must collapse: {line}"
    );
}

/// Determinism: same input, same options, byte-identical document — including node ids, which are
/// positional over a sorted map.
#[test]
fn the_document_is_byte_stable_for_the_same_input() {
    assert_eq!(rendered(), rendered());
    let reordered = serde_json::json!({
        "trees": engine_output()["trees"].as_array().unwrap().iter().rev().cloned().collect::<Vec<_>>(),
        "crossLayer": engine_output()["crossLayer"],
    });
    assert_eq!(
        rendered(),
        project(&reordered, None, DEFAULT_GRAPH_TOP),
        "tree REQUEST order must not change the drawn graph — sources sort by id"
    );
}

/// A document with no join at all is still a valid, honest document: the census reports six zeros and
/// every source keeps a subgraph.
#[test]
fn an_empty_join_still_renders_a_valid_document_with_a_full_census() {
    let out = project(
        &serde_json::json!({ "trees": [{ "sourceId": "solo", "output": {} }], "crossLayer": {} }),
        None,
        DEFAULT_GRAPH_TOP,
    );
    assert!(
        out.starts_with(
            "flowchart LR
"
        ),
        "{out}"
    );
    for bucket in [
        "edges",
        "unconsumedProvides",
        "unprovidedConsumes",
        "unresolvedConsumes",
        "externalConsumes",
        "ambiguousConsumes",
    ] {
        assert!(
            out.contains(&format!("%%   {bucket}: 0/0/0 from 0 site(s)")),
            "the census must carry every bucket even when the whole join is empty: {out}"
        );
    }
    assert!(
        out.contains("no rows in this view"),
        "an empty subgraph must say it is empty rather than render as a bare box: {out}"
    );
}

/// Rows are call SITES; nodes and arrows are relations. `--top` must cap what is DRAWN, or the
/// disclosure describes a picture the reader is not looking at — measured on the OSS corpus, where 60
/// `edges` rows collapse to 8 arrows and a row-based `--top 5` drew exactly one of them.
#[test]
fn repeated_call_sites_collapse_into_one_relation_and_the_census_publishes_both_scales() {
    let mut engine = engine_output();
    // Three call sites in `web` for the same missing route, in three different files.
    engine["crossLayer"]["unprovidedConsumes"] = serde_json::json!([
        { "source": "web", "kind": "http", "key": "GET /api/orders", "file": "src/a.ts", "line": 1 },
        { "source": "web", "kind": "http", "key": "GET /api/orders", "file": "src/b.ts", "line": 1 },
        { "source": "web", "kind": "http", "key": "GET /api/orders", "file": "src/c.ts", "line": 1 }
    ]);
    let out = project(&engine, None, DEFAULT_GRAPH_TOP);
    assert!(
        out.contains("%%   unprovidedConsumes: 1/1/1 from 3 site(s)"),
        "one relation drawn, and the three sites it aggregates published beside it: {out}"
    );
    assert_eq!(
        out.lines()
            .filter(|l| l.contains("unprovided · http GET /api/orders"))
            .count(),
        1,
        "three sites must draw exactly one node: {out}"
    );
    // ...and the cap is not spent on the duplicates: at --top 1 this bucket is complete, not truncated.
    let capped = project(&engine, None, 1);
    assert!(
        !capped.contains("TRUNCATED"),
        "a bucket whose sites all collapse into one relation is not truncated at --top 1: {capped}"
    );
}
