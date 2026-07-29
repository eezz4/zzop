//! Unit tests for `cross-layer/all-consumes-unjoined`. Every fixture below is a reduction of a MEASURED
//! shape from the 2026-07-29 dogfood run (17 trees, template vocabulary on) — the module doc names the two
//! real trees each one stands for.

use std::collections::BTreeSet;

use zzop_core::io::{AmbiguousConsume, EdgeFrom, EdgeTo, IoConsume, TaggedConsume, TaggedProvide};
use zzop_core::{CrossLayerEdge, CrossLayerResult, Finding, IoProvide, Severity};

use super::{all_consumes_unjoined_findings, retain_non_subsumed_sources, MIN_UNJOINED_CONSUMES};

fn consume(kind: &str, key: &str, file: &str, line: u32) -> IoConsume {
    IoConsume {
        kind: kind.to_string(),
        key: Some(key.to_string()),
        file: file.to_string(),
        line,
        raw: None,
        method: None,
        retry_configured: None,
        body: None,
        client: None,
    }
}

fn tagged(source: &str, kind: &str, key: &str, file: &str, line: u32) -> TaggedConsume {
    TaggedConsume {
        source: source.to_string(),
        consume: consume(kind, key, file, line),
    }
}

fn provide(source: &str, kind: &str, key: &str) -> TaggedProvide {
    TaggedProvide {
        source: source.to_string(),
        provide: IoProvide {
            body: None,
            kind: kind.to_string(),
            key: key.to_string(),
            file: "be/routes.ts".to_string(),
            line: 1,
            symbol: None,
        },
    }
}

fn ambiguous(
    source: &str,
    key: &str,
    file: &str,
    line: u32,
    candidates: usize,
) -> AmbiguousConsume {
    AmbiguousConsume {
        source: source.to_string(),
        consume: consume("http", key, file, line),
        candidates: (0..candidates)
            .map(|i| provide(&format!("be-{i}"), "http", key))
            .collect(),
    }
}

fn edge(source: &str, key: &str) -> CrossLayerEdge {
    CrossLayerEdge {
        kind: "http".to_string(),
        key: key.to_string(),
        from: EdgeFrom {
            source: source.to_string(),
            file: "fe/api.ts".to_string(),
            line: 1,
        },
        to: EdgeTo {
            source: "be".to_string(),
            file: "be/routes.ts".to_string(),
            line: 1,
            symbol: None,
        },
        cross_source: true,
        low_confidence_reason: None,
    }
}

/// Baseline `CrossLayerResult` with one http provide present, so `run_has_http_provides` is satisfied and
/// each test varies only the axis it is about.
fn base() -> CrossLayerResult {
    CrossLayerResult {
        edges: Vec::new(),
        unconsumed_provides: vec![provide("be", "http", "GET /api/articles")],
        unprovided_consumes: Vec::new(),
        unresolved_consumes: Vec::new(),
        external_consumes: Vec::new(),
        ambiguous_consumes: Vec::new(),
        host_rekey_counts: Vec::new(),
    }
}

/// The `be-fastapi-fs` shape: a tree whose calls carry `/api/v1/...` while its own routes key without the
/// prefix, so every call lands unprovided and 17 `unprovided-mutation-call` findings followed.
#[test]
fn a_tree_whose_every_call_is_unprovided_gets_one_finding_anchored_at_its_first_site() {
    let mut cl = base();
    cl.unprovided_consumes = vec![
        tagged("fe", "http", "GET /api/v1/items", "sdk.gen.ts", 19),
        tagged("fe", "http", "POST /api/v1/items", "sdk.gen.ts", 61),
        tagged("fe", "http", "DELETE /api/v1/items/{}", "sdk.gen.ts", 106),
    ];
    let out = all_consumes_unjoined_findings(&cl, &BTreeSet::new(), &BTreeSet::new());
    assert_eq!(out.findings.len(), 1);
    let f = &out.findings[0];
    assert_eq!(f.rule_id, "cross-layer/all-consumes-unjoined");
    assert_eq!(f.severity, Severity::Info);
    assert_eq!((f.file.as_str(), f.line), ("sdk.gen.ts", 19));
    let data = f.data.as_ref().unwrap();
    assert_eq!(data["consumeCount"], 3);
    assert_eq!(data["unprovidedCount"], 3);
    assert_eq!(data["ambiguousCount"], 0);
    assert_eq!(out.subsumed_sources, BTreeSet::from(["fe".to_string()]));
}

