use super::*;
use crate::cross_layer::unconsumed_mutation_endpoint::{
    reported_provide_sites, unconsumed_mutation_endpoint_findings,
};

fn dead(key: &str, source: &str, file: &str, line: u32) -> TaggedProvide {
    TaggedProvide {
        source: source.to_string(),
        provide: zzop_core::IoProvide {
            body: None,
            kind: "http".to_string(),
            key: key.to_string(),
            file: file.to_string(),
            line,
            symbol: None,
        },
    }
}

fn dead_kind(kind: &str, key: &str, source: &str, file: &str, line: u32) -> TaggedProvide {
    TaggedProvide {
        source: source.to_string(),
        provide: zzop_core::IoProvide {
            body: None,
            kind: kind.to_string(),
            key: key.to_string(),
            file: file.to_string(),
            line,
            symbol: None,
        },
    }
}

fn unresolved(kind: &str, source: &str) -> TaggedConsume {
    TaggedConsume {
        source: source.to_string(),
        consume: zzop_core::IoConsume {
            client: None,
            body: None,
            kind: kind.to_string(),
            key: None,
            file: "dyn.ts".to_string(),
            line: 1,
            raw: Some("dyn".to_string()),
            method: None,
            retry_configured: None,
        },
    }
}

fn no_near_miss() -> BTreeMap<(String, String, u32), NearMissTargetRef> {
    BTreeMap::new()
}

fn no_trpc() -> BTreeSet<String> {
    BTreeSet::new()
}

fn trpc_sources(sources: &[&str]) -> BTreeSet<String> {
    sources.iter().map(|s| s.to_string()).collect()
}

/// The `cross-layer/unconsumed-mutation-endpoint` rule reported nothing this run — either it found no write
/// route, or the user disabled it. Both cases must leave this rule's coverage complete.
fn none_reported() -> BTreeSet<(String, String, u32, String)> {
    BTreeSet::new()
}

/// The general rule as the engine calls it when the specialization is ENABLED: its real output decides what
/// is suppressed. Deliberately not a hand-written site set — that would re-introduce the drift this wiring
/// exists to prevent.
fn with_mutation_rule_enabled(provides: &[TaggedProvide]) -> (Vec<Finding>, Vec<Finding>) {
    let specialized = unconsumed_mutation_endpoint_findings(
        provides,
        &[],
        &BTreeSet::new(),
        &no_near_miss(),
        &no_trpc(),
    );
    let general = unconsumed_endpoint_findings(
        provides,
        &[],
        &no_near_miss(),
        &no_trpc(),
        &reported_provide_sites(&specialized),
        EXTERNALLY_FETCHED_PATHS,
    );
    (general, specialized)
}

#[test]
fn dead_http_provide_is_flagged_with_source_and_anchor() {
    let out = unconsumed_endpoint_findings(
        &[dead("GET /orphan", "be", "Api.java", 12)],
        &[],
        &no_near_miss(),
        &no_trpc(),
        &none_reported(),
        EXTERNALLY_FETCHED_PATHS,
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].rule_id, "cross-layer/unconsumed-endpoint");
    assert_eq!(out[0].severity, Severity::Info);
    assert_eq!(out[0].file, "Api.java");
    assert_eq!(out[0].line, 12);
    assert!(out[0].message.contains("GET /orphan"));
    assert!(out[0].message.contains("source `be`"));
    assert!(out[0].message.contains("disabled_rules"));
    assert!(!out[0].message.contains("near-miss"));
}

#[test]
fn dead_provide_registered_in_a_test_fixture_file_is_skipped() {
    let out = unconsumed_endpoint_findings(
        &[dead(
            "GET /fixture",
            "be",
            "src/api/__test__/handlers.test.ts",
            125,
        )],
        &[],
        &no_near_miss(),
        &no_trpc(),
        &none_reported(),
        EXTERNALLY_FETCHED_PATHS,
    );
    assert!(out.is_empty());
}

#[test]
fn non_http_dead_provide_is_ignored() {
    let out = unconsumed_endpoint_findings(
        &[dead_kind("db-table", "table:users", "db", "schema.sql", 1)],
        &[],
        &no_near_miss(),
        &no_trpc(),
        &none_reported(),
        EXTERNALLY_FETCHED_PATHS,
    );
    assert!(out.is_empty());
}

