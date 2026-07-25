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
        retry_configured: None,
    }
}

#[test]
fn unmatched_consume_is_flagged_when_the_tree_has_a_provide() {
    let provides = vec![provide("GET /a", "api.ts", 1)];
    let consumes = vec![consume("http", Some("GET /missing"), "client.ts", 3)];
    let found = unprovided_consume_findings(&provides, &consumes, &[]);
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
    let found = unprovided_consume_findings(&provides, &consumes, &[]);
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
    let found = unprovided_consume_findings(&provides, &consumes, &[]);
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
    assert!(unprovided_consume_findings(&provides, &consumes, &[]).is_empty());
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
    assert!(unprovided_consume_findings(&provides, &consumes, &[]).is_empty());
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
    assert!(unprovided_consume_findings(&provides, &consumes, &[]).is_empty());
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
    assert!(unprovided_consume_findings(&provides, &consumes, &[]).is_empty());
}

#[test]
fn next_js_public_prefix_stripped_json_path_is_vetoed() {
    // Some frameworks serve public/ files with the `public/` prefix stripped from the URL — no
    // asset-directory segment survives in the key, but the API-segment gate still catches this.
    let provides = vec![provide("GET /a", "api.ts", 1)];
    let consumes = vec![consume("http", Some("GET /i18n/ko.json"), "client.ts", 3)];
    assert!(unprovided_consume_findings(&provides, &consumes, &[]).is_empty());
}

#[test]
fn rails_style_json_api_route_with_an_api_segment_still_fires() {
    // GET /api/users.json — Rails-style format-suffixed API route, real API consumption; the /api/
    // segment stops the default json/xml veto from applying.
    let provides = vec![provide("GET /a", "api.ts", 1)];
    let consumes = vec![consume("http", Some("GET /api/users.json"), "client.ts", 3)];
    let found = unprovided_consume_findings(&provides, &consumes, &[]);
    assert_eq!(found.len(), 1, "{:?}", found);
    assert_eq!(found[0].severity, zzop_core::Severity::Info);
    assert!(found[0].message.contains("GET /api/users.json"));
}

#[test]
fn xml_with_an_api_segment_still_fires() {
    let provides = vec![provide("GET /a", "api.ts", 1)];
    let consumes = vec![consume("http", Some("GET /api/feed.xml"), "client.ts", 3)];
    let found = unprovided_consume_findings(&provides, &consumes, &[]);
    assert_eq!(found.len(), 1, "{:?}", found);
}

#[test]
fn versioned_api_segment_json_route_still_fires() {
    let provides = vec![provide("GET /a", "api.ts", 1)];
    let consumes = vec![consume("http", Some("GET /v1/users.json"), "client.ts", 3)];
    let found = unprovided_consume_findings(&provides, &consumes, &[]);
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
    let found = unprovided_consume_findings(&provides, &consumes, &[]);
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
    assert!(unprovided_consume_findings(&provides, &consumes, &[]).is_empty());
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
    assert!(unprovided_consume_findings(&provides, &consumes, &[]).is_empty());
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
    let found = unprovided_consume_findings(&provides, &consumes, &[]);
    assert_eq!(found.len(), 1, "{:?}", found);
}

#[test]
fn matched_consume_is_never_flagged() {
    let provides = vec![provide("GET /a", "api.ts", 1)];
    let consumes = vec![consume("http", Some("GET /a"), "client.ts", 3)];
    assert!(unprovided_consume_findings(&provides, &consumes, &[]).is_empty());
}

#[test]
fn zero_http_provides_vetoes_every_consume_pure_fe_tree() {
    let consumes = vec![consume("http", Some("GET /remote"), "client.ts", 3)];
    assert!(unprovided_consume_findings(&[], &consumes, &[]).is_empty());
}

#[test]
fn unresolved_consume_key_none_is_never_flagged() {
    let provides = vec![provide("GET /a", "api.ts", 1)];
    let consumes = vec![consume("http", None, "client.ts", 3)];
    assert!(unprovided_consume_findings(&provides, &consumes, &[]).is_empty());
}

#[test]
fn non_http_consume_kind_is_ignored() {
    let provides = vec![provide("GET /a", "api.ts", 1)];
    let consumes = vec![consume("queue", Some("topic:x"), "client.ts", 3)];
    assert!(unprovided_consume_findings(&provides, &consumes, &[]).is_empty());
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
    assert!(unprovided_consume_findings(&provides, &consumes, &[]).is_empty());
}

