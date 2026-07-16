use super::*;

#[allow(clippy::too_many_arguments)]
fn record(
    consume_source: &str,
    consume_key: &str,
    consume_file: &str,
    consume_line: u32,
    provide_source: &str,
    provide_key: &str,
    prefix: &str,
    consume_missing_prefix: bool,
) -> PrefixNearMissRecord {
    PrefixNearMissRecord {
        consume_source: consume_source.to_string(),
        consume_key: consume_key.to_string(),
        consume_file: consume_file.to_string(),
        consume_line,
        provide_source: provide_source.to_string(),
        provide_key: provide_key.to_string(),
        prefix: prefix.to_string(),
        consume_missing_prefix,
    }
}

#[test]
fn fires_at_threshold_not_below() {
    let records = vec![
        record(
            "fe",
            "GET /articles",
            "Api.tsx",
            10,
            "be",
            "GET /api/articles",
            "/api",
            true,
        ),
        record(
            "fe",
            "GET /comments",
            "Api.tsx",
            20,
            "be",
            "GET /api/comments",
            "/api",
            true,
        ),
        record(
            "fe",
            "GET /users",
            "Api.tsx",
            30,
            "be",
            "GET /api/users",
            "/api",
            true,
        ),
    ];
    let out = prefix_drift_findings(&records);
    assert_eq!(out.findings.len(), 1);
    let f = &out.findings[0];
    assert_eq!(f.rule_id, "cross-layer/prefix-drift");
    assert_eq!(f.severity, Severity::Info);
    assert_eq!(f.data.as_ref().unwrap()["routeCount"], 3);
    assert!(f.message.contains("/api"));
    assert!(f.message.contains("missing"));
    // Regression pin for the disable-hint convention (also satisfies the engine's file-level
    // `native_rule_files_that_build_findings_mention_disabled_rules` contract for this split-out
    // test file, whose `Finding` fixtures would otherwise trip the grep).
    assert!(f.message.contains("disabled_rules"));
    assert_eq!(out.subsumed.len(), 3);

    // Below threshold (only 2 records): must not fire.
    let below = prefix_drift_findings(&records[..2]);
    assert!(below.findings.is_empty());
    assert!(below.subsumed.is_empty());
}

#[test]
fn groups_are_separated_by_provide_source_prefix_and_direction() {
    // Same consume_source/provide_source, but different prefixes ("/api" vs "/v1") and one differing
    // direction — none of these subgroups reach 3, so nothing fires even though the total is 3+ records.
    let records = vec![
        record(
            "fe",
            "GET /articles",
            "Api.tsx",
            10,
            "be",
            "GET /api/articles",
            "/api",
            true,
        ),
        record(
            "fe",
            "GET /comments",
            "Api.tsx",
            20,
            "be",
            "GET /v1/comments",
            "/v1",
            true,
        ),
        record(
            "fe",
            "GET /api/users",
            "Api.tsx",
            30,
            "be",
            "GET /users",
            "/api",
            false,
        ),
    ];
    let out = prefix_drift_findings(&records);
    assert!(out.findings.is_empty());
    assert!(out.subsumed.is_empty());

    // Bump each subgroup to 3 and confirm each fires independently.
    let mut bigger = records.clone();
    bigger.push(record(
        "fe",
        "GET /widgets",
        "Api.tsx",
        40,
        "be",
        "GET /api/widgets",
        "/api",
        true,
    ));
    bigger.push(record(
        "fe",
        "GET /gadgets",
        "Api.tsx",
        50,
        "be",
        "GET /api/gadgets",
        "/api",
        true,
    ));
    let out2 = prefix_drift_findings(&bigger);
    assert_eq!(out2.findings.len(), 1);
    assert_eq!(out2.findings[0].data.as_ref().unwrap()["prefix"], "/api");
    assert_eq!(
        out2.findings[0].data.as_ref().unwrap()["consumeMissingPrefix"],
        true
    );
}

