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
            response: None,
            body: None,
            kind: kind.to_string(),
            key: key.to_string(),
            file: file.to_string(),
            line,
            symbol: symbol.map(str::to_string),
            ..Default::default()
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

fn no_blind() -> BTreeSet<String> {
    BTreeSet::new()
}

fn blind(sources: &[&str]) -> BTreeSet<String> {
    sources.iter().map(|s| s.to_string()).collect()
}

fn near_miss_at(
    source: &str,
    file: &str,
    line: u32,
) -> BTreeMap<(String, String, u32), NearMissTargetRef> {
    let mut m = BTreeMap::new();
    m.insert(
        (source.to_string(), file.to_string(), line),
        NearMissTargetRef {
            consume_file: "fe/api.ts".to_string(),
            consume_line: 9,
            count: 1,
        },
    );
    m
}

#[test]
fn a_provide_that_is_a_known_near_miss_target_is_not_a_confident_zero() {
    // The inversion this closes: a caller was FOUND — off by a base prefix, named in this very
    // finding's cross-reference note — and the endpoint was still reported at Warning as if nobody
    // called it. Meanwhile `cross-layer/prefix-drift`, which states the actual cause, is info. The
    // louder message was the wrong one.
    //
    // This rule's own doctrine already says a zero is only confident when the consume key space was
    // resolved. The run-level `blind_sources` predicate cannot witness THIS case — those consumes are
    // resolved, they just land one prefix over — so the near-miss evidence, which is per-provide and
    // already in hand, decides this finding's severity.
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
        &BTreeSet::new(),
        &BTreeSet::new(),
        &near_miss_at("be", "Api.java", 12),
        &no_trpc(),
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].severity, Severity::Info, "{:?}", out[0]);
    // The de-escalation must SAY why, the same discipline the blind-source branch follows.
    assert!(out[0].message.contains("near-miss"), "{}", out[0].message);
}

#[test]
fn a_provide_with_no_near_miss_keeps_warning() {
    // Guard for the above: the de-escalation is evidence-driven, not a blanket softening.
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
        &BTreeSet::new(),
        &BTreeSet::new(),
        &no_near_miss(),
        &no_trpc(),
    );
    assert_eq!(out[0].severity, Severity::Warning);
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
        &BTreeSet::new(),
        &BTreeSet::new(),
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
    assert!(out[0].message.contains("disabledRules"));
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
        &BTreeSet::new(),
        &BTreeSet::new(),
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
        &BTreeSet::new(),
        &BTreeSet::new(),
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
        &BTreeSet::new(),
        &BTreeSet::new(),
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
        &BTreeSet::new(),
        &BTreeSet::new(),
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
        &BTreeSet::new(),
        &BTreeSet::new(),
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
        &BTreeSet::new(),
        &BTreeSet::new(),
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
        &BTreeSet::new(),
        &BTreeSet::new(),
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
        &BTreeSet::new(),
        &BTreeSet::new(),
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
        &BTreeSet::new(),
        &BTreeSet::new(),
        &no_near_miss(),
        &trpc_sources(&["api"]),
    );
    assert_eq!(out.len(), 1);
}

// --- Severity calibration ---

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
        &BTreeSet::new(),
        &BTreeSet::new(),
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
        &BTreeSet::new(),
        &BTreeSet::new(),
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
        &BTreeSet::new(),
        &BTreeSet::new(),
        &no_near_miss(),
        &no_trpc(),
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].severity, Severity::Warning);
    assert!(out[0].message.contains("standing attack surface"));
    assert!(!out[0].message.contains("severity here is reduced"));
    // Seals the severity-gate FRAMING (see the module doc's "The NO-downgrade branch speaks too"): the
    // no-blind branch must never go back to an empty note, which would leave warning severity readable as
    // a proof that the caller set was complete. Both halves are pinned — the narrow check that did not
    // fire is NAMED, and the non-claim it licenses is stated.
    assert!(
        out[0].message.contains("majority-unresolved"),
        "the warning branch must name the narrow check that did not fire: {}",
        out[0].message
    );
    assert!(
        out[0]
            .message
            .contains("not that the caller set was proven complete"),
        "the warning branch must deny the completeness reading: {}",
        out[0].message
    );
}

#[test]
fn the_downgrade_note_leads_the_message_instead_of_trailing_it() {
    // A field reviewer read the Warning/Info difference between two run modes as a bug because the sentence
    // explaining it sat at the end of a long message. Pinned by position, not just presence.
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
        &blind(&["fe"]),
        &BTreeSet::new(),
        &BTreeSet::new(),
        &no_near_miss(),
        &no_trpc(),
    );
    let message = &out[0].message;
    let note = message.find("consume side is partly blind").unwrap();
    let risk = message.find("standing attack surface").unwrap();
    assert!(note < risk, "{message}");
}

// --- Site handoff to `cross-layer/unconsumed-endpoint` ---