#[test]
fn no_unconsumed_provides_is_empty() {
    assert!(unconsumed_endpoint_findings(
        &[],
        &[],
        &no_near_miss(),
        &no_trpc(),
        &none_reported(),
        EXTERNALLY_FETCHED_PATHS
    )
    .is_empty());
}

#[test]
fn message_states_the_unresolved_http_count_honestly() {
    let out = unconsumed_endpoint_findings(
        &[dead("GET /orphan", "be", "Api.java", 12)],
        &[
            unresolved("http", "fe"),
            unresolved("http", "fe"),
            unresolved("queue", "fe"), // not http — must not inflate the count
        ],
        &no_near_miss(),
        &no_trpc(),
        &none_reported(),
        EXTERNALLY_FETCHED_PATHS,
    );
    assert_eq!(out.len(), 1);
    assert!(out[0].message.contains("2 unresolved"));
}

#[test]
fn multiple_unconsumed_provides_are_sorted_by_file_then_line() {
    let out = unconsumed_endpoint_findings(
        &[
            dead("GET /b", "be", "z.java", 1),
            dead("GET /a", "be", "a.java", 9),
            dead("GET /c", "be", "a.java", 2),
        ],
        &[],
        &no_near_miss(),
        &no_trpc(),
        &none_reported(),
        EXTERNALLY_FETCHED_PATHS,
    );
    let sites: Vec<(&str, u32)> = out.iter().map(|f| (f.file.as_str(), f.line)).collect();
    assert_eq!(sites, vec![("a.java", 2), ("a.java", 9), ("z.java", 1)]);
}

#[test]
fn near_miss_cross_reference_note_fires_when_the_provide_is_a_near_miss_target() {
    let mut targets = BTreeMap::new();
    targets.insert(
        ("be".to_string(), "Api.java".to_string(), 12),
        NearMissTargetRef {
            consume_file: "Api.tsx".to_string(),
            consume_line: 7,
            count: 3,
        },
    );
    let out = unconsumed_endpoint_findings(
        &[dead("GET /orphan", "be", "Api.java", 12)],
        &[],
        &targets,
        &no_trpc(),
        &none_reported(),
        EXTERNALLY_FETCHED_PATHS,
    );
    assert_eq!(out.len(), 1);
    assert!(out[0].message.contains("3 unmatched http consume(s)"));
    assert!(out[0]
        .message
        .contains("cross-layer/route-near-miss` finding at Api.tsx:7"));
    let data = out[0].data.as_ref().unwrap();
    assert_eq!(data["nearMissConsumeCount"], 3);
    assert_eq!(data["nearMissConsumeExample"], "Api.tsx:7");
}

#[test]
fn near_miss_cross_reference_note_is_absent_when_the_provide_is_not_a_near_miss_target() {
    let out = unconsumed_endpoint_findings(
        &[dead("GET /orphan", "be", "Api.java", 12)],
        &[],
        &no_near_miss(),
        &no_trpc(),
        &none_reported(),
        EXTERNALLY_FETCHED_PATHS,
    );
    assert_eq!(out.len(), 1);
    assert!(!out[0].message.contains("near-miss"));
    assert!(out[0]
        .data
        .as_ref()
        .unwrap()
        .get("nearMissConsumeCount")
        .is_none());
}

#[test]
fn trpc_mount_route_is_suppressed_when_its_own_tree_has_a_trpc_edge() {
    let out = unconsumed_endpoint_findings(
        &[dead(
            "GET /api/trpc/{}",
            "web",
            "pages/api/trpc/[trpc].ts",
            3,
        )],
        &[],
        &no_near_miss(),
        &trpc_sources(&["web"]),
        &none_reported(),
        EXTERNALLY_FETCHED_PATHS,
    );
    assert!(out.is_empty());
}

#[test]
fn trpc_mount_route_is_still_reported_when_no_tree_has_a_trpc_edge() {
    let out = unconsumed_endpoint_findings(
        &[dead(
            "GET /api/trpc/{}",
            "web",
            "pages/api/trpc/[trpc].ts",
            3,
        )],
        &[],
        &no_near_miss(),
        &no_trpc(),
        &none_reported(),
        EXTERNALLY_FETCHED_PATHS,
    );
    assert_eq!(out.len(), 1);
    assert!(out[0].message.contains("GET /api/trpc/{}"));
}

