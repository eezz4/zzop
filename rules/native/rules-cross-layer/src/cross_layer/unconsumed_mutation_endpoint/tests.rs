use super::*;
use zzop_core::io::IoProvide;

fn unconsumed_provide(
    kind: &str,
    key: &str,
    source: &str,
    file: &str,
    line: u32,
    symbol: Option<&str>,
) -> TaggedProvide {
    TaggedProvide {
        source: source.to_string(),
        provide: IoProvide {
            body: None,
            kind: kind.to_string(),
            key: key.to_string(),
            file: file.to_string(),
            line,
            symbol: symbol.map(str::to_string),
        },
    }
}

fn unresolved_http(source: &str) -> TaggedConsume {
    TaggedConsume {
        source: source.to_string(),
        consume: zzop_core::IoConsume {
            client: None,
            body: None,
            kind: "http".to_string(),
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

fn no_blind() -> BTreeSet<String> {
    BTreeSet::new()
}

fn blind(sources: &[&str]) -> BTreeSet<String> {
    sources.iter().map(|s| s.to_string()).collect()
}

#[test]
fn dead_write_endpoint_is_flagged_with_method_and_source() {
    let out = unconsumed_mutation_endpoint_findings(
        &[unconsumed_provide(
            "http",
            "DELETE /api/users/{}",
            "be",
            "Api.java",
            12,
            Some("deleteUser"),
        )],
        &[],
        &no_blind(),
        &no_near_miss(),
        &no_trpc(),
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].rule_id, "cross-layer/unconsumed-mutation-endpoint");
    assert_eq!(out[0].severity, Severity::Warning);
    assert_eq!(out[0].file, "Api.java");
    assert_eq!(out[0].line, 12);
    assert!(out[0].message.contains("DELETE /api/users/{}"));
    assert!(out[0].message.contains("standing attack surface"));
    assert!(out[0].message.contains("cross-layer/unconsumed-endpoint"));
    assert!(out[0].message.contains("disabled_rules"));
    assert!(!out[0].message.contains("near-miss"));
    let data = out[0].data.as_ref().unwrap();
    assert_eq!(data["method"], "DELETE");
    assert_eq!(data["symbol"], "deleteUser");
    assert_eq!(data["unresolvedHttpConsumeCount"], 0);
}

#[test]
fn read_method_dead_endpoint_is_not_this_rules_turf() {
    let out = unconsumed_mutation_endpoint_findings(
        &[unconsumed_provide(
            "http",
            "GET /api/users",
            "be",
            "Api.java",
            12,
            None,
        )],
        &[],
        &no_blind(),
        &no_near_miss(),
        &no_trpc(),
    );
    assert!(out.is_empty());
}

#[test]
fn dead_provide_registered_in_a_test_fixture_file_is_skipped() {
    let out = unconsumed_mutation_endpoint_findings(
        &[unconsumed_provide(
            "http",
            "POST /api/users",
            "be",
            "src/api/__test__/handlers.test.ts",
            5,
            None,
        )],
        &[],
        &no_blind(),
        &no_near_miss(),
        &no_trpc(),
    );
    assert!(out.is_empty());
}

#[test]
fn non_http_dead_provide_is_ignored() {
    let out = unconsumed_mutation_endpoint_findings(
        &[unconsumed_provide(
            "db-table",
            "table:users",
            "db",
            "schema.sql",
            1,
            None,
        )],
        &[],
        &no_blind(),
        &no_near_miss(),
        &no_trpc(),
    );
    assert!(out.is_empty());
}

#[test]
fn determinism_multiple_findings_sorted_by_file_then_line() {
    let out = unconsumed_mutation_endpoint_findings(
        &[
            unconsumed_provide("http", "POST /b", "be", "z.java", 1, None),
            unconsumed_provide("http", "PUT /a", "be", "a.java", 9, None),
            unconsumed_provide("http", "PATCH /c", "be", "a.java", 2, None),
        ],
        &[],
        &no_blind(),
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
            count: 2,
        },
    );
    let out = unconsumed_mutation_endpoint_findings(
        &[unconsumed_provide(
            "http",
            "DELETE /api/users/{}",
            "be",
            "Api.java",
            12,
            Some("deleteUser"),
        )],
        &[],
        &no_blind(),
        &targets,
        &no_trpc(),
    );
    assert_eq!(out.len(), 1);
    assert!(out[0].message.contains("2 unmatched http consume(s)"));
    assert!(out[0]
        .message
        .contains("cross-layer/route-near-miss` finding at Api.tsx:7"));
    let data = out[0].data.as_ref().unwrap();
    assert_eq!(data["nearMissConsumeCount"], 2);
    assert_eq!(data["nearMissConsumeExample"], "Api.tsx:7");
}

#[test]
fn near_miss_cross_reference_note_is_absent_when_the_provide_is_not_a_near_miss_target() {
    let out = unconsumed_mutation_endpoint_findings(
        &[unconsumed_provide(
            "http",
            "DELETE /api/users/{}",
            "be",
            "Api.java",
            12,
            Some("deleteUser"),
        )],
        &[],
        &no_blind(),
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
fn trpc_mount_route_write_verb_is_suppressed_when_its_own_tree_has_a_trpc_edge() {
    // `file_routes`'s pages/api fallback-verb convention emits POST too for a default-export
    // handler — the write-verb rule must suppress the tRPC mount site exactly like its sibling.
    let out = unconsumed_mutation_endpoint_findings(
        &[unconsumed_provide(
            "http",
            "POST /api/trpc/{}",
            "web",
            "pages/api/trpc/[trpc].ts",
            3,
            Some("default"),
        )],
        &[],
        &no_blind(),
        &no_near_miss(),
        &trpc_sources(&["web"]),
    );
    assert!(out.is_empty());
}

#[test]
fn trpc_mount_route_write_verb_is_still_reported_when_no_tree_has_a_trpc_edge() {
    let out = unconsumed_mutation_endpoint_findings(
        &[unconsumed_provide(
            "http",
            "POST /api/trpc/{}",
            "web",
            "pages/api/trpc/[trpc].ts",
            3,
            Some("default"),
        )],
        &[],
        &no_blind(),
        &no_near_miss(),
        &no_trpc(),
    );
    assert_eq!(out.len(), 1);
}

#[test]
fn trpc_mount_route_write_verb_is_still_reported_when_only_a_different_tree_has_trpc_edges() {
    // Class A regression: mirrors `unconsumed_endpoint`'s equivalent test — a run-global edge count
    // would wrongly suppress tree "web"'s own write-verb mount route based on tree "api"'s edges.
    let out = unconsumed_mutation_endpoint_findings(
        &[unconsumed_provide(
            "http",
            "POST /api/trpc/{}",
            "web",
            "pages/api/trpc/[trpc].ts",
            3,
            Some("default"),
        )],
        &[],
        &no_blind(),
        &no_near_miss(),
        &trpc_sources(&["api"]),
    );
    assert_eq!(out.len(), 1);
}

// --- Severity calibration (mono-hub field review) ---

#[test]
fn message_states_the_unresolved_http_count_honestly() {
    let out = unconsumed_mutation_endpoint_findings(
        &[unconsumed_provide(
            "http",
            "DELETE /api/users/{}",
            "be",
            "Api.java",
            12,
            None,
        )],
        &[
            unresolved_http("fe"),
            unresolved_http("fe"),
            unresolved_http("fe"),
        ],
        &no_blind(),
        &no_near_miss(),
        &no_trpc(),
    );
    assert_eq!(out.len(), 1);
    assert!(out[0].message.contains("3 unresolved"));
    let data = out[0].data.as_ref().unwrap();
    assert_eq!(data["unresolvedHttpConsumeCount"], 3);
}

#[test]
fn a_blind_source_downgrades_severity_to_info_and_names_the_source() {
    let out = unconsumed_mutation_endpoint_findings(
        &[unconsumed_provide(
            "http",
            "POST /api/group",
            "be",
            "Api.java",
            12,
            None,
        )],
        &[
            unresolved_http("fe"),
            unresolved_http("fe"),
            unresolved_http("fe"),
        ],
        &blind(&["fe"]),
        &no_near_miss(),
        &no_trpc(),
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].severity, Severity::Info);
    assert!(out[0].message.contains("`fe`"), "{}", out[0].message);
    assert!(
        out[0].message.contains("3 unresolved"),
        "{}",
        out[0].message
    );
    assert!(
        out[0].message.contains("severity here is reduced to"),
        "{}",
        out[0].message
    );
    // Still attack-surface-framed — the downgrade lowers confidence, not the underlying claim.
    assert!(out[0].message.contains("standing attack surface"));
}

#[test]
fn no_blind_source_keeps_warning_and_todays_attack_surface_framing() {
    let out = unconsumed_mutation_endpoint_findings(
        &[unconsumed_provide(
            "http",
            "POST /api/group",
            "be",
            "Api.java",
            12,
            None,
        )],
        &[],
        &no_blind(),
        &no_near_miss(),
        &no_trpc(),
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].severity, Severity::Warning);
    assert!(out[0].message.contains("standing attack surface"));
    assert!(!out[0].message.contains("severity here is reduced"));
}

#[test]
fn blind_source_list_is_capped_at_three_with_a_remainder_count() {
    let out = unconsumed_mutation_endpoint_findings(
        &[unconsumed_provide(
            "http",
            "POST /api/group",
            "be",
            "Api.java",
            12,
            None,
        )],
        &[],
        &blind(&["a", "b", "c", "d", "e"]),
        &no_near_miss(),
        &no_trpc(),
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].severity, Severity::Info);
    assert!(out[0].message.contains("`a`"));
    assert!(out[0].message.contains("`b`"));
    assert!(out[0].message.contains("`c`"));
    assert!(!out[0].message.contains("`d`"));
    assert!(out[0].message.contains("and 2 more"), "{}", out[0].message);
}
