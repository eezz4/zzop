//! Unit tests for `external_shadow_internal_findings` — split out for file size (the crate's
//! standard `foo.rs` + `foo/tests.rs` layout, see `external_base_url_drift/tests.rs`).

use super::*;

fn consume(key: Option<&str>, source: &str, file: &str, line: u32) -> TaggedConsume {
    TaggedConsume {
        source: source.to_string(),
        consume: zzop_core::IoConsume {
            client: None,
            body: None,
            kind: "http".to_string(),
            key: key.map(str::to_string),
            file: file.to_string(),
            line,
            raw: None,
            method: None,
            retry_configured: None,
        },
    }
}

fn provide(key: &str, source: &str, file: &str, line: u32) -> HttpProvideSite {
    HttpProvideSite {
        source: source.to_string(),
        key: key.to_string(),
        file: file.to_string(),
        line,
    }
}

#[test]
fn absolute_url_matching_an_internal_route_is_flagged_anchored_at_the_consume() {
    let external = vec![consume(
        Some("GET https://app.internal.example.com/api/users"),
        "fe",
        "Ctx.tsx",
        10,
    )];
    let provides = vec![provide("GET /api/users", "be", "Api.java", 20)];
    let out = external_shadow_internal_findings(&external, &provides);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].rule_id, "cross-layer/external-shadow-internal");
    assert_eq!(out[0].severity, Severity::Warning);
    assert_eq!(out[0].file, "Ctx.tsx");
    assert_eq!(out[0].line, 10);
    assert!(out[0].message.contains("app.internal.example.com"));
    assert!(out[0].message.contains("Api.java:20"));
    assert!(out[0].message.contains("disabledRules"));
    let data = out[0].data.as_ref().unwrap();
    assert_eq!(data["host"], "app.internal.example.com");
    assert_eq!(data["normalizedKey"], "GET /api/users");
    assert_eq!(data["otherProvideCount"], 0);
    // Seals that the payload names BOTH sides of the join: the consuming tree (the anchor) and the
    // provider it matched. Before this, only `matchedProvide.source` shipped, so a consumer keying on
    // `<source>/<file>:<line>` collapsed two trees' identical relative paths onto one key.
    assert_eq!(data["consumeSource"], "fe");
    assert_eq!(data["matchedProvide"]["source"], "be");
}

/// Seals the collision the `consumeSource` field exists to prevent: two trees whose consume sites share
/// a relative path AND a line produce two findings whose only distinguishing datum is `consumeSource`.
#[test]
fn two_trees_with_the_same_consume_path_and_line_stay_distinguishable_by_consume_source() {
    let external = vec![
        consume(
            Some("GET https://app.internal.example.com/api/users"),
            "xfe",
            "src/consumes.ts",
            7,
        ),
        consume(
            Some("GET https://app.internal.example.com/api/users"),
            "xbe2",
            "src/consumes.ts",
            7,
        ),
    ];
    let provides = vec![provide("GET /api/users", "be", "Api.java", 20)];
    let out = external_shadow_internal_findings(&external, &provides);
    assert_eq!(out.len(), 2);
    let mut sources: Vec<&str> = out
        .iter()
        .map(|f| f.data.as_ref().unwrap()["consumeSource"].as_str().unwrap())
        .collect();
    sources.sort_unstable();
    assert_eq!(sources, vec!["xbe2", "xfe"]);
}

#[test]
fn unprovided_path_is_not_flagged() {
    let external = vec![consume(
        Some("GET https://api.vendor.com/v1/widgets"),
        "fe",
        "Ctx.tsx",
        10,
    )];
    let provides = vec![provide("GET /api/users", "be", "Api.java", 20)];
    assert!(external_shadow_internal_findings(&external, &provides).is_empty());
}

#[test]
fn non_http_external_consume_is_ignored() {
    let mut c = consume(
        Some("GET https://app.internal.example.com/api/users"),
        "fe",
        "Ctx.tsx",
        10,
    );
    c.consume.kind = "queue".to_string();
    let provides = vec![provide("GET /api/users", "be", "Api.java", 20)];
    assert!(external_shadow_internal_findings(&[c], &provides).is_empty());
}