#[test]
fn reported_provide_sites_reads_back_the_anchors_of_this_rules_own_findings() {
    let out = unconsumed_mutation_endpoint_findings(
        &[
            unconsumed_provide("http", "POST /a", "be", "Api.java", 12, None),
            unconsumed_provide("http", "DELETE /b", "web", "route.ts", 3, None),
            unconsumed_provide("http", "GET /c", "be", "Api.java", 40, None),
        ],
        &[],
        &no_blind(),
        &BTreeSet::new(),
        &BTreeSet::new(),
        &no_near_miss(),
        &no_trpc(),
    );
    let sites = reported_provide_sites(&out);
    assert_eq!(
        sites,
        [
            (
                "be".to_string(),
                "Api.java".to_string(),
                12,
                "POST /a".to_string()
            ),
            (
                "web".to_string(),
                "route.ts".to_string(),
                3,
                "DELETE /b".to_string()
            ),
        ]
        .into_iter()
        .collect::<BTreeSet<_>>()
    );
}

#[test]
fn reported_provide_sites_distinguishes_routes_sharing_one_anchor() {
    // A verb-agnostic registration puts every method on ONE `file:line`. The handoff key must separate them,
    // or `unconsumed-endpoint` would stand down on the whole line instead of on the reported routes.
    let out = unconsumed_mutation_endpoint_findings(
        &[
            unconsumed_provide("http", "POST /webhook", "be", "handlers.go", 30, None),
            unconsumed_provide("http", "DELETE /webhook", "be", "handlers.go", 30, None),
            unconsumed_provide("http", "GET /webhook", "be", "handlers.go", 30, None),
        ],
        &[],
        &no_blind(),
        &BTreeSet::new(),
        &BTreeSet::new(),
        &no_near_miss(),
        &no_trpc(),
    );
    let sites = reported_provide_sites(&out);
    assert_eq!(sites.len(), 2);
    assert!(sites.contains(&(
        "be".to_string(),
        "handlers.go".to_string(),
        30,
        "POST /webhook".to_string()
    )));
    assert!(!sites.iter().any(|(_, _, _, key)| key == "GET /webhook"));
}

#[test]
fn reported_provide_sites_ignores_findings_from_other_rules() {
    // The engine hands over a slice this rule produced, but the reader must stay id-anchored so a future
    // caller cannot accidentally silence `unconsumed-endpoint` with some other rule's anchors.
    let foreign = Finding {
        rule_id: "cross-layer/unconsumed-endpoint".to_string(),
        severity: Severity::Info,
        file: "Api.java".to_string(),
        line: 12,
        message: String::new(),
        evidence_paths: Vec::new(),
        data: Some(serde_json::json!({ "source": "be", "key": "POST /a" })),
    };
    assert!(reported_provide_sites(&[foreign]).is_empty());
}

#[test]
fn the_message_discloses_that_the_general_rule_stands_down_here() {
    let out = unconsumed_mutation_endpoint_findings(
        &[unconsumed_provide(
            "http",
            "POST /api/ledger/{}/verify",
            "be",
            "Api.java",
            12,
            None,
        )],
        &[],
        &no_blind(),
        &BTreeSet::new(),
        &BTreeSet::new(),
        &no_near_miss(),
        &no_trpc(),
    );
    assert!(
        out[0]
            .message
            .contains("stays silent on this route by design"),
        "{}",
        out[0].message
    );
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
        &BTreeSet::new(),
        &BTreeSet::new(),
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

#[test]
fn a_tree_that_contributed_nothing_downgrades_the_band_and_the_sentence_names_that_mechanism() {
    // 🔴 Review ledger V63. The band used to move OPPOSITE to the evidence here. `blind_sources` is a
    // RATIO over a floor (`MIN_TOTAL_CONSUMES`), so a caller tree with 5 consumes of which 3 were
    // unresolved counted as blind and dropped this to info — while a caller tree that contributed
    // NOTHING fell below the floor, counted as NOT blind, and left the band at warning. Measured on
    // `corpus/oss/fe-svelte` + `be-gin`: 16 write endpoints at warning while the caller's `.svelte`
    // files were never parsed by anything.
    let silent: BTreeSet<String> = ["fe-svelte".to_string()].into_iter().collect();
    let out = unconsumed_mutation_endpoint_findings(
        &[unconsumed_provide(
            "http",
            "POST /api/articles",
            "be",
            "routers.go",
            15,
            Some("ArticleCreate"),
        )],
        &[],
        &no_blind(),
        &silent,
        &BTreeSet::new(),
        &no_near_miss(),
        &no_trpc(),
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].severity, Severity::Info, "{:?}", out[0]);
    // The sentence must name THIS mechanism. Saying "majority-unresolved consumes" about a tree with no
    // consumes at all would be a false claim, and sending the reader looking for unresolved URLs that do
    // not exist is worse than saying nothing.
    assert!(
        out[0].message.contains("contributed NO joinable io at all"),
        "{}",
        out[0].message
    );
    assert!(
        !out[0].message.contains("majority-unresolved"),
        "the silent branch must not borrow the ratio branch's sentence: {}",
        out[0].message
    );
}