#[test]
fn localhost_absolute_url_consume_is_vetoed() {
    // Localhost is a strict SUBSET of the absolute-URL veto (module doc "Absolute-URL
    // (external-egress) veto"): the host-carrying key can never string-match the internal,
    // extension-free provided key ("GET /a"), so it must be skipped rather than wrongly flagged.
    let provides = vec![provide("GET /a", "api.ts", 1)];
    let consumes = vec![consume(
        "http",
        Some("GET https://localhost:3000/api/users"),
        "client.ts",
        3,
    )];
    assert!(unprovided_consume_findings(&provides, &consumes, &[]).is_empty());
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
    assert!(unprovided_consume_findings(&provides, &consumes, &[]).is_empty());
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
    assert!(unprovided_consume_findings(&provides, &consumes, &[]).is_empty());
}

#[test]
fn third_party_absolute_url_consume_is_vetoed_as_external_egress() {
    // Linker-contract parity. `CrossLayerResult::external_consumes` in `crates/core/src/io/facts.rs`
    // is the contract: a consume key carrying a host ("://") is third-party egress, "never counted as
    // `unprovidedConsumes`, since an unmatched absolute-URL consume is expected ... not drift". This
    // rule used to flag exactly that, so one fact read as drift single-tree and as external egress
    // multi-tree. Do NOT restore the old "non-localhost absolute URL is still flagged" assertion —
    // that would re-open the contradiction. Field key that exposed it: an unmatched vendor API GET.
    let provides = vec![provide("GET /a", "api.ts", 1)];
    let consumes = vec![
        consume(
            "http",
            Some("GET https://api.sunrise-sunset.org/json?lat=36.7&lng=-119.7"),
            "client.ts",
            3,
        ),
        consume(
            "http",
            Some("POST https://api.stripe.com/v1/charges"),
            "client.ts",
            9,
        ),
    ];
    assert!(
        unprovided_consume_findings(&provides, &consumes, &[]).is_empty(),
        "absolute-URL consumes are external egress, never drift"
    );
}

#[test]
fn absolute_url_consumes_never_reach_the_foreign_fold() {
    // The veto runs before the foreign/overlapping split, so three host-carrying keys must not
    // accumulate into an aggregate finding either (the fold can only replace findings, never create).
    let provides = vec![provide("GET /settle/a", "settle.ts", 1)];
    let consumes = vec![
        consume("http", Some("GET https://vendor.com/orders/1"), "c.ts", 10),
        consume("http", Some("GET https://vendor.com/orders/2"), "c.ts", 11),
        consume("http", Some("GET http://vendor.com/orders/3"), "c.ts", 12),
    ];
    assert!(unprovided_consume_findings(&provides, &consumes, &[]).is_empty());
}

#[test]
fn a_relative_api_path_is_still_flagged_alongside_a_vetoed_absolute_url() {
    // Negative control for the absolute-URL veto: it must only swallow host-carrying keys, leaving a
    // relative, internal `/api/...` consume flagged exactly as before.
    let provides = vec![provide("GET /api/users", "api.ts", 1)];
    let consumes = vec![
        consume(
            "http",
            Some("GET https://api.stripe.com/v1/charges"),
            "client.ts",
            3,
        ),
        consume("http", Some("GET /api/missing"), "client.ts", 5),
    ];
    let found = unprovided_consume_findings(&provides, &consumes, &[]);
    assert_eq!(found.len(), 1, "{:?}", found);
    assert_eq!(
        found[0].data.as_ref().unwrap()["key"].as_str(),
        Some("GET /api/missing")
    );
}

#[test]
fn the_message_states_the_absolute_url_veto_it_actually_applies() {
    // Message contract: the finding text must describe the veto the code performs, not the narrower
    // localhost-only skip it used to describe.
    let provides = vec![provide("GET /api/users", "api.ts", 1)];
    let consumes = vec![consume("http", Some("GET /api/missing"), "client.ts", 3)];
    let found = unprovided_consume_findings(&provides, &consumes, &[]);
    let message = &found[0].message;
    assert!(message.contains("`://`"), "{message}");
    assert!(message.contains("externalConsumes"), "{message}");
    assert!(
        !message.contains("dev self-reference and is never flagged"),
        "stale localhost-only wording must be gone: {message}"
    );
}