/// The `fe-axios` shape: keys resolve, but ten sibling RealWorld backends provide each one, so the linker
/// refuses to pick and every call lands ambiguous — 19 findings apiece, four trees over.
#[test]
fn a_tree_whose_every_call_is_ambiguous_is_folded_the_same_way() {
    let mut cl = base();
    cl.ambiguous_consumes = vec![
        ambiguous("fe-axios", "GET /articles", "pages/articles.ts", 18, 8),
        ambiguous("fe-axios", "POST /articles", "pages/articles.ts", 39, 8),
        ambiguous("fe-axios", "GET /user", "pages/user.ts", 4, 8),
    ];
    let out = all_consumes_unjoined_findings(&cl, &BTreeSet::new(), &BTreeSet::new());
    assert_eq!(out.findings.len(), 1);
    let data = out.findings[0].data.as_ref().unwrap();
    assert_eq!(data["ambiguousCount"], 3);
    assert_eq!(data["unprovidedCount"], 0);
    // Anchor is the (file, line)-sorted first site: `pages/articles.ts` sorts before `pages/user.ts`, so
    // the anchor is that file's lowest line — NOT the lowest line tree-wide.
    assert_eq!(
        (out.findings[0].file.as_str(), out.findings[0].line),
        ("pages/articles.ts", 18)
    );
}

/// The whole point of the rule: a tree that joined even ONE call is not join-blind, and its remaining
/// unjoined calls are ordinary per-call findings, not a base-path story.
#[test]
fn a_tree_with_even_one_http_edge_is_never_folded() {
    let mut cl = base();
    cl.edges = vec![edge("fe", "GET /api/articles")];
    cl.unprovided_consumes = vec![
        tagged("fe", "http", "GET /a", "fe/api.ts", 2),
        tagged("fe", "http", "GET /b", "fe/api.ts", 3),
        tagged("fe", "http", "GET /c", "fe/api.ts", 4),
    ];
    let out = all_consumes_unjoined_findings(&cl, &BTreeSet::new(), &BTreeSet::new());
    assert!(out.findings.is_empty());
    assert!(out.subsumed_sources.is_empty());
}

/// Blaming a lone front end for not joining would blame the user for the shape of their own invocation.
#[test]
fn a_run_with_no_http_provides_anywhere_stays_silent() {
    let mut cl = base();
    cl.unconsumed_provides = vec![provide("be", "db-table", "table:users")];
    cl.unprovided_consumes = vec![
        tagged("fe", "http", "GET /a", "fe/api.ts", 2),
        tagged("fe", "http", "GET /b", "fe/api.ts", 3),
        tagged("fe", "http", "GET /c", "fe/api.ts", 4),
    ];
    assert!(
        all_consumes_unjoined_findings(&cl, &BTreeSet::new(), &BTreeSet::new())
            .findings
            .is_empty()
    );
}

/// Below the floor the per-call findings ARE the readable form — see `MIN_UNJOINED_CONSUMES`'s doc.
#[test]
fn fires_at_threshold_not_below() {
    let mk = |n: usize| {
        let mut cl = base();
        cl.unprovided_consumes = (0..n)
            .map(|i| {
                tagged(
                    "fe",
                    "http",
                    &format!("GET /r{i}"),
                    "fe/api.ts",
                    i as u32 + 1,
                )
            })
            .collect();
        all_consumes_unjoined_findings(&cl, &BTreeSet::new(), &BTreeSet::new())
            .findings
            .len()
    };
    assert_eq!(mk(MIN_UNJOINED_CONSUMES - 1), 0);
    assert_eq!(mk(MIN_UNJOINED_CONSUMES), 1);
}