#[test]
fn multiple_matching_provides_report_first_sorted_and_other_count() {
    let external = vec![consume(
        Some("GET https://app.internal.example.com/api/users"),
        "fe",
        "Ctx.tsx",
        10,
    )];
    let provides = vec![
        provide("GET /api/users", "be2", "Z.java", 1),
        provide("GET /api/users", "be1", "A.java", 5),
    ];
    let out = external_shadow_internal_findings(&external, &provides);
    assert_eq!(out.len(), 1);
    let data = out[0].data.as_ref().unwrap();
    assert_eq!(data["matchedProvide"]["source"], "be1");
    assert_eq!(data["matchedProvide"]["file"], "A.java");
    assert_eq!(data["otherProvideCount"], 1);
}

#[test]
fn consume_in_a_test_fixture_file_is_skipped() {
    let external = vec![consume(
        Some("GET https://app.internal.example.com/api/users"),
        "fe",
        "src/__tests__/Ctx.test.tsx",
        10,
    )];
    let provides = vec![provide("GET /api/users", "be", "Api.java", 20)];
    assert!(external_shadow_internal_findings(&external, &provides).is_empty());
}

/// The measured false positive, in the corpus's own bytes (mono-hub, 2026-07-25): a third-party
/// root URL whose query is dropped normalizes to `GET /`, which any worker/root handler in the
/// analysis also provides. Nothing about `v2ex.com` is internal.
#[test]
fn a_third_party_root_url_does_not_shadow_an_internal_root_handler() {
    let external = vec![consume(
        Some("GET https://www.v2ex.com/?tab=hot"),
        "@apps/community-hub-fe",
        "src/lib/sources/V2EX_HOT.ts",
        14,
    )];
    let provides = vec![provide("GET /", "@base/utils-all", "src/worker.ts", 3)];
    assert!(external_shadow_internal_findings(&external, &provides).is_empty());
}

/// Same gate, the other contentless shape: an unresolved interpolation leaves an all-slot path,
/// which "matches" a catch-all route for no reason at all.
#[test]
fn an_all_slot_external_path_does_not_shadow_a_catch_all_route() {
    let external = vec![consume(
        Some("GET https://api.vendor.com/{}"),
        "fe",
        "Ctx.tsx",
        10,
    )];
    let provides = vec![provide("GET /{}", "be", "server.ts", 20)];
    assert!(external_shadow_internal_findings(&external, &provides).is_empty());
}

/// DOCUMENTED RESIDUAL — asserted as a false negative on purpose. A hardcoded internal host at the
/// root IS the shape this rule is for, and the contentless-path gate silences it, because the
/// matcher's evidence (`GET /` == `GET /`) is byte-identical to the third-party case above. If a
/// host-ownership axis ever arrives, this test is where the trade-off gets re-decided.
#[test]
fn a_hardcoded_internal_host_at_the_root_is_the_documented_residual() {
    let external = vec![consume(
        Some("GET https://app.internal.example.com/"),
        "fe",
        "Ctx.tsx",
        10,
    )];
    let provides = vec![provide("GET /", "be", "Api.java", 20)];
    assert!(external_shadow_internal_findings(&external, &provides).is_empty());
}

/// The gate is exactly "zero literal segments" — ONE literal segment is enough evidence to keep the
/// rule firing, so the veto cannot creep into ordinary shallow routes.
#[test]
fn one_literal_segment_is_enough_and_still_fires() {
    let external = vec![consume(
        Some("GET https://app.internal.example.com/health"),
        "fe",
        "Ctx.tsx",
        10,
    )];
    let provides = vec![provide("GET /health", "be", "Api.java", 20)];
    let out = external_shadow_internal_findings(&external, &provides);
    assert_eq!(out.len(), 1);
    assert_eq!(
        out[0].data.as_ref().unwrap()["normalizedKey"],
        "GET /health"
    );
}

#[test]
fn determinism_multiple_findings_sorted_by_file_then_line() {
    let external = vec![
        consume(
            Some("GET https://app.internal.example.com/api/orders"),
            "fe",
            "Z.tsx",
            1,
        ),
        consume(
            Some("GET https://app.internal.example.com/api/users"),
            "fe",
            "A.tsx",
            5,
        ),
    ];
    let provides = vec![
        provide("GET /api/orders", "be", "Api.java", 1),
        provide("GET /api/users", "be", "Api.java", 2),
    ];
    let out = external_shadow_internal_findings(&external, &provides);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].file, "A.tsx");
    assert_eq!(out[1].file, "Z.tsx");
}
