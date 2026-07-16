use super::*;

fn consume(kind: &str, key: Option<&str>, source: &str, file: &str, line: u32) -> TaggedConsume {
    TaggedConsume {
        source: source.to_string(),
        consume: zzop_core::IoConsume {
            client: None,
            body: None,
            kind: kind.to_string(),
            key: key.map(str::to_string),
            file: file.to_string(),
            line,
            raw: None,
            method: None,
        },
    }
}

#[test]
fn versioned_and_versionless_paths_on_the_same_host_are_flagged_anchored_at_versionless() {
    let external = vec![
        consume(
            "http",
            Some("GET https://api.vendor.com/v1/users"),
            "fe",
            "Users.ts",
            40,
        ),
        consume(
            "http",
            Some("GET https://api.vendor.com/accounts"),
            "fe",
            "Accounts.ts",
            7,
        ),
    ];
    let out = external_version_inconsistent_findings(&external);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].rule_id, "cross-layer/external-version-inconsistent");
    assert_eq!(out[0].severity, Severity::Info);
    assert_eq!(out[0].file, "Accounts.ts");
    assert_eq!(out[0].line, 7);
    assert!(out[0].message.contains("api.vendor.com"));
    assert!(out[0].message.contains("disabled_rules"));
    assert!(out[0].message.contains("equally plausible readings"));
    assert!(out[0]
        .message
        .contains("genuinely distinct endpoint family"));
    assert!(out[0].message.contains("Check `api.vendor.com`'s API docs"));
    let data = out[0].data.as_ref().unwrap();
    assert_eq!(data["versionedPathCount"], 1);
    assert_eq!(data["versionlessPathCount"], 1);
}

#[test]
fn only_versioned_paths_on_a_host_is_not_flagged() {
    let external = vec![
        consume(
            "http",
            Some("GET https://api.vendor.com/v1/users"),
            "fe",
            "Users.ts",
            40,
        ),
        consume(
            "http",
            Some("GET https://api.vendor.com/v2/accounts"),
            "fe",
            "Accounts.ts",
            7,
        ),
    ];
    assert!(external_version_inconsistent_findings(&external).is_empty());
}

#[test]
fn only_versionless_paths_on_a_host_is_not_flagged() {
    let external = vec![
        consume(
            "http",
            Some("GET https://api.vendor.com/users"),
            "fe",
            "Users.ts",
            40,
        ),
        consume(
            "http",
            Some("GET https://api.vendor.com/accounts"),
            "fe",
            "Accounts.ts",
            7,
        ),
    ];
    assert!(external_version_inconsistent_findings(&external).is_empty());
}

#[test]
fn bare_root_path_is_dropped_from_the_versionless_side() {
    // Only a "/" consume plus a versioned one — root pins nothing, so this must not fire.
    let external = vec![
        consume(
            "http",
            Some("GET https://api.vendor.com/v1/users"),
            "fe",
            "Users.ts",
            40,
        ),
        consume(
            "http",
            Some("GET https://api.vendor.com/"),
            "fe",
            "Root.ts",
            1,
        ),
    ];
    assert!(external_version_inconsistent_findings(&external).is_empty());
}

#[test]
fn each_host_is_classified_independently() {
    // host A: versioned only, host B: versionless only — neither alone qualifies.
    let external = vec![
        consume(
            "http",
            Some("GET https://a.vendor.com/v1/users"),
            "fe",
            "A.ts",
            1,
        ),
        consume(
            "http",
            Some("GET https://b.vendor.com/users"),
            "fe",
            "B.ts",
            1,
        ),
    ];
    assert!(external_version_inconsistent_findings(&external).is_empty());
}

#[test]
fn consume_in_a_test_fixture_file_does_not_count_toward_the_host_classification() {
    // Versionless consume is only in a test fixture — it must not count toward "both" territory.
    let external = vec![
        consume(
            "http",
            Some("GET https://api.vendor.com/v1/users"),
            "fe",
            "Users.ts",
            40,
        ),
        consume(
            "http",
            Some("GET https://api.vendor.com/accounts"),
            "fe",
            "src/__tests__/Accounts.test.ts",
            7,
        ),
    ];
    assert!(external_version_inconsistent_findings(&external).is_empty());
}

#[test]
fn non_http_kind_is_ignored() {
    let external = vec![
        consume(
            "queue",
            Some("GET https://api.vendor.com/v1/users"),
            "fe",
            "Users.ts",
            40,
        ),
        consume(
            "queue",
            Some("GET https://api.vendor.com/accounts"),
            "fe",
            "Accounts.ts",
            7,
        ),
    ];
    assert!(external_version_inconsistent_findings(&external).is_empty());
}

#[test]
fn findings_are_sorted_deterministically_by_file_then_line() {
    let external = vec![
        // host b: fires, anchored at B-versionless.ts:9
        consume(
            "http",
            Some("GET https://b.vendor.com/v1/x"),
            "fe",
            "B-versioned.ts",
            2,
        ),
        consume(
            "http",
            Some("GET https://b.vendor.com/x"),
            "fe",
            "B-versionless.ts",
            9,
        ),
        // host a: fires, anchored at A-versionless.ts:3
        consume(
            "http",
            Some("GET https://a.vendor.com/v1/y"),
            "fe",
            "A-versioned.ts",
            50,
        ),
        consume(
            "http",
            Some("GET https://a.vendor.com/y"),
            "fe",
            "A-versionless.ts",
            3,
        ),
    ];
    let out = external_version_inconsistent_findings(&external);
    assert_eq!(out.len(), 2);
    assert_eq!((out[0].file.as_str(), out[0].line), ("A-versionless.ts", 3));
    assert_eq!((out[1].file.as_str(), out[1].line), ("B-versionless.ts", 9));
}