#[test]
fn extra_prefix_direction_wording() {
    let records = vec![
        record(
            "fe",
            "GET /api/articles",
            "Api.tsx",
            10,
            "be",
            "GET /articles",
            "/api",
            false,
        ),
        record(
            "fe",
            "GET /api/comments",
            "Api.tsx",
            20,
            "be",
            "GET /comments",
            "/api",
            false,
        ),
        record(
            "fe",
            "GET /api/users",
            "Api.tsx",
            30,
            "be",
            "GET /users",
            "/api",
            false,
        ),
    ];
    let out = prefix_drift_findings(&records);
    assert_eq!(out.findings.len(), 1);
    assert!(out.findings[0].message.contains("extra"));
    assert!(out.findings[0].message.contains("remove"));
}

fn near_miss_finding(file: &str, line: u32, source: &str, key: &str, msg: &str) -> Finding {
    Finding {
        rule_id: "cross-layer/route-near-miss".to_string(),
        severity: Severity::Info,
        file: file.to_string(),
        line,
        message: msg.to_string(),
        data: Some(serde_json::json!({"consumeSource": source, "consumeKey": key})),
    }
}

#[test]
fn retain_non_subsumed_drops_only_subsumed() {
    let subsumed_finding = near_miss_finding("Api.tsx", 10, "fe", "GET /articles", "subsumed");
    let kept_finding = near_miss_finding("Api.tsx", 999, "fe", "GET /comments", "kept");
    let no_data_finding = Finding {
        rule_id: "cross-layer/route-near-miss".to_string(),
        severity: Severity::Info,
        file: "Api.tsx".to_string(),
        line: 10,
        message: "no data keys".to_string(),
        data: None,
    };

    let mut subsumed = std::collections::BTreeSet::new();
    subsumed.insert((
        "fe".to_string(),
        "Api.tsx".to_string(),
        10,
        "GET /articles".to_string(),
    ));

    let out = retain_non_subsumed(
        vec![
            subsumed_finding.clone(),
            kept_finding.clone(),
            no_data_finding.clone(),
        ],
        &subsumed,
    );
    assert_eq!(out.len(), 2);
    assert!(out.iter().any(|f| f.message == "kept"));
    assert!(out.iter().any(|f| f.message == "no data keys"));
    assert!(!out.iter().any(|f| f.message == "subsumed"));
}

#[test]
fn retain_non_subsumed_keeps_a_second_consume_on_the_same_line() {
    // Two distinct consumes on the SAME source+file+line (e.g. `Promise.all([get('/articles'),
    // get('/Articles')])`): one folded into the aggregate (prefix), one an independent case near-miss.
    // Keying on (source, file, line) alone would drop BOTH — the key MUST include consumeKey so only the
    // folded consume is suppressed and the other survives (output-philosophy §0/§1 — no silent drop).
    let folded = near_miss_finding("Api.tsx", 10, "fe", "GET /articles", "folded-prefix");
    let independent = near_miss_finding("Api.tsx", 10, "fe", "GET /Articles", "independent-case");

    let mut subsumed = std::collections::BTreeSet::new();
    subsumed.insert((
        "fe".to_string(),
        "Api.tsx".to_string(),
        10,
        "GET /articles".to_string(),
    ));

    let out = retain_non_subsumed(vec![folded, independent], &subsumed);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].message, "independent-case");
}

#[test]
fn deterministic_output() {
    let records = vec![
        record(
            "fe",
            "GET /articles",
            "Api.tsx",
            10,
            "be",
            "GET /api/articles",
            "/api",
            true,
        ),
        record(
            "fe",
            "GET /comments",
            "Api.tsx",
            20,
            "be",
            "GET /api/comments",
            "/api",
            true,
        ),
        record(
            "fe",
            "GET /users",
            "Api.tsx",
            30,
            "be",
            "GET /api/users",
            "/api",
            true,
        ),
    ];
    let out1 = prefix_drift_findings(&records);
    let out2 = prefix_drift_findings(&records);
    assert_eq!(
        out1.findings.iter().map(|f| &f.message).collect::<Vec<_>>(),
        out2.findings.iter().map(|f| &f.message).collect::<Vec<_>>()
    );
}
