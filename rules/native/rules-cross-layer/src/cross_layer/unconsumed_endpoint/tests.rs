use super::*;

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

#[test]
fn dead_http_provide_is_flagged_with_source_and_anchor() {
    let out = unconsumed_endpoint_findings(
        &[dead("GET /orphan", "be", "Api.java", 12)],
        &[],
        &no_near_miss(),
        &no_trpc(),
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
    );
    assert!(out.is_empty());
}

#[test]
fn no_unconsumed_provides_is_empty() {
    assert!(unconsumed_endpoint_findings(&[], &[], &no_near_miss(), &no_trpc()).is_empty());
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
    );
    assert_eq!(out.len(), 1);
}
