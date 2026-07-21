//! Unit tests for `unprovided_consume_findings`'s join + veto logic in isolation (e2e coverage —
//! real FE/BE fixtures — lives in `crates/engine/tests/analyze_io_natives.rs`).
use super::*;

fn provide(key: &str, file: &str, line: u32) -> zzop_core::IoProvide {
    zzop_core::IoProvide {
        body: None,
        kind: "http".to_string(),
        key: key.to_string(),
        file: file.to_string(),
        line,
        symbol: None,
    }
}

fn consume(kind: &str, key: Option<&str>, file: &str, line: u32) -> zzop_core::IoConsume {
    zzop_core::IoConsume {
        client: None,
        body: None,
        kind: kind.to_string(),
        key: key.map(str::to_string),
        file: file.to_string(),
        line,
        raw: None,
        method: None,
    }
}

#[test]
fn unmatched_consume_is_flagged_when_the_tree_has_a_provide() {
    let provides = vec![provide("GET /a", "api.ts", 1)];
    let consumes = vec![consume("http", Some("GET /missing"), "client.ts", 3)];
    let found = unprovided_consume_findings(&provides, &consumes);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].file, "client.ts");
    assert_eq!(found[0].line, 3);
    assert_eq!(found[0].rule_id, "unprovided-consume");
    assert_eq!(found[0].severity, zzop_core::Severity::Info);
    assert!(found[0].message.contains("GET /missing"));
}

#[test]
fn a_consume_in_a_test_file_is_never_flagged() {
    // A `test_*.py`/`*.spec.ts` call to a missing route (a deliberate 404 probe, or an httpx/requests
    // client fixture) is test scaffolding, not deployed egress — it must not be judged against the app's
    // routes, mirroring the cross-tree join's own test-classified io drop (filter_join_io, D11).
    let provides = vec![provide("GET /a", "api.ts", 1)];
    let consumes = vec![
        consume(
            "http",
            Some("GET /wrong_path/asd"),
            "tests/test_errors.py",
            15,
        ),
        consume("http", Some("GET /also-missing"), "src/client.spec.ts", 9),
    ];
    let found = unprovided_consume_findings(&provides, &consumes);
    assert!(
        found.is_empty(),
        "test-file consumes must not be flagged: {found:?}"
    );
}

#[test]
fn a_non_test_consume_alongside_a_test_consume_is_still_flagged() {
    // The test-file skip must not suppress a real app-code consume that happens to share the batch.
    let provides = vec![provide("GET /a", "api.ts", 1)];
    let consumes = vec![
        consume(
            "http",
            Some("GET /wrong_path/asd"),
            "tests/test_errors.py",
            15,
        ),
        consume("http", Some("GET /missing"), "src/client.ts", 3),
    ];
    let found = unprovided_consume_findings(&provides, &consumes);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].file, "src/client.ts");
}

#[test]
fn always_veto_static_asset_extension_consume_is_never_flagged() {
    let provides = vec![provide("GET /a", "api.ts", 1)];
    let consumes = vec![consume(
        "http",
        Some("GET /assets/icon.svg"),
        "client.ts",
        3,
    )];
    assert!(unprovided_consume_findings(&provides, &consumes).is_empty());
}

#[test]
fn always_veto_extension_followed_by_a_query_string_is_still_vetoed() {
    let provides = vec![provide("GET /a", "api.ts", 1)];
    let consumes = vec![consume(
        "http",
        Some("GET /assets/icon.svg?v=2"),
        "client.ts",
        3,
    )];
    assert!(unprovided_consume_findings(&provides, &consumes).is_empty());
}

#[test]
fn json_in_a_public_asset_directory_is_vetoed() {
    // /public/recipes.json — no API-ish segment anywhere in the path, so it's vetoed by default.
    let provides = vec![provide("GET /a", "api.ts", 1)];
    let consumes = vec![consume(
        "http",
        Some("GET /public/recipes.json"),
        "client.ts",
        3,
    )];
    assert!(unprovided_consume_findings(&provides, &consumes).is_empty());
}

#[test]
fn xml_in_a_static_asset_directory_is_vetoed() {
    let provides = vec![provide("GET /a", "api.ts", 1)];
    let consumes = vec![consume(
        "http",
        Some("GET /static/sitemap.xml"),
        "client.ts",
        3,
    )];
    assert!(unprovided_consume_findings(&provides, &consumes).is_empty());
}