#[test]
fn placeholder_i18n_json_asset_fetch_is_vetoed() {
    // Field key: a Next.js `public/i18n/*.json` fetch built from a template literal keys as
    // `GET /i18n/{}.json`. The json/xml tier vetoes it via the ABSENT API segment (module doc).
    let provides = vec![provide("GET /api/users", "api.ts", 1)];
    let consumes = vec![consume("http", Some("GET /i18n/{}.json"), "client.ts", 3)];
    assert!(unprovided_consume_findings(&provides, &consumes, &[]).is_empty());
}

#[test]
fn family_completing_static_asset_extensions_are_vetoed() {
    // Extensions added to ALWAYS_VETO_EXTENSION_PATTERN complete the image/font/script families that
    // were already there; none of them can name an API route shape.
    let provides = vec![provide("GET /a", "api.ts", 1)];
    for key in [
        "GET /assets/app.mjs",
        "GET /assets/app.cjs",
        "GET /assets/hero.avif",
        "GET /assets/logo.bmp",
        "GET /fonts/inter.ttf",
        "GET /fonts/inter.otf",
        "GET /fonts/legacy.eot",
        "GET /fonts/inter.TTF?v=3",
    ] {
        let consumes = vec![consume("http", Some(key), "client.ts", 3)];
        assert!(
            unprovided_consume_findings(&provides, &consumes, &[]).is_empty(),
            "expected {key} to be vetoed as a static asset"
        );
    }
}

#[test]
fn an_api_route_whose_path_merely_contains_an_asset_word_still_fires() {
    // Negative control for the widened extension vocabulary: the anchor is still end-of-path, so a
    // route whose path only mentions an asset name mid-segment stays flaggable.
    let provides = vec![provide("GET /api/users", "api.ts", 1)];
    for key in ["GET /api/otf-templates", "GET /api/avif/convert"] {
        let consumes = vec![consume("http", Some(key), "client.ts", 3)];
        let found = unprovided_consume_findings(&provides, &consumes, &[]);
        assert_eq!(found.len(), 1, "expected {key} to fire: {found:?}");
    }
}

#[test]
fn findings_are_deterministic_across_repeated_runs() {
    // Detection-surface change re-verification: the veto set is order-independent and the output is
    // byte-identical run to run (same keys, same anchors, same messages).
    let provides = vec![provide("GET /api/users", "api.ts", 1)];
    let consumes = vec![
        consume("http", Some("GET https://vendor.com/x"), "client.ts", 2),
        consume("http", Some("GET /api/missing"), "client.ts", 3),
        consume("http", Some("GET /i18n/{}.json"), "client.ts", 4),
        consume("http", Some("GET /orders/1"), "client.ts", 5),
        consume("http", Some("GET /orders/2"), "client.ts", 6),
        consume("http", Some("GET /orders/3"), "client.ts", 7),
    ];
    let first = unprovided_consume_findings(&provides, &consumes, &[]);
    let second = unprovided_consume_findings(&provides, &consumes, &[]);
    let render = |fs: &[zzop_core::Finding]| {
        fs.iter()
            .map(|f| format!("{}:{}:{}", f.file, f.line, f.message))
            .collect::<Vec<_>>()
    };
    assert_eq!(render(&first), render(&second));
    // 1 overlapping individual + 1 foreign aggregate; the vendor URL and the i18n asset are vetoed.
    assert_eq!(first.len(), 2, "{first:?}");
}

// -----------------------------------------------------------------------------------------
// Declared-host re-key (module doc "Structural gates"). The transform itself lives inside
// `zzop_core::classify_consume_join` and is pinned by the linker's own
// `crates/core/src/io/link/tests/hosts.rs`; what these cases pin is this rule's WIRING of it — that it
// runs before every veto, and what the resulting finding says.
// -----------------------------------------------------------------------------------------