#[test]
fn trpc_mount_route_is_still_reported_when_only_a_different_tree_has_trpc_edges() {
    // Class A regression: a run-global `trpc_edge_count` gate would suppress tree "web"'s literal
    // trpc-segment route purely because tree "api" has trpc edges — the mount-IS-transport
    // justification only holds for the tree whose OWN edges flow through the route.
    let out = unconsumed_endpoint_findings(
        &[dead(
            "GET /api/trpc/{}",
            "web",
            "pages/api/trpc/[trpc].ts",
            3,
        )],
        &[],
        &no_near_miss(),
        &trpc_sources(&["api"]),
        &none_reported(),
        EXTERNALLY_FETCHED_PATHS,
    );
    assert_eq!(out.len(), 1);
    assert!(out[0].message.contains("GET /api/trpc/{}"));
}

#[test]
fn a_route_that_merely_contains_but_does_not_carry_a_trpc_segment_is_not_suppressed() {
    // "trpcish" is not the literal segment `trpc` — must not false-positive on substring match.
    let out = unconsumed_endpoint_findings(
        &[dead(
            "GET /api/trpcish/status",
            "web",
            "pages/api/trpcish/status.ts",
            3,
        )],
        &[],
        &no_near_miss(),
        &trpc_sources(&["web"]),
        &none_reported(),
        EXTERNALLY_FETCHED_PATHS,
    );
    assert_eq!(out.len(), 1);
}

// --- Externally-fetched path veto ---

#[test]
fn every_externally_fetched_path_token_is_vetoed() {
    // Each token of the policy vocabulary, pinned individually: "no consumer in this analysis" is not weak
    // evidence but NO evidence for a path whose requester (monitor, browser, crawler, feed reader) sits
    // outside every analyzable tree by construction.
    for path in [
        "/",
        "/health",
        "/healthz",
        "/healthcheck",
        "/livez",
        "/readyz",
        "/robots.txt",
        "/sitemap.xml",
        "/sitemap_index.xml",
        "/rss.xml",
        "/feed.xml",
        "/atom.xml",
        "/favicon.ico",
        "/.well-known/security.txt",
        "/.well-known/acme-challenge/{}",
    ] {
        let out = unconsumed_endpoint_findings(
            &[dead(&format!("GET {path}"), "web", "worker.ts", 4)],
            &[],
            &no_near_miss(),
            &no_trpc(),
            &none_reported(),
            EXTERNALLY_FETCHED_PATHS,
        );
        assert!(out.is_empty(), "expected `GET {path}` to be vetoed");
    }
}

#[test]
fn vetoed_paths_are_matched_case_insensitively_and_ignore_a_trailing_slash() {
    for key in ["GET /Health", "GET /health/", "HEAD /FAVICON.ICO"] {
        let out = unconsumed_endpoint_findings(
            &[dead(key, "web", "worker.ts", 4)],
            &[],
            &no_near_miss(),
            &no_trpc(),
            &none_reported(),
            EXTERNALLY_FETCHED_PATHS,
        );
        assert!(out.is_empty(), "expected `{key}` to be vetoed");
    }
}

#[test]
fn an_ordinary_route_that_merely_resembles_a_vetoed_path_still_fires() {
    // The veto is whole-path and exact — an application route that embeds or extends a vetoed token is
    // ordinary deployed surface and must keep reporting. `/feed`, `/rss`, `/metrics`, `/status` are
    // deliberately OUT of the vocabulary: each is just as plausibly an in-app route.
    for key in [
        "GET /orphan",
        "GET /api/health",
        "GET /health-report",
        "GET /healthy",
        "GET /feed",
        "GET /rss",
        "GET /metrics",
        "GET /status",
        "GET /well-known/thing",
    ] {
        let out = unconsumed_endpoint_findings(
            &[dead(key, "be", "Api.java", 12)],
            &[],
            &no_near_miss(),
            &no_trpc(),
            &none_reported(),
            EXTERNALLY_FETCHED_PATHS,
        );
        assert_eq!(out.len(), 1, "expected `{key}` to still fire");
        assert!(out[0].message.contains(key));
    }
}