#[test]
fn next_js_public_prefix_stripped_json_path_is_vetoed() {
    // Some frameworks serve public/ files with the `public/` prefix stripped from the URL — no
    // asset-directory segment survives in the key, but the API-segment gate still catches this.
    let provides = vec![provide("GET /a", "api.ts", 1)];
    let consumes = vec![consume("http", Some("GET /i18n/ko.json"), "client.ts", 3)];
    assert!(unprovided_consume_findings(&provides, &consumes).is_empty());
}

#[test]
fn rails_style_json_api_route_with_an_api_segment_still_fires() {
    // GET /api/users.json — Rails-style format-suffixed API route, real API consumption; the /api/
    // segment stops the default json/xml veto from applying.
    let provides = vec![provide("GET /a", "api.ts", 1)];
    let consumes = vec![consume("http", Some("GET /api/users.json"), "client.ts", 3)];
    let found = unprovided_consume_findings(&provides, &consumes);
    assert_eq!(found.len(), 1, "{:?}", found);
    assert_eq!(found[0].severity, zzop_core::Severity::Info);
    assert!(found[0].message.contains("GET /api/users.json"));
}

#[test]
fn xml_with_an_api_segment_still_fires() {
    let provides = vec![provide("GET /a", "api.ts", 1)];
    let consumes = vec![consume("http", Some("GET /api/feed.xml"), "client.ts", 3)];
    let found = unprovided_consume_findings(&provides, &consumes);
    assert_eq!(found.len(), 1, "{:?}", found);
}

#[test]
fn versioned_api_segment_json_route_still_fires() {
    let provides = vec![provide("GET /a", "api.ts", 1)];
    let consumes = vec![consume("http", Some("GET /v1/users.json"), "client.ts", 3)];
    let found = unprovided_consume_findings(&provides, &consumes);
    assert_eq!(found.len(), 1, "{:?}", found);
}

#[test]
fn graphql_segment_json_route_still_fires() {
    let provides = vec![provide("GET /a", "api.ts", 1)];
    let consumes = vec![consume(
        "http",
        Some("GET /graphql/schema.json"),
        "client.ts",
        3,
    )];
    let found = unprovided_consume_findings(&provides, &consumes);
    assert_eq!(found.len(), 1, "{:?}", found);
}

#[test]
fn json_path_with_no_api_segment_is_vetoed_regardless_of_directory_name() {
    // "/database/export.json" — not under a conventional asset directory either, but the inverted
    // gate vetoes it by default anyway since no /api/,/graphql/,/rpc/,/vN/ segment is present.
    let provides = vec![provide("GET /a", "api.ts", 1)];
    let consumes = vec![consume(
        "http",
        Some("GET /database/export.json"),
        "client.ts",
        3,
    )];
    assert!(unprovided_consume_findings(&provides, &consumes).is_empty());
}

#[test]
fn api_segment_match_requires_a_whole_path_segment_not_a_substring() {
    // "/apiary/" contains "api" as a substring but not as a whole `/api/` path segment — this must
    // still be vetoed (no real API-ish segment present), not fooled by the substring.
    let provides = vec![provide("GET /a", "api.ts", 1)];
    let consumes = vec![consume(
        "http",
        Some("GET /apiary/export.json"),
        "client.ts",
        3,
    )];
    assert!(unprovided_consume_findings(&provides, &consumes).is_empty());
}

#[test]
fn a_path_that_only_contains_an_asset_extension_mid_segment_is_not_vetoed() {
    let provides = vec![provide("GET /a", "api.ts", 1)];
    let consumes = vec![consume(
        "http",
        Some("GET /api/json-export"),
        "client.ts",
        3,
    )];
    let found = unprovided_consume_findings(&provides, &consumes);
    assert_eq!(found.len(), 1, "{:?}", found);
}

#[test]
fn matched_consume_is_never_flagged() {
    let provides = vec![provide("GET /a", "api.ts", 1)];
    let consumes = vec![consume("http", Some("GET /a"), "client.ts", 3)];
    assert!(unprovided_consume_findings(&provides, &consumes).is_empty());
}

#[test]
fn zero_http_provides_vetoes_every_consume_pure_fe_tree() {
    let consumes = vec![consume("http", Some("GET /remote"), "client.ts", 3)];
    assert!(unprovided_consume_findings(&[], &consumes).is_empty());
}

#[test]
fn unresolved_consume_key_none_is_never_flagged() {
    let provides = vec![provide("GET /a", "api.ts", 1)];
    let consumes = vec![consume("http", None, "client.ts", 3)];
    assert!(unprovided_consume_findings(&provides, &consumes).is_empty());
}

