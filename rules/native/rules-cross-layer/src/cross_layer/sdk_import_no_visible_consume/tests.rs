use super::*;

fn pkg(source: &str, specifier: &str, file_count: usize, example_file: &str) -> PackageImportSite {
    PackageImportSite {
        source: source.to_string(),
        specifier: specifier.to_string(),
        file_count,
        example_file: example_file.to_string(),
    }
}

#[test]
fn sdk_import_with_no_visible_consumes_fires_once_per_tree() {
    let imports = vec![
        pkg("web", "@acme/sdk", 40, "src/lib/api.ts"),
        pkg("web", "svelte", 200, "src/App.svelte"),
    ];
    let totals = vec![("web".to_string(), 0)];
    let out = sdk_import_no_visible_consume_findings(&imports, &totals);
    assert_eq!(out.len(), 1);
    assert_eq!(
        out[0].rule_id,
        "cross-layer/untraced-client-import-no-visible-consume"
    );
    assert_eq!(out[0].file, "src/lib/api.ts");
    let data = out[0].data.as_ref().unwrap();
    assert_eq!(data["visibleHttpConsumes"], 0);
    assert_eq!(data["sdkPackages"].as_array().unwrap().len(), 1);
    assert_eq!(data["sdkPackages"][0]["kind"], "sdk");
}

#[test]
fn visible_consumes_at_the_ratio_rule_floor_hand_off_instead_of_firing() {
    let imports = vec![pkg("web", "@acme/sdk", 40, "src/lib/api.ts")];
    let totals = vec![("web".to_string(), MIN_TOTAL_CONSUMES)];
    assert!(sdk_import_no_visible_consume_findings(&imports, &totals).is_empty());
}

#[test]
fn sdk_imported_from_too_few_files_does_not_fire() {
    let imports = vec![pkg("web", "@acme/sdk", 2, "src/lib/api.ts")];
    let totals = vec![("web".to_string(), 0)];
    assert!(sdk_import_no_visible_consume_findings(&imports, &totals).is_empty());
}

#[test]
fn non_sdk_specifiers_do_not_fire_and_segment_anchoring_holds() {
    let imports = vec![
        pkg("web", "react", 100, "src/App.tsx"),
        pkg("web", "sdkim-utils", 10, "src/x.ts"), // "sdk" not a whole segment
    ];
    let totals = vec![("web".to_string(), 0)];
    assert!(sdk_import_no_visible_consume_findings(&imports, &totals).is_empty());
}

#[test]
fn openapi_and_api_client_shapes_match() {
    let imports = vec![
        pkg("a", "openapi-fetch", 5, "src/a.ts"),
        pkg("b", "@acme/api-client", 6, "src/b.ts"),
    ];
    let totals = vec![("a".to_string(), 0), ("b".to_string(), 1)];
    let out = sdk_import_no_visible_consume_findings(&imports, &totals);
    assert_eq!(out.len(), 2);
    // Deterministic: sorted by anchor file.
    assert_eq!(out[0].file, "src/a.ts");
    assert_eq!(out[1].file, "src/b.ts");
}

#[test]
fn graphql_client_libraries_match_but_the_bare_graphql_server_package_does_not() {
    let imports = vec![
        pkg("fe", "@apollo/client", 50, "src/apollo.ts"),
        pkg("fe2", "@urql/core", 12, "src/urql.ts"),
        pkg("be", "graphql", 30, "src/schema.ts"), // server-side schema package: no claim
    ];
    let totals = vec![];
    let out = sdk_import_no_visible_consume_findings(&imports, &totals);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].file, "src/apollo.ts");
    assert_eq!(out[1].file, "src/urql.ts");
}

#[test]
fn tree_missing_from_totals_counts_as_zero_visible() {
    let imports = vec![pkg("web", "foo-sdk", 3, "src/api.ts")];
    let out = sdk_import_no_visible_consume_findings(&imports, &[]);
    assert_eq!(out.len(), 1);
}

#[test]
fn superagent_from_a_single_file_with_zero_visible_consumes_fires() {
    let imports = vec![pkg("web", "superagent", 1, "src/lib/client.ts")];
    let totals = vec![("web".to_string(), 0)];
    let out = sdk_import_no_visible_consume_findings(&imports, &totals);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].file, "src/lib/client.ts");
    let data = out[0].data.as_ref().unwrap();
    assert_eq!(data["sdkPackages"][0]["kind"], "opaqueClient");
}

#[test]
fn got_from_a_single_file_fires() {
    let imports = vec![pkg("web", "got", 1, "src/lib/client.ts")];
    let totals = vec![("web".to_string(), 0)];
    let out = sdk_import_no_visible_consume_findings(&imports, &totals);
    assert_eq!(out.len(), 1);
    let data = out[0].data.as_ref().unwrap();
    assert_eq!(data["sdkPackages"][0]["kind"], "opaqueClient");
}