/// Third-party egress is SUPPOSED not to join, and extractor blindness has its own aggregate
/// (`unresolved-consume-ratio`). Neither may push a tree over the floor — otherwise a tree that calls only
/// Stripe, or one that extracted nothing, reads as a base-path failure.
#[test]
fn external_and_unresolved_consumes_never_count_toward_the_floor() {
    let mut cl = base();
    cl.external_consumes = vec![
        tagged(
            "fe",
            "http",
            "GET https://api.stripe.com/v1/charges",
            "fe/pay.ts",
            1,
        ),
        tagged(
            "fe",
            "http",
            "POST https://api.stripe.com/v1/refunds",
            "fe/pay.ts",
            2,
        ),
        tagged(
            "fe",
            "http",
            "GET https://api.stripe.com/v1/events",
            "fe/pay.ts",
            3,
        ),
    ];
    cl.unresolved_consumes = vec![
        tagged("fe", "http", "GET /{}", "fe/wrap.ts", 1),
        tagged("fe", "http", "GET /{}", "fe/wrap.ts", 2),
        tagged("fe", "http", "GET /{}", "fe/wrap.ts", 3),
    ];
    assert!(
        all_consumes_unjoined_findings(&cl, &BTreeSet::new(), &BTreeSet::new())
            .findings
            .is_empty()
    );
}

/// The `xlayer-fe` benchmark shape that forced the `diagnosed` precondition into existence: five internal
/// calls, none joined, but three of them ALREADY carry a specific diagnosis (method mismatch, version skew,
/// path near-miss). Only 2 are unexplained — below the floor — so "one unresolved base path" is never
/// asserted over a tree whose calls each break in their own way.
#[test]
fn consumes_another_rule_already_diagnosed_do_not_count_toward_the_floor() {
    let mut cl = base();
    cl.unprovided_consumes = vec![
        tagged("xfe", "http", "POST /widgets", "consumes.ts", 5),
        tagged("xfe", "http", "GET /v2/gadgets", "consumes.ts", 6),
        tagged("xfe", "http", "DELETE /missing", "consumes.ts", 9),
        tagged("xfe", "http", "GET /items/detail", "consumes.ts", 12),
    ];
    cl.ambiguous_consumes = vec![ambiguous("xfe", "GET /widgets", "consumes.ts", 11, 2)];

    // With nothing diagnosed, 5 unexplained calls clear the floor and the tree folds.
    assert_eq!(
        all_consumes_unjoined_findings(&cl, &BTreeSet::new(), &BTreeSet::new())
            .findings
            .len(),
        1
    );

    // With the three real diagnoses in hand, 2 remain — below the floor, silent.
    let diagnosed = BTreeSet::from([
        ("xfe".to_string(), "consumes.ts".to_string(), 5),
        ("xfe".to_string(), "consumes.ts".to_string(), 6),
        ("xfe".to_string(), "consumes.ts".to_string(), 12),
    ]);
    let out = all_consumes_unjoined_findings(&cl, &diagnosed, &BTreeSet::new());
    assert!(out.findings.is_empty(), "{:?}", out.findings);
    assert!(out.subsumed_sources.is_empty());
}

/// A diagnosis belongs to ONE tree: the same relative path and line in a different tree must not be
/// discounted. `cases/` has several trees sharing a relative path, which is why the anchor is a triple.
#[test]
fn a_diagnosis_in_one_tree_never_discounts_the_same_anchor_in_another() {
    let mut cl = base();
    cl.unprovided_consumes = vec![
        tagged("other", "http", "GET /a", "consumes.ts", 5),
        tagged("other", "http", "GET /b", "consumes.ts", 6),
        tagged("other", "http", "GET /c", "consumes.ts", 7),
    ];
    let diagnosed = BTreeSet::from([
        ("xfe".to_string(), "consumes.ts".to_string(), 5),
        ("xfe".to_string(), "consumes.ts".to_string(), 6),
        ("xfe".to_string(), "consumes.ts".to_string(), 7),
    ]);
    assert_eq!(
        all_consumes_unjoined_findings(&cl, &diagnosed, &BTreeSet::new())
            .findings
            .len(),
        1
    );
}