#[test]
fn non_http_consume_kind_is_ignored() {
    let provides = vec![provide("GET /a", "api.ts", 1)];
    let consumes = vec![consume("queue", Some("topic:x"), "client.ts", 3)];
    assert!(unprovided_consume_findings(&provides, &consumes).is_empty());
}

#[test]
fn a_non_http_provide_does_not_satisfy_the_zero_provides_gate() {
    let provides = vec![zzop_core::IoProvide {
        body: None,
        kind: "queue".to_string(),
        key: "topic:x".to_string(),
        file: "worker.ts".to_string(),
        line: 1,
        symbol: None,
    }];
    let consumes = vec![consume("http", Some("GET /missing"), "client.ts", 3)];
    assert!(unprovided_consume_findings(&provides, &consumes).is_empty());
}

#[test]
fn localhost_absolute_url_consume_is_vetoed() {
    // The host-carrying key can never string-match the internal, extension-free provided key ("GET
    // /a"), so it must be skipped rather than wrongly flagged.
    let provides = vec![provide("GET /a", "api.ts", 1)];
    let consumes = vec![consume(
        "http",
        Some("GET https://localhost:3000/api/users"),
        "client.ts",
        3,
    )];
    assert!(unprovided_consume_findings(&provides, &consumes).is_empty());
}

#[test]
fn localhost_with_port_and_path_absolute_url_consume_is_vetoed() {
    let provides = vec![provide("GET /a", "api.ts", 1)];
    let consumes = vec![consume(
        "http",
        Some("POST https://localhost:8080/api/orders/create"),
        "client.ts",
        7,
    )];
    assert!(unprovided_consume_findings(&provides, &consumes).is_empty());
}

#[test]
fn loopback_ip_absolute_url_consume_is_vetoed() {
    let provides = vec![provide("GET /a", "api.ts", 1)];
    let consumes = vec![consume(
        "http",
        Some("GET https://127.0.0.1:3000/api/users"),
        "client.ts",
        3,
    )];
    assert!(unprovided_consume_findings(&provides, &consumes).is_empty());
}

#[test]
fn non_localhost_absolute_url_consume_is_still_flagged_when_unprovided() {
    // Negative control: a non-localhost absolute URL must NOT be swept up by the new veto — it keeps
    // going through the existing join/veto logic exactly as before (still flagged when unmatched).
    let provides = vec![provide("GET /a", "api.ts", 1)];
    let consumes = vec![consume(
        "http",
        Some("GET https://api.stripe.com/v1/charges"),
        "client.ts",
        3,
    )];
    let found = unprovided_consume_findings(&provides, &consumes);
    assert_eq!(found.len(), 1, "{:?}", found);
    assert!(found[0]
        .message
        .contains("https://api.stripe.com/v1/charges"));
}

#[test]
fn results_sorted_by_file_then_line() {
    // All three consumes share the provide's first segment ("a") so they stay individual (overlapping,
    // never folded) — this test is only about the final sort order, not the fold behavior below.
    let provides = vec![provide("GET /a/base", "api.ts", 1)];
    let consumes = vec![
        consume("http", Some("GET /a/x"), "b.ts", 5),
        consume("http", Some("GET /a/y"), "a.ts", 9),
        consume("http", Some("GET /a/z"), "a.ts", 2),
    ];
    let found = unprovided_consume_findings(&provides, &consumes);
    assert_eq!(found.len(), 3);
    assert_eq!(
        found
            .iter()
            .map(|f| (f.file.as_str(), f.line))
            .collect::<Vec<_>>(),
        vec![("a.ts", 2), ("a.ts", 9), ("b.ts", 5)]
    );
}

// -----------------------------------------------------------------------------------------
// Foreign-vs-overlapping fold (see module doc "Foreign-vs-overlapping fold").
// -----------------------------------------------------------------------------------------