fn hosts(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn a_declared_host_absolute_url_is_rekeyed_and_joins_this_trees_provide() {
    // The mirror-gap defect: without the re-key this call is vetoed as egress here while the multi-tree
    // join re-keys it and matches — same fact, opposite answers, one level narrower than the `://` bug.
    let provides = vec![provide("GET /api/users", "api.ts", 1)];
    let consumes = vec![consume(
        "http",
        Some("GET https://gw.example.com/api/users"),
        "client.ts",
        3,
    )];
    let found = unprovided_consume_findings(&provides, &consumes, &hosts(&["gw.example.com"]));
    assert!(
        found.is_empty(),
        "declared host must join, not be vetoed: {found:?}"
    );
}

#[test]
fn a_rekeyed_consume_with_no_provider_is_flagged_under_its_internal_join_key() {
    let provides = vec![provide("GET /api/users", "api.ts", 1)];
    let consumes = vec![consume(
        "http",
        Some("GET https://gw.example.com/api/missing?x=1"),
        "client.ts",
        3,
    )];
    let found = unprovided_consume_findings(&provides, &consumes, &hosts(&["gw.example.com"]));
    assert_eq!(found.len(), 1, "{found:?}");
    let data = found[0].data.as_ref().unwrap();
    // Bucket invariant parity: the reported key is the scheme-free JOIN key, never the absolute spelling
    // (the query string is dropped by `http_consume_interface_key`, same as any other consume key).
    assert_eq!(data["key"].as_str(), Some("GET /api/missing"));
    assert_eq!(
        data["rawKey"].as_str(),
        Some("GET https://gw.example.com/api/missing?x=1")
    );
    assert!(
        found[0]
            .message
            .contains("https://gw.example.com/api/missing?x=1"),
        "{}",
        found[0].message
    );
    assert!(found[0].message.contains("re-keyed to the internal path"));
}

#[test]
fn consume_side_port_is_ignored_when_the_declared_host_carries_none() {
    let provides = vec![provide("GET /api/users", "api.ts", 1)];
    let consumes = vec![consume(
        "http",
        Some("GET https://gw.example.com:8080/api/users"),
        "client.ts",
        3,
    )];
    assert!(
        unprovided_consume_findings(&provides, &consumes, &hosts(&["gw.example.com"])).is_empty()
    );
}

#[test]
fn a_declared_host_with_a_port_requires_an_exact_match() {
    let provides = vec![provide("GET /api/users", "api.ts", 1)];
    let declared = hosts(&["gw.example.com:8080"]);
    // Exact host:port joins.
    let hit = vec![consume(
        "http",
        Some("GET https://gw.example.com:8080/api/users"),
        "client.ts",
        3,
    )];
    assert!(unprovided_consume_findings(&provides, &hit, &declared).is_empty());
    // The miss cases (wrong port, no port at all) are unobservable from this rule — a non-re-keyed
    // absolute URL is vetoed as egress, so both outcomes are "no finding". They are pinned on the
    // transform itself, in `crates/core/src/io/link/tests/hosts.rs`'s same-named test, which can
    // observe them as external-bucket entries.
}

#[test]
fn declared_host_matching_is_ascii_case_insensitive() {
    let provides = vec![provide("GET /api/users", "api.ts", 1)];
    let consumes = vec![consume(
        "http",
        Some("GET https://GW.Example.COM/api/users"),
        "client.ts",
        3,
    )];
    assert!(
        unprovided_consume_findings(&provides, &consumes, &hosts(&["gw.example.com"])).is_empty()
    );
}

// A `ws://` key is never re-keyed even when its host is declared (so the absolute-URL veto still
// applies): a property of the shared transform, pinned in `crates/core/src/io/link/tests/hosts.rs`
// (`ws_scheme_stays_external_even_when_the_host_is_declared_internal`).

#[test]
fn an_undeclared_host_is_still_vetoed_as_egress() {
    let provides = vec![provide("GET /api/users", "api.ts", 1)];
    let consumes = vec![consume(
        "http",
        Some("GET https://vendor.com/api/users"),
        "client.ts",
        3,
    )];
    assert!(
        unprovided_consume_findings(&provides, &consumes, &hosts(&["gw.example.com"])).is_empty()
    );
}

#[test]
fn declaring_no_hosts_leaves_the_absolute_url_veto_byte_identical() {
    // The re-key is inert for the overwhelmingly common tree that declares nothing.
    let provides = vec![provide("GET /api/users", "api.ts", 1)];
    let consumes = vec![
        consume(
            "http",
            Some("GET https://gw.example.com/api/users"),
            "c.ts",
            3,
        ),
        consume("http", Some("GET /api/missing"), "c.ts", 5),
    ];
    let found = unprovided_consume_findings(&provides, &consumes, &[]);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(
        found[0].data.as_ref().unwrap()["key"].as_str(),
        Some("GET /api/missing")
    );
    assert!(found[0].data.as_ref().unwrap().get("rawKey").is_none());
}

#[test]
fn an_aggregate_names_the_absolute_spellings_of_its_rekeyed_entries() {
    // A folded entry is enumerated under its JOIN key, so without this the message shows an internal
    // path the author cannot grep for anywhere in their source.
    let provides = vec![provide("GET /settle/a", "settle.ts", 1)];
    let consumes = vec![
        consume(
            "http",
            Some("GET https://gw.example.com/orders/1"),
            "c.ts",
            10,
        ),
        consume("http", Some("GET /orders/2"), "c.ts", 11),
        consume("http", Some("GET /orders/3"), "c.ts", 12),
    ];
    let found = unprovided_consume_findings(&provides, &consumes, &hosts(&["gw.example.com"]));
    assert_eq!(found.len(), 1, "{found:?}");
    let data = found[0].data.as_ref().unwrap();
    assert_eq!(data["callCount"], 3);
    let routes = data["routes"].as_array().unwrap();
    assert!(routes.iter().any(|r| r.as_str() == Some("GET /orders/1")));
    assert_eq!(
        data["rawKeys"].as_array().unwrap()[0].as_str(),
        Some("GET https://gw.example.com/orders/1")
    );
    assert!(
        found[0]
            .message
            .contains("as written in the source they are: GET https://gw.example.com/orders/1"),
        "{}",
        found[0].message
    );
}

#[test]
fn an_aggregate_with_no_rekeyed_entry_carries_no_raw_keys_field() {
    let provides = vec![provide("GET /settle/a", "settle.ts", 1)];
    let consumes = vec![
        consume("http", Some("GET /orders/1"), "c.ts", 10),
        consume("http", Some("GET /orders/2"), "c.ts", 11),
        consume("http", Some("GET /orders/3"), "c.ts", 12),
    ];
    let found = unprovided_consume_findings(&provides, &consumes, &[]);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].data.as_ref().unwrap().get("rawKeys").is_none());
    assert!(!found[0].message.contains("as written in the source"));
}