/// The `unresolved` benchmark fixture: a tree whose URLs are built from a variable `base`, so
/// `cross-layer/unresolved-consume-ratio` already reports it blind. This rule must stand down rather than
/// become the third co-firer in a family built to partition — see `blind_sources` in the module doc.
#[test]
fn a_tree_already_reported_blind_by_unresolved_consume_ratio_is_left_to_that_rule() {
    let mut cl = base();
    cl.unprovided_consumes = vec![
        tagged("unresolved", "http", "GET /detail", "gateway.ts", 11),
        tagged("unresolved", "http", "GET /health", "gateway.ts", 15),
        tagged("unresolved", "http", "GET /zz-telemetry", "gateway.ts", 10),
    ];
    // Without the partition it fires: three unexplained calls, nothing joined.
    assert_eq!(
        all_consumes_unjoined_findings(&cl, &BTreeSet::new(), &BTreeSet::new())
            .findings
            .len(),
        1
    );
    let blind = BTreeSet::from(["unresolved".to_string()]);
    let out = all_consumes_unjoined_findings(&cl, &BTreeSet::new(), &blind);
    assert!(out.findings.is_empty(), "{:?}", out.findings);
    // And it must not subsume either — standing down means the per-call findings stay.
    assert!(out.subsumed_sources.is_empty());
}

/// db-table joins are intra-tree by nature and say nothing about route topology.
#[test]
fn db_table_consumes_are_not_this_rules_population() {
    let mut cl = base();
    cl.unprovided_consumes = vec![
        tagged("be", "db-table", "table:a", "be/repo.ts", 1),
        tagged("be", "db-table", "table:b", "be/repo.ts", 2),
        tagged("be", "db-table", "table:c", "be/repo.ts", 3),
    ];
    assert!(
        all_consumes_unjoined_findings(&cl, &BTreeSet::new(), &BTreeSet::new())
            .findings
            .is_empty()
    );
}

/// `output-philosophy.md` §0/§1 — a replacement must disclose what it replaced. The message has to carry
/// the count, the bucket split, real keys, and an exclude hint.
#[test]
fn the_message_discloses_count_split_keys_and_how_to_exclude() {
    let mut cl = base();
    cl.unprovided_consumes = vec![
        tagged("fe", "http", "GET /articles", "fe/api.ts", 1),
        tagged("fe", "http", "POST /articles", "fe/api.ts", 2),
    ];
    cl.ambiguous_consumes = vec![ambiguous("fe", "GET /user", "fe/api.ts", 3, 4)];
    let out = all_consumes_unjoined_findings(&cl, &BTreeSet::new(), &BTreeSet::new());
    let m = &out.findings[0].message;
    assert!(m.contains("and 3 of them"), "count: {m}");
    assert!(
        m.contains("not one internal http call"),
        "the totality claim: {m}"
    );
    assert!(m.contains("2 with no provider anywhere"), "split: {m}");
    assert!(m.contains("1 matching 2+ trees"), "split: {m}");
    assert!(m.contains("GET /articles"), "keys: {m}");
    assert!(m.contains("disabled_rules"), "exclude: {m}");
    // §9: only knobs that EXIST may be named. Both sides now have one, and the consume-side knob is the
    // one this finding's own diagnosis points at — a message that named only the serving side would be
    // sending a reader with a calling-side base to the wrong end of the join.
    assert!(
        m.contains("trees[].topology.mountedAt"),
        "serving knob: {m}"
    );
    assert!(
        m.contains("trees[].topology.clientBase"),
        "calling knob: {m}"
    );
    // The pre-2026-07-29 wording, kept as a pin: this sentence became false the day the knob shipped, and
    // an aggregate that tells a user their repair does not exist is worse than one that stays silent.
    assert!(
        !m.contains("NO declarative knob"),
        "stale absence claim: {m}"
    );
}