#[test]
fn field_case_nine_foreign_unmatched_consumes_fold_into_one_aggregate() {
    // A tree that provides a handful of routes under one
    // family (/settle) but whose sibling apps' routes (served outside this analysis) leak in as
    // consumes spread across several foreign first segments.
    let provides = vec![
        provide("GET /settle/a", "settle.ts", 1),
        provide("GET /settle/b", "settle.ts", 2),
        provide("GET /settle/c", "settle.ts", 3),
        provide("GET /settle/d", "settle.ts", 4),
        provide("GET /settle/e", "settle.ts", 5),
    ];
    let consumes = vec![
        consume("http", Some("GET /orders/1"), "client.ts", 10),
        consume("http", Some("GET /orders/2"), "client.ts", 11),
        consume("http", Some("GET /orders/3"), "client.ts", 12),
        consume("http", Some("GET /users/1"), "client.ts", 13),
        consume("http", Some("GET /users/2"), "client.ts", 14),
        consume("http", Some("GET /users/3"), "client.ts", 15),
        consume("http", Some("GET /billing/1"), "client.ts", 16),
        consume("http", Some("GET /billing/2"), "client.ts", 17),
        consume("http", Some("GET /billing/3"), "client.ts", 18),
    ];
    let found = unprovided_consume_findings(&provides, &consumes);
    assert_eq!(found.len(), 1, "{:?}", found);
    let f = &found[0];
    assert_eq!(f.rule_id, "unprovided-consume");
    assert_eq!(f.severity, zzop_core::Severity::Info);
    let data = f.data.as_ref().unwrap();
    assert_eq!(data["callCount"], 9);
    let routes = data["routes"].as_array().unwrap();
    assert_eq!(routes.len(), 9);
    for c in &consumes {
        let key = c.key.as_ref().unwrap();
        assert!(
            routes.iter().any(|r| r.as_str() == Some(key.as_str())),
            "missing {key} in routes: {routes:?}"
        );
        assert!(f.message.contains(key.as_str()), "missing {key} in message");
    }
    assert!(f.message.contains("9 calls"));
    assert!(f.message.contains("settle"));
    assert!(f.message.contains("This replaces 9 individual"));
}

#[test]
fn overlapping_unmatched_consume_keeps_the_individual_finding_shape() {
    // First-segment overlap ("api") preserves today's individual, byte-for-byte finding — this is the
    // typo/removed-route signal the fold must not swallow.
    let provides = vec![provide("GET /api/users", "api.ts", 1)];
    let consumes = vec![consume("http", Some("GET /api/missing"), "client.ts", 3)];
    let found = unprovided_consume_findings(&provides, &consumes);
    assert_eq!(found.len(), 1, "{:?}", found);
    assert_eq!(found[0].file, "client.ts");
    assert_eq!(found[0].line, 3);
    assert!(found[0].message.contains("GET /api/missing"));
    assert!(found[0]
        .message
        .starts_with("This call consumes `GET /api/missing`"));
    assert_eq!(
        found[0].data.as_ref().unwrap()["key"].as_str(),
        Some("GET /api/missing")
    );
    // Paste-ready injection stub for the "route in a file this analysis didn't parse" case.
    assert_eq!(
        found[0].data.as_ref().unwrap()["injectionStub"].as_str(),
        Some("routes: [{ \"key\": \"GET /api/missing\", \"role\": \"provide\" }]")
    );
}

#[test]
fn fires_at_threshold_not_below() {
    // Mirrors `cross-layer/prefix-drift`'s `fires_at_threshold_not_below` pin naming/shape.
    let provides = vec![provide("GET /settle/a", "settle.ts", 1)];
    let three_foreign = vec![
        consume("http", Some("GET /orders/1"), "client.ts", 10),
        consume("http", Some("GET /orders/2"), "client.ts", 11),
        consume("http", Some("GET /orders/3"), "client.ts", 12),
    ];
    let found = unprovided_consume_findings(&provides, &three_foreign);
    assert_eq!(found.len(), 1, "{:?}", found);
    assert_eq!(found[0].data.as_ref().unwrap()["callCount"], 3);

    // Below threshold (only 2 foreign consumes): must stay individual, not fold.
    let two_foreign = vec![
        consume("http", Some("GET /orders/1"), "client.ts", 10),
        consume("http", Some("GET /orders/2"), "client.ts", 11),
    ];
    let below = unprovided_consume_findings(&provides, &two_foreign);
    assert_eq!(below.len(), 2, "{:?}", below);
    assert!(below
        .iter()
        .all(|f| f.data.as_ref().unwrap().get("callCount").is_none()));
}

#[test]
fn mixed_overlapping_and_foreign_consumes_split_correctly() {
    // 1 overlapping (stays individual) + 3 foreign (fold into 1 aggregate) => 2 total findings.
    let provides = vec![provide("GET /api/users", "api.ts", 1)];
    let consumes = vec![
        consume("http", Some("GET /api/missing"), "client.ts", 3),
        consume("http", Some("GET /orders/1"), "client.ts", 10),
        consume("http", Some("GET /orders/2"), "client.ts", 11),
        consume("http", Some("GET /orders/3"), "client.ts", 12),
    ];
    let found = unprovided_consume_findings(&provides, &consumes);
    assert_eq!(found.len(), 2, "{:?}", found);

    let individual = found
        .iter()
        .find(|f| f.data.as_ref().unwrap().get("key").is_some())
        .expect("individual finding for the overlapping consume");
    assert_eq!(
        individual.data.as_ref().unwrap()["key"].as_str(),
        Some("GET /api/missing")
    );

    let aggregate = found
        .iter()
        .find(|f| f.data.as_ref().unwrap().get("callCount").is_some())
        .expect("aggregate finding for the 3 foreign consumes");
    assert_eq!(aggregate.data.as_ref().unwrap()["callCount"], 3);
}

