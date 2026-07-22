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

#[test]
fn same_path_two_hosts_is_flagged_anchored_at_first_site() {
    let external = vec![
        consume(
            Some("GET https://staging.vendor.com/v1/widgets"),
            "fe",
            "B.tsx",
            5,
        ),
        consume(
            Some("GET https://prod.vendor.com/v1/widgets"),
            "be",
            "A.java",
            1,
        ),
    ];
    let out = external_base_url_drift_findings(&external);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].rule_id, "cross-layer/external-base-url-drift");
    assert_eq!(out[0].severity, Severity::Info);
    assert_eq!(out[0].file, "A.java");
    assert_eq!(out[0].line, 1);
    assert!(out[0].message.contains("/v1/widgets"));
    assert!(out[0].message.contains("disabled_rules"));
    let data = out[0].data.as_ref().unwrap();
    assert_eq!(data["path"], "/v1/widgets");
    assert_eq!(
        data["hosts"],
        serde_json::json!(["prod.vendor.com", "staging.vendor.com"])
    );
    assert_eq!(data["siteCount"], 2);
}

#[test]
fn test_fixture_file_does_not_count_toward_the_host_threshold() {
    // The test-fixture consume must not push this path over the 2-distinct-host threshold.
    let external = vec![
        consume(
            Some("GET https://staging.vendor.com/v1/widgets"),
            "fe",
            "src/__tests__/B.test.tsx",
            5,
        ),
        consume(
            Some("GET https://prod.vendor.com/v1/widgets"),
            "be",
            "A.java",
            1,
        ),
    ];
    assert!(external_base_url_drift_findings(&external).is_empty());
}

#[test]
fn same_path_single_host_is_not_flagged() {
    let external = vec![
        consume(
            Some("GET https://prod.vendor.com/v1/widgets"),
            "fe",
            "A.tsx",
            1,
        ),
        consume(
            Some("POST https://prod.vendor.com/v1/widgets"),
            "be",
            "B.java",
            2,
        ),
    ];
    assert!(external_base_url_drift_findings(&external).is_empty());
}

#[test]
fn single_segment_path_is_never_flagged_even_with_two_hosts() {
    let external = vec![
        consume(Some("GET https://a.vendor.com/api"), "fe", "A.tsx", 1),
        consume(Some("GET https://b.vendor.com/api"), "be", "B.java", 2),
    ];
    assert!(external_base_url_drift_findings(&external).is_empty());
}

#[test]
fn port_only_difference_between_hosts_is_flagged() {
    let external = vec![
        consume(
            Some("GET https://vendor.com:8443/v1/widgets"),
            "fe",
            "B.tsx",
            5,
        ),
        consume(Some("GET https://vendor.com/v1/widgets"), "be", "A.java", 1),
    ];
    let out = external_base_url_drift_findings(&external);
    assert_eq!(out.len(), 1);
    let data = out[0].data.as_ref().unwrap();
    assert_eq!(
        data["hosts"],
        serde_json::json!(["vendor.com", "vendor.com:8443"])
    );
}

#[test]
fn related_subdomains_of_the_same_registrable_domain_are_flagged() {
    // Same registrable domain (`zoom.us`) — one logical service reached via two hosts, should fire.
    let external = vec![
        consume(
            Some("POST https://api.zoom.us/oauth/token"),
            "fe",
            "B.tsx",
            5,
        ),
        consume(Some("POST https://zoom.us/oauth/token"), "be", "A.java", 1),
    ];
    let out = external_base_url_drift_findings(&external);
    assert_eq!(out.len(), 1);
    let data = out[0].data.as_ref().unwrap();
    assert_eq!(data["hosts"], serde_json::json!(["api.zoom.us", "zoom.us"]));
}

#[test]
fn unrelated_vendors_sharing_a_conventional_path_are_not_flagged() {
    // Different registrable domains (`pipedrive.com` != `dub.co`) — unrelated vendors, not drift.
    let external = vec![
        consume(
            Some("POST https://oauth.pipedrive.com/oauth/token"),
            "fe",
            "B.tsx",
            5,
        ),
        consume(
            Some("POST https://api.dub.co/oauth/token"),
            "be",
            "A.java",
            1,
        ),
    ];
    assert!(external_base_url_drift_findings(&external).is_empty());
}

#[test]
fn mixed_group_fires_with_only_the_related_pair_listed() {
    // api.dub.co shares no registrable domain with anyone — it must be dropped, not just tolerated.
    let external = vec![
        consume(
            Some("POST https://api.zoom.us/oauth/token"),
            "fe",
            "B.tsx",
            5,
        ),
        consume(Some("POST https://zoom.us/oauth/token"), "be", "A.java", 1),
        consume(
            Some("POST https://api.dub.co/oauth/token"),
            "other",
            "C.rb",
            9,
        ),
    ];
    let out = external_base_url_drift_findings(&external);
    assert_eq!(out.len(), 1);
    let data = out[0].data.as_ref().unwrap();
    assert_eq!(data["hosts"], serde_json::json!(["api.zoom.us", "zoom.us"]));
    assert_eq!(data["siteCount"], 2);
    assert!(!out[0].message.contains("dub.co"));
}

#[test]
fn determinism_multiple_findings_sorted_by_file_then_line() {
    let external = vec![
        consume(Some("GET https://z1.vendor.com/v1/z"), "fe", "Z.tsx", 5),
        consume(Some("GET https://z2.vendor.com/v1/z"), "be", "Y.java", 1),
        consume(Some("GET https://a1.vendor.com/v1/a"), "fe", "A.tsx", 5),
        consume(Some("GET https://a2.vendor.com/v1/a"), "be", "B.java", 1),
    ];
    let out = external_base_url_drift_findings(&external);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].file, "B.java");
    assert_eq!(out[1].file, "Y.java");
}