/// A truncated sample must never read as the whole set.
#[test]
fn a_truncated_key_sample_still_reports_the_honest_total() {
    let mut cl = base();
    cl.unprovided_consumes = (0..12)
        .map(|i| tagged("fe", "http", &format!("GET /r{i:02}"), "fe/api.ts", i + 1))
        .collect();
    let out = all_consumes_unjoined_findings(&cl, &BTreeSet::new(), &BTreeSet::new());
    let f = &out.findings[0];
    let data = f.data.as_ref().unwrap();
    assert_eq!(data["consumeCount"], 12);
    assert_eq!(data["distinctKeyCount"], 12);
    assert_eq!(data["sampleKeys"].as_array().unwrap().len(), 8);
    assert!(f.message.contains("+4 more"), "{}", f.message);
}

#[test]
fn two_blind_trees_each_get_their_own_finding_sorted_deterministically() {
    let mut cl = base();
    cl.unprovided_consumes = vec![
        tagged("z-fe", "http", "GET /a", "z/api.ts", 1),
        tagged("z-fe", "http", "GET /b", "z/api.ts", 2),
        tagged("z-fe", "http", "GET /c", "z/api.ts", 3),
        tagged("a-fe", "http", "GET /a", "a/api.ts", 1),
        tagged("a-fe", "http", "GET /b", "a/api.ts", 2),
        tagged("a-fe", "http", "GET /c", "a/api.ts", 3),
    ];
    let out = all_consumes_unjoined_findings(&cl, &BTreeSet::new(), &BTreeSet::new());
    let sites: Vec<&str> = out.findings.iter().map(|f| f.file.as_str()).collect();
    assert_eq!(sites, vec!["a/api.ts", "z/api.ts"]);
    assert_eq!(
        out.subsumed_sources,
        BTreeSet::from(["a-fe".to_string(), "z-fe".to_string()])
    );
}

// ---------------------------------------------------------------------------------------------
// retain_non_subsumed_sources
// ---------------------------------------------------------------------------------------------

fn finding(rule_id: &str, source: Option<&str>) -> Finding {
    Finding {
        rule_id: rule_id.to_string(),
        severity: Severity::Warning,
        file: "fe/api.ts".to_string(),
        line: 1,
        message: String::new(),
        evidence_paths: Vec::new(),
        data: source.map(|s| serde_json::json!({ "consumeSource": s })),
    }
}

#[test]
fn only_the_two_replaced_rule_ids_from_a_folded_tree_are_dropped() {
    let subsumed = BTreeSet::from(["fe".to_string()]);
    let kept = retain_non_subsumed_sources(
        vec![
            finding("cross-layer/ambiguous-consume", Some("fe")),
            finding("cross-layer/unprovided-mutation-call", Some("fe")),
            // same tree, but an AGGREGATE — never subsumed (it names the actual prefix).
            finding("cross-layer/prefix-drift", Some("fe")),
            // a different tree that joined fine.
            finding("cross-layer/ambiguous-consume", Some("other")),
        ],
        &subsumed,
    );
    let ids: Vec<&str> = kept.iter().map(|f| f.rule_id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["cross-layer/prefix-drift", "cross-layer/ambiguous-consume"]
    );
    assert_eq!(kept[1].data.as_ref().unwrap()["consumeSource"], "other");
}

/// An unrecognized shape must never vanish silently — that is the failure mode this whole rule exists to
/// remove, and re-introducing it inside the fold would be the worst possible place for it.
#[test]
fn a_replaced_rule_with_no_consume_source_is_kept() {
    let kept = retain_non_subsumed_sources(
        vec![finding("cross-layer/ambiguous-consume", None)],
        &BTreeSet::from(["fe".to_string()]),
    );
    assert_eq!(kept.len(), 1);
}