#[test]
fn the_ratio_branch_keeps_its_own_sentence_when_only_it_fires() {
    // Guard for the above in the other direction: the two mechanisms are separate inputs and each names
    // itself. A run where only the ratio predicate fired must not start claiming trees contributed
    // nothing — they contributed call sites, just unresolvable ones.
    let ratio: BTreeSet<String> = ["fe".to_string()].into_iter().collect();
    let out = unconsumed_mutation_endpoint_findings(
        &[unconsumed_provide(
            "http",
            "POST /api/articles",
            "be",
            "routers.go",
            15,
            Some("ArticleCreate"),
        )],
        &[],
        &ratio,
        &BTreeSet::new(),
        &BTreeSet::new(),
        &no_near_miss(),
        &no_trpc(),
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].severity, Severity::Info, "{:?}", out[0]);
    assert!(
        out[0].message.contains("majority-unresolved"),
        "{}",
        out[0].message
    );
    assert!(
        !out[0].message.contains("contributed NO joinable io"),
        "{}",
        out[0].message
    );
}

#[test]
fn zero_contribution_alone_does_not_move_the_band() {
    // The half of the gate that keeps this from silencing real attack surface. A shared-lib or UI-only
    // package in a monorepo join legitimately has no io; the ENGINE only puts a source in the silent set
    // when it ALSO measured that most of that tree's own files went unread by any parser. With no such
    // measurement the set is empty here, and a confident zero stays a warning.
    let out = unconsumed_mutation_endpoint_findings(
        &[unconsumed_provide(
            "http",
            "POST /api/articles",
            "be",
            "routers.go",
            15,
            Some("ArticleCreate"),
        )],
        &[],
        &no_blind(),
        &BTreeSet::new(),
        &BTreeSet::new(),
        &no_near_miss(),
        &no_trpc(),
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].severity, Severity::Warning, "{:?}", out[0]);
}

#[test]
fn an_untraced_client_witness_downgrades_the_band_and_the_sentence_names_that_mechanism() {
    // 🔴 Review ledger V82, and the third instance of V63's shape. The band read two witnesses while a
    // THIRD was being computed a few lines away and never reached it — so a reply could carry twelve
    // `warning` write endpoints each saying "no blindness was WITNESSED", beside
    // `untraced-client-import-no-visible-consume` saying the join is blind for exactly the tree that
    // would have been their caller. Measured on `corpus/oss/pair-redux-fastapi.jsonc`, where
    // `fe-redux` routes every call through `superagent`: 12 warning + 8 info became 20 info.
    let untraced: BTreeSet<String> = ["fe-redux".to_string()].into_iter().collect();
    let out = unconsumed_mutation_endpoint_findings(
        &[unconsumed_provide(
            "http",
            "POST /api/articles",
            "be",
            "routers.py",
            15,
            Some("create_article"),
        )],
        &[],
        &no_blind(),
        &BTreeSet::new(),
        &untraced,
        &no_near_miss(),
        &no_trpc(),
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].severity, Severity::Info, "{:?}", out[0]);
    assert!(
        out[0]
            .message
            .contains("client/SDK package this build cannot read"),
        "{}",
        out[0].message
    );
    // Each witness names itself. Borrowing another's sentence sends the reader after evidence that is
    // not there — an unresolved URL to inspect, or a tree that contributed nothing.
    assert!(
        !out[0].message.contains("majority-unresolved"),
        "{}",
        out[0].message
    );
    assert!(
        !out[0].message.contains("contributed NO joinable io"),
        "{}",
        out[0].message
    );
}

#[test]
fn the_warning_branch_admits_all_three_checks_stayed_silent() {
    // The empty branch's sentence is the one that has to move whenever a witness is added, and it did
    // not when the second arrived: it still said the check "only asks whether a source's http consumes
    // are majority-unresolved" after there were two. A band that explains itself with a stale inventory
    // of its own inputs is the class-extrapolation this branch exists to prevent.
    let out = unconsumed_mutation_endpoint_findings(
        &[unconsumed_provide(
            "http",
            "POST /api/articles",
            "be",
            "routers.py",
            15,
            Some("create_article"),
        )],
        &[],
        &no_blind(),
        &BTreeSet::new(),
        &BTreeSet::new(),
        &no_near_miss(),
        &no_trpc(),
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].severity, Severity::Warning, "{:?}", out[0]);
    assert!(
        out[0]
            .message
            .contains("three consume-side blindness checks"),
        "the silent branch must name how many checks it is speaking for: {}",
        out[0].message
    );
}