#[test]
fn a_key_without_the_method_path_shape_is_never_vetoed() {
    // An unrecognized key shape is not evidence of anything — it must not silently buy silence.
    let out = unconsumed_endpoint_findings(
        &[dead("/health", "be", "Api.java", 12)],
        &[],
        &no_near_miss(),
        &no_trpc(),
        &none_reported(),
        EXTERNALLY_FETCHED_PATHS,
    );
    assert_eq!(out.len(), 1);
}

// --- Handoff to `cross-layer/unconsumed-mutation-endpoint` ---

#[test]
fn a_write_route_yields_exactly_one_finding_when_the_specialization_is_enabled() {
    // The measured defect: `POST /api/ledger/{}/verify` fired BOTH rules at the identical file:line.
    let (general, specialized) =
        with_mutation_rule_enabled(&[dead("POST /api/ledger/{}/verify", "be", "Api.java", 12)]);
    assert!(general.is_empty());
    assert_eq!(specialized.len(), 1);
    assert_eq!(
        specialized[0].rule_id,
        "cross-layer/unconsumed-mutation-endpoint"
    );
    assert_eq!(specialized[0].line, 12);
}

#[test]
fn every_write_verb_is_handed_to_the_specialization_when_it_is_enabled() {
    for key in [
        "POST /api/items",
        "PUT /api/users/{}",
        "PATCH /api/users/{}",
        "DELETE /api/users/{}",
    ] {
        let (general, specialized) = with_mutation_rule_enabled(&[dead(key, "be", "Api.java", 12)]);
        assert!(general.is_empty(), "expected `{key}` not to be repeated");
        assert_eq!(specialized.len(), 1, "expected `{key}` to be specialized");
    }
}

#[test]
fn a_write_route_is_still_reported_here_when_the_specialization_is_disabled() {
    // Regression guard against "simplifying" the handoff into an unconditional write-verb veto: with the
    // specialization disabled nothing reported that site, so silence here would be a coverage hole in a rule
    // the user did NOT disable. `none_reported()` is exactly what the engine passes in that case.
    let out = unconsumed_endpoint_findings(
        &[dead("POST /api/ledger/{}/verify", "be", "Api.java", 12)],
        &[],
        &no_near_miss(),
        &no_trpc(),
        &none_reported(),
        EXTERNALLY_FETCHED_PATHS,
    );
    assert_eq!(out.len(), 1);
    assert!(out[0].message.contains("POST /api/ledger/{}/verify"));
}

#[test]
fn suppression_is_anchored_per_route_not_per_rule_run() {
    // A reported write route silences only ITSELF: a different route in the same file still reports, and so
    // does the same route at the same file:line under a different source tree.
    let provides = [
        dead("POST /api/items", "be", "Api.java", 12),
        dead("GET /api/orphan", "be", "Api.java", 40),
        dead("POST /api/items", "other", "Api.java", 12),
    ];
    let specialized = unconsumed_mutation_endpoint_findings(
        &provides[..1],
        &[],
        &BTreeSet::new(),
        &no_near_miss(),
        &no_trpc(),
    );
    let general = unconsumed_endpoint_findings(
        &provides,
        &[],
        &no_near_miss(),
        &no_trpc(),
        &reported_provide_sites(&specialized),
        EXTERNALLY_FETCHED_PATHS,
    );
    let sites: Vec<(&str, u32)> = general.iter().map(|f| (f.file.as_str(), f.line)).collect();
    assert_eq!(sites, vec![("Api.java", 12), ("Api.java", 40)]);
    assert!(general[0].message.contains("source `other`"));
}