// -----------------------------------------------------------------------------------------
// Veto/fold interaction (module doc "A veto can RAISE the finding count").
// -----------------------------------------------------------------------------------------

#[test]
fn vetoing_one_sibling_turns_one_aggregate_into_two_individual_findings() {
    let provides = vec![provide("GET /settle/a", "settle.ts", 1)];
    let three_real = vec![
        consume("http", Some("GET /orders/1"), "client.ts", 10),
        consume("http", Some("GET /orders/2"), "client.ts", 11),
        consume("http", Some("GET /orders/3"), "client.ts", 12),
    ];
    let folded = unprovided_consume_findings(&provides, &three_real, &[]);
    assert_eq!(folded.len(), 1, "3 foreign consumes fold: {folded:?}");
    assert_eq!(folded[0].data.as_ref().unwrap()["callCount"], 3);

    // Same tree, but one sibling is a vendor absolute URL: the veto drops the group to 2, below the fold
    // threshold, so ONE aggregate becomes TWO individual findings — at anchors the aggregate never used.
    let one_vetoed = vec![
        consume("http", Some("GET /orders/1"), "client.ts", 10),
        consume("http", Some("GET /orders/2"), "client.ts", 11),
        consume(
            "http",
            Some("GET https://vendor.com/orders/3"),
            "client.ts",
            12,
        ),
    ];
    let unfolded = unprovided_consume_findings(&provides, &one_vetoed, &[]);
    assert_eq!(
        unfolded.len(),
        2,
        "count RISES when a veto unfolds a group: {unfolded:?}"
    );
    assert!(unfolded
        .iter()
        .all(|f| f.data.as_ref().unwrap().get("key").is_some()));
    assert!(
        unfolded.len() > folded.len(),
        "this is the documented count inversion, not a regression"
    );
}