#[test]
fn all_slot_consume_with_no_path_segment_overlap_counts_as_foreign() {
    // GET /{} — the all-slot placeholder is compared as a literal token ("{}"), which is not in the
    // /settle-only provide space, so it counts as foreign like any other non-overlapping segment.
    let provides = vec![provide("GET /settle/a", "settle.ts", 1)];
    let consumes = vec![
        consume("http", Some("GET /{}"), "client.ts", 10),
        consume("http", Some("GET /orders/2"), "client.ts", 11),
        consume("http", Some("GET /orders/3"), "client.ts", 12),
    ];
    let found = unprovided_consume_findings(&provides, &consumes);
    assert_eq!(found.len(), 1, "{:?}", found);
    assert_eq!(found[0].data.as_ref().unwrap()["callCount"], 3);
    let routes = found[0].data.as_ref().unwrap()["routes"]
        .as_array()
        .unwrap();
    assert!(routes.iter().any(|r| r.as_str() == Some("GET /{}")));
}

#[test]
fn aggregate_message_appends_ellipsis_when_more_than_three_provide_segments() {
    // 4 distinct provided first segments — only the first 3 (alphabetical, via BTreeSet) are
    // rendered inline, so the message must append an ellipsis to avoid implying the tree only
    // provides those 3 path families.
    let provides = vec![
        provide("GET /alpha/a", "api.ts", 1),
        provide("GET /beta/a", "api.ts", 2),
        provide("GET /gamma/a", "api.ts", 3),
        provide("GET /delta/a", "api.ts", 4),
    ];
    let consumes = vec![
        consume("http", Some("GET /orders/1"), "client.ts", 10),
        consume("http", Some("GET /orders/2"), "client.ts", 11),
        consume("http", Some("GET /orders/3"), "client.ts", 12),
    ];
    let found = unprovided_consume_findings(&provides, &consumes);
    assert_eq!(found.len(), 1, "{:?}", found);
    assert!(
        found[0].message.contains("alpha, beta, delta, …"),
        "{}",
        found[0].message
    );
}

#[test]
fn aggregate_message_does_not_dangle_when_the_only_provides_are_root_path() {
    // A tree whose only http provides are `GET /` contributes zero
    // first-segments (`first_path_segment` returns `None` for `/`), so the normal "{m} provide(s)
    // under {segments}" clause would render as a dangling "under " with nothing after it. The
    // reworded clause must not contain that dangling construct.
    let provides = vec![provide("GET /", "app.ts", 1)];
    let consumes = vec![
        consume("http", Some("GET /orders/1"), "client.ts", 10),
        consume("http", Some("GET /orders/2"), "client.ts", 11),
        consume("http", Some("GET /orders/3"), "client.ts", 12),
    ];
    let found = unprovided_consume_findings(&provides, &consumes);
    assert_eq!(found.len(), 1, "{:?}", found);
    let message = &found[0].message;
    assert!(
        !message.contains("under (")
            && !message.contains("under )")
            && !message.contains("under  "),
        "message must not dangle a trailing \"under\" with nothing after it: {message}"
    );
    assert!(
        message.contains("none under a named path prefix"),
        "expected the reworded no-segments clause: {message}"
    );
}

#[test]
fn aggregate_message_omits_ellipsis_when_three_or_fewer_provide_segments() {
    // Exactly 3 distinct provided first segments — all of them fit inline, so no ellipsis should
    // be appended (negative control for the ellipsis added above).
    let provides = vec![
        provide("GET /alpha/a", "api.ts", 1),
        provide("GET /beta/a", "api.ts", 2),
        provide("GET /gamma/a", "api.ts", 3),
    ];
    let consumes = vec![
        consume("http", Some("GET /orders/1"), "client.ts", 10),
        consume("http", Some("GET /orders/2"), "client.ts", 11),
        consume("http", Some("GET /orders/3"), "client.ts", 12),
    ];
    let found = unprovided_consume_findings(&provides, &consumes);
    assert_eq!(found.len(), 1, "{:?}", found);
    assert!(
        found[0].message.contains("alpha, beta, gamma"),
        "{}",
        found[0].message
    );
    assert!(!found[0].message.contains('…'), "{}", found[0].message);
}