#[test]
fn a_co_located_read_route_survives_the_stand_down_of_its_write_siblings() {
    // A verb-agnostic registration (gin `router.Any`, axum `any(...)`, the TS pathname-dispatch adapter)
    // emits one provide PER method at ONE `file:line`. Suppression keyed on the anchor alone would let the
    // reported write verbs silence the co-located `GET /webhook` in BOTH rules — a silent hole. The
    // suppression key must identify the route, not the line.
    let provides = [
        dead("GET /webhook", "be", "handlers.go", 30),
        dead("POST /webhook", "be", "handlers.go", 30),
        dead("PUT /webhook", "be", "handlers.go", 30),
        dead("PATCH /webhook", "be", "handlers.go", 30),
        dead("DELETE /webhook", "be", "handlers.go", 30),
    ];
    let (general, specialized) = with_mutation_rule_enabled(&provides);
    assert_eq!(specialized.len(), 4, "{:?}", specialized);
    assert_eq!(general.len(), 1, "{:?}", general);
    assert!(general[0].message.contains("GET /webhook"));
    assert_eq!(general[0].line, 30);
}

#[test]
fn a_read_route_is_reported_by_this_rule_and_by_it_alone() {
    // The other half of the partition: neither rule may go silent on a non-write route.
    let (general, specialized) =
        with_mutation_rule_enabled(&[dead("GET /api/orphan", "be", "Api.java", 12)]);
    assert_eq!(general.len(), 1);
    assert_eq!(general[0].rule_id, "cross-layer/unconsumed-endpoint");
    assert!(specialized.is_empty());
    assert!(general[0]
        .message
        .contains("cross-layer/unconsumed-mutation-endpoint"));
}

// --- per-source volume fold (`MAX_LISTED_PER_SOURCE`) ---

/// `n` dead GET routes in one source, one per file so the anchors are distinct and sortable.
fn many_dead(n: u32, source: &str) -> Vec<TaggedProvide> {
    (0..n)
        .map(|i| {
            dead(
                &format!("GET /api/r{i:03}"),
                source,
                &format!("r{i:03}.ts"),
                1,
            )
        })
        .collect()
}

#[test]
fn a_source_at_the_cap_is_listed_in_full_with_no_fold() {
    let provides = many_dead(MAX_LISTED_PER_SOURCE as u32, "be");
    let f = unconsumed_endpoint_findings(
        &provides,
        &[],
        &no_near_miss(),
        &no_trpc(),
        &none_reported(),
        EXTERNALLY_FETCHED_PATHS,
    );
    assert_eq!(f.len(), MAX_LISTED_PER_SOURCE, "{f:?}");
    assert!(
        f.iter()
            .all(|x| x.data.as_ref().unwrap()["key"] != serde_json::Value::Null),
        "no fold finding may appear at or below the cap"
    );
}

#[test]
fn a_public_api_tree_folds_the_tail_into_one_disclosed_finding() {
    let provides = many_dead(503, "medusa");
    let f = unconsumed_endpoint_findings(
        &provides,
        &[],
        &no_near_miss(),
        &no_trpc(),
        &none_reported(),
        EXTERNALLY_FETCHED_PATHS,
    );
    assert_eq!(
        f.len(),
        MAX_LISTED_PER_SOURCE + 1,
        "the tail collapses to exactly one finding: {}",
        f.len()
    );
    let fold = f
        .iter()
        .find(|x| {
            x.data
                .as_ref()
                .unwrap()
                .get("foldedEndpointCount")
                .is_some()
        })
        .expect("a fold finding must exist");
    assert_eq!(fold.rule_id, "cross-layer/unconsumed-endpoint");
    assert_eq!(
        fold.data.as_ref().unwrap()["foldedEndpointCount"],
        503 - MAX_LISTED_PER_SOURCE
    );
    assert!(
        fold.message.contains("crossLayer.unconsumedProvides"),
        "the fold must say where the uncapped list is: {}",
        fold.message
    );
}

#[test]
fn the_fold_is_per_source_so_a_small_tree_beside_a_huge_one_is_untouched() {
    let mut provides = many_dead(503, "medusa");
    provides.extend(many_dead(2, "web"));
    let f = unconsumed_endpoint_findings(
        &provides,
        &[],
        &no_near_miss(),
        &no_trpc(),
        &none_reported(),
        EXTERNALLY_FETCHED_PATHS,
    );
    let web: Vec<_> = f
        .iter()
        .filter(|x| x.data.as_ref().unwrap()["source"] == "web")
        .collect();
    assert_eq!(web.len(), 2, "{web:?}");
    assert!(web.iter().all(|x| x
        .data
        .as_ref()
        .unwrap()
        .get("foldedEndpointCount")
        .is_none()));
}