#[test]
fn the_individual_message_discloses_the_fold_count_inversion() {
    let provides = vec![provide("GET /api/users", "api.ts", 1)];
    let consumes = vec![consume("http", Some("GET /api/missing"), "client.ts", 3)];
    let found = unprovided_consume_findings(&provides, &consumes, &[]);
    let m = &found[0].message;
    assert!(m.contains("the finding COUNT can rise"), "{m}");
    assert!(m.contains("2 individual findings"), "{m}");
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
    let found = unprovided_consume_findings(&provides, &consumes, &[]);
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
    let found = unprovided_consume_findings(&provides, &consumes, &[]);
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
    let found = unprovided_consume_findings(&provides, &consumes, &[]);
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
    let found = unprovided_consume_findings(&provides, &three_foreign, &[]);
    assert_eq!(found.len(), 1, "{:?}", found);
    assert_eq!(found[0].data.as_ref().unwrap()["callCount"], 3);

    // Below threshold (only 2 foreign consumes): must stay individual, not fold.
    let two_foreign = vec![
        consume("http", Some("GET /orders/1"), "client.ts", 10),
        consume("http", Some("GET /orders/2"), "client.ts", 11),
    ];
    let below = unprovided_consume_findings(&provides, &two_foreign, &[]);
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
    let found = unprovided_consume_findings(&provides, &consumes, &[]);
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
fn an_all_slot_consume_key_is_vetoed_and_never_reaches_the_fold() {
    // `GET /{}` names no route (mono-hub `joke-generator/fetchJoke.ts`: an unresolved `${BASE}` prefix
    // drops the host and leaves an all-placeholder path), so it cannot be evidence that a route is
    // missing. It used to be counted as a foreign key and folded in, inventing a contract nobody
    // claimed. Vetoing it drops the group from 3 to 2 — below the fold threshold — which is exactly the
    // documented "a veto can RAISE the finding count" interaction: ONE aggregate becomes TWO individuals.
    let provides = vec![provide("GET /settle/a", "settle.ts", 1)];
    let consumes = vec![
        consume("http", Some("GET /{}"), "client.ts", 10),
        consume("http", Some("GET /orders/2"), "client.ts", 11),
        consume("http", Some("GET /orders/3"), "client.ts", 12),
    ];
    let found = unprovided_consume_findings(&provides, &consumes, &[]);
    assert_eq!(found.len(), 2, "{:?}", found);
    // Checked on `data.key`, not the message: every message now NAMES this veto in its own veto list.
    let keys: Vec<&str> = found
        .iter()
        .map(|f| f.data.as_ref().unwrap()["key"].as_str().unwrap())
        .collect();
    assert_eq!(keys, vec!["GET /orders/2", "GET /orders/3"]);
    assert!(found.iter().all(|f| f.line != 10));
}

#[test]
fn a_multi_slot_consume_key_is_vetoed_but_one_literal_segment_is_enough_to_fire() {
    // `POST /{}/{}` is just as identity-less as `GET /{}`; `GET /{}/orders` carries one literal segment,
    // which is joinable evidence, so it stays reportable. Same predicate as the multi-tree linker's
    // route-identity gate — `zzop_core::key_carries_route_identity`, not a copy of it.
    let provides = vec![provide("GET /settle/a", "settle.ts", 1)];
    let consumes = vec![
        consume("http", Some("POST /{}/{}"), "client.ts", 10),
        consume("http", Some("GET /{}/orders"), "client.ts", 11),
    ];
    let found = unprovided_consume_findings(&provides, &consumes, &[]);
    assert_eq!(found.len(), 1, "{:?}", found);
    assert_eq!(found[0].line, 11);
    assert!(found[0].message.contains("GET /{}/orders"));
}

#[test]
fn a_root_path_consume_is_resolved_evidence_and_still_fires() {
    // `GET /` has zero segments but the path is fully KNOWN — it is a real root route, not a head-drop
    // artifact, so the identity veto must not swallow it (the near-miss rules' `is_all_slot_path` gate
    // answers a different question and does treat it as vacuous; see the core predicate's doc).
    let provides = vec![provide("GET /settle/a", "settle.ts", 1)];
    let consumes = vec![consume("http", Some("GET /"), "client.ts", 10)];
    let found = unprovided_consume_findings(&provides, &consumes, &[]);
    assert_eq!(found.len(), 1, "{:?}", found);
    assert!(found[0].message.contains("consumes `GET /`"));
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
    let found = unprovided_consume_findings(&provides, &consumes, &[]);
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
    let found = unprovided_consume_findings(&provides, &consumes, &[]);
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
    let found = unprovided_consume_findings(&provides, &consumes, &[]);
    assert_eq!(found.len(), 1, "{:?}", found);
    assert!(
        found[0].message.contains("alpha, beta, gamma"),
        "{}",
        found[0].message
    );
    assert!(!found[0].message.contains('…'), "{}", found[0].message);
}