#[test]
fn non_client_bare_package_does_not_fire() {
    let imports = vec![pkg("web", "lodash", 1, "src/util.ts")];
    let totals = vec![("web".to_string(), 0)];
    assert!(sdk_import_no_visible_consume_findings(&imports, &totals).is_empty());
}

#[test]
fn requestly_does_not_match_the_request_anchor() {
    let imports = vec![pkg("web", "requestly", 1, "src/lib/client.ts")];
    let totals = vec![("web".to_string(), 0)];
    assert!(sdk_import_no_visible_consume_findings(&imports, &totals).is_empty());
}

#[test]
fn oazapfts_from_a_single_file_with_zero_visible_consumes_fires() {
    // Native recognition of the oazapfts call family is gone (decision: generated SDKs are
    // injection adapters, not engine vocab); an unadapted oazapfts import is now an opaque client
    // just like superagent/got, so it must fire the disclosure rather than staying silent.
    let imports = vec![pkg("web", "oazapfts", 1, "src/lib/client.ts")];
    let totals = vec![("web".to_string(), 0)];
    let out = sdk_import_no_visible_consume_findings(&imports, &totals);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].file, "src/lib/client.ts");
    let data = out[0].data.as_ref().unwrap();
    assert_eq!(data["sdkPackages"][0]["kind"], "opaqueClient");
}

#[test]
fn opaque_client_at_or_above_the_ratio_rule_floor_hands_off_instead_of_firing() {
    let imports = vec![pkg("web", "superagent", 1, "src/lib/client.ts")];
    let totals = vec![("web".to_string(), MIN_TOTAL_CONSUMES)];
    assert!(sdk_import_no_visible_consume_findings(&imports, &totals).is_empty());
}

/// The parser's own recognized-client names, READ OUT OF THE PARSER — every `client: ...` value
/// `adapters::egress::matchers::match_http_call` stamps on an `IoConsume`, which IS the set of clients
/// whose calls the join can already see.
///
/// Why a text read rather than a shared symbol: T2 — `rules/**` must not depend on
/// `zzop_parser_typescript` (the layering forbids it), so there is no constant to import. The
/// alternative this replaced was a hand-copied mirror whose own comment admitted it ("keep it in sync
/// if the extractor learns a new client name") — and, measured 2026-07-28, deleting an entry from that
/// mirror kept the test green, so the mirror was the only thing the pin ever checked. Reading the
/// shipped source is the same route `packages/cli-bin/src/cli/help/tests.rs` and
/// `crates/engine/tests/rule_contracts/surface_parity.rs` take for the same reason.
///
/// The parse is deliberately dumb: on every line that assigns `client:`, take the quoted literals
/// (the two-branch forms `client: if obj == "axios" { "axios" } else { "ky" }` are the reason it is
/// per-line rather than per-literal-after-the-colon). A dumb parse can only fail by finding too
/// little, and the caller below asserts a non-empty result first.
fn parser_recognized_http_clients() -> std::collections::BTreeSet<String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../parser/parser-typescript/src/adapters/egress/matchers.rs");
    // A moved/renamed matcher module is a HARD failure, never a silent empty set.
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "cannot read the parser's egress matchers at {}: {e} — if that module moved, re-point \
             this pin rather than dropping it",
            path.display()
        )
    });
    let mut out = std::collections::BTreeSet::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") || !trimmed.contains("client:") {
            continue;
        }
        let mut rest = &trimmed[trimmed.find("client:").expect("checked above") + 7..];
        while let Some(i) = rest.find('"') {
            rest = &rest[i + 1..];
            let Some(end) = rest.find('"') else { break };
            out.insert(rest[..end].to_string());
            rest = &rest[end + 1..];
        }
    }
    out
}

/// Cross-crate contract pin (T2): `OPAQUE_HTTP_CLIENT_PATTERN` must stay DISJOINT from the HTTP clients
/// the parser's egress extractor recognizes natively. If a recognized client ALSO matched the opaque
/// pattern, its calls would be both join-visible AND counted as an opaque blind spot (double-count /
/// false blindness report). The recognized set is derived from the parser's own source rather than
/// mirrored here, so an extractor that learns a new client name is checked the moment it ships.
#[test]
fn opaque_pattern_is_disjoint_from_natively_recognized_http_clients() {
    let recognized = parser_recognized_http_clients();
    assert!(
        recognized.len() >= 3,
        "the parser scan found {recognized:?} — it has stopped matching `matchers.rs`, so this test \
         would vouch for nothing",
    );
    let opaque = regex::Regex::new(OPAQUE_HTTP_CLIENT_PATTERN).unwrap();
    for client in &recognized {
        assert!(
            !opaque.is_match(client),
            "`{client}` is recognized by the parser's egress extractor — it must not also match \
             OPAQUE_HTTP_CLIENT_PATTERN (would double-count as an opaque blind spot)",
        );
    }
}
