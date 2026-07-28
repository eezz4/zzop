//! Unit tests for the tool-output shaping helpers (`shape_findings`/`shape_list`/`FindingFilters`).
//! `bucket_keys`'s own tests live beside it in `bucket_keys.rs`.

use super::*;

fn finding(rule: &str, severity: &str, idx: usize) -> serde_json::Value {
    serde_json::json!({ "ruleId": rule, "severity": severity, "path": format!("f{idx}.ts") })
}

#[test]
fn counts_stay_full_while_filter_narrows_shown() {
    let findings = vec![
        finding("a", "info", 0),
        finding("b", "critical", 1),
        finding("a", "warning", 2),
    ];
    let filters = FindingFilters {
        min_severity: Some("warning".into()),
        rule: None,
        limit: None,
    };
    let shaped = shape_findings(&findings, &filters);
    assert_eq!(shaped["total"], 3);
    assert_eq!(shaped["bySeverity"]["info"], 1); // full-set counts, not filtered
    let shown = shaped["shown"].as_array().unwrap();
    assert_eq!(shown.len(), 2);
    // severity-desc ordering: critical before warning
    assert_eq!(shown[0]["severity"], "critical");
    assert!(shaped.get("truncated").is_none()); // complete list => no truncation key
}

#[test]
fn truncation_is_disclosed_never_silent() {
    let findings: Vec<_> = (0..5).map(|i| finding("r", "info", i)).collect();
    let filters = FindingFilters {
        min_severity: None,
        rule: None,
        limit: Some(2),
    };
    let shaped = shape_findings(&findings, &filters);
    assert_eq!(shaped["shown"].as_array().unwrap().len(), 2);
    assert_eq!(shaped["truncated"]["shown"], 2);
    assert_eq!(shaped["truncated"]["totalMatching"], 5);
    // `shape_findings` is the ONE surface where `severity`/`rule`/`limit` genuinely move the cap, so
    // naming `limit` here is a remedy that works — see the `shape_list` pin below for why the other
    // capped lists must not copy this wording.
    assert!(shaped["truncated"]["hint"]
        .as_str()
        .unwrap()
        .contains("limit"));
}

/// Seals the seam that made an inert remedy possible: `shape_list` used to hardcode
/// `shape_findings`' hint, so `edgesTruncated`/`degradedTruncated` told callers to "raise limit"
/// when no tool argument moves either cap. The hint is now the CALLER's, echoed verbatim — a shared
/// default cannot come back without deleting this pin.
#[test]
fn shape_list_echoes_the_callers_hint_verbatim_instead_of_a_shared_default() {
    let items: Vec<serde_json::Value> = (0..5).map(|i| serde_json::json!({ "i": i })).collect();
    let hint = "`buckets.edges` carries the full, uncapped count";
    let (shown, truncated) = shape_list(&items, 2, hint);
    assert_eq!(shown.len(), 2);
    let truncated = truncated.expect("a capped list always discloses");
    assert_eq!(truncated["shown"], 2);
    assert_eq!(truncated["totalMatching"], 5);
    assert_eq!(truncated["hint"], hint);
    // A list that fits is not "truncated" — disclosure only when something was actually dropped.
    assert!(shape_list(&items, 5, hint).1.is_none());
}

#[test]
fn deterministic_order_same_input_same_output() {
    let findings = vec![
        finding("a", "warning", 0),
        finding("b", "warning", 1),
        finding("c", "critical", 2),
    ];
    let filters = FindingFilters {
        min_severity: None,
        rule: None,
        limit: None,
    };
    let one = serde_json::to_string(&shape_findings(&findings, &filters)).unwrap();
    let two = serde_json::to_string(&shape_findings(&findings, &filters)).unwrap();
    assert_eq!(one, two);
    let shaped = shape_findings(&findings, &filters);
    let shown = shaped["shown"].as_array().unwrap();
    // critical first, then the two warnings in original order (stable tiebreak).
    assert_eq!(shown[0]["ruleId"], "c");
    assert_eq!(shown[1]["ruleId"], "a");
    assert_eq!(shown[2]["ruleId"], "b");
}

#[test]
fn unknown_severity_argument_is_a_named_error() {
    let args = serde_json::json!({ "severity": "sev-nope" });
    let err = FindingFilters::from_args(Some(&args)).unwrap_err();
    assert!(err.contains("sev-nope"));
    assert!(err.contains("critical"));
}

/// Boundary-value torture round: a `severity` NUMBER used to silently fall through `as_str()` and
/// drop the filter entirely (indistinguishable from "no severity argument at all") instead of hitting
/// the same "unknown severity" rejection a bad STRING gets.
#[test]
fn a_non_string_severity_hits_the_same_rejection_path_as_an_unknown_string() {
    let args = serde_json::json!({ "severity": 5 });
    let err = FindingFilters::from_args(Some(&args)).unwrap_err();
    assert!(err.contains("unknown severity 5"), "got: {err}");
    assert!(err.contains("critical"));
}

/// `limit: 0` is legal — "counts only, no findings listed" — and must NOT be treated as "not
/// provided" (which would fall back to the default cap of 50).
#[test]
fn limit_zero_is_legal_and_distinct_from_absent() {
    let args = serde_json::json!({ "limit": 0 });
    let filters = FindingFilters::from_args(Some(&args)).unwrap();
    assert_eq!(filters.limit, Some(0));
}

/// Boundary-value torture round: `-1`, `1001`, `999999`, `"50"` (string), and `3.7` (float) all used
/// to be silently accepted and behave as "no cap" (`as_u64()` returning `None` on every one of them
/// was treated as "argument omitted"). Every one must now be a named rejection instead.
#[test]
fn out_of_range_and_wrong_type_limit_values_are_all_named_rejections() {
    for bad in [
        serde_json::json!(-1),
        serde_json::json!(1001),
        serde_json::json!(999_999),
        serde_json::json!("50"),
        serde_json::json!(3.7),
    ] {
        let args = serde_json::json!({ "limit": bad });
        let err = match FindingFilters::from_args(Some(&args)) {
            Err(e) => e,
            Ok(_) => panic!("limit {bad} must be rejected, got Ok"),
        };
        assert!(
            err.contains("limit must be an integer between 0 and 1000"),
            "limit {bad}: got {err}"
        );
    }
}

#[test]
fn a_valid_in_range_limit_is_accepted() {
    let args = serde_json::json!({ "limit": 500 });
    let filters = FindingFilters::from_args(Some(&args)).unwrap();
    assert_eq!(filters.limit, Some(500));
}

/// The wire-neutral constructor is what a non-JSON host (the `zzop` CLI) reaches for: already-parsed
/// values in, no MCP `tools/call` object fabricated on the way. `None`s are the unfiltered default view.
#[test]
fn the_wire_neutral_constructor_takes_parsed_values_with_no_json_in_sight() {
    let unfiltered = FindingFilters::new(None, None, None).expect("no filters always construct");
    assert_eq!(unfiltered.min_severity, None);
    assert_eq!(unfiltered.rule, None);
    assert_eq!(unfiltered.limit, None);

    let filtered = FindingFilters::new(Some("warning"), Some("sql/nplus1"), Some(0))
        .expect("valid values construct");
    assert_eq!(filtered.min_severity.as_deref(), Some("warning"));
    assert_eq!(filtered.rule.as_deref(), Some("sql/nplus1"));
    // `0` survives as `Some(0)` here for the same reason it does through `from_args`: "counts only"
    // is a real request, never "not provided".
    assert_eq!(filtered.limit, Some(0));
}

/// The two constructors share ONE validation vocabulary — a wire-neutral caller must not be the lenient
/// door into the shaping layer. Both rejections are asserted to carry the SAME message the JSON lane
/// produces, so a future divergence fails here rather than shipping two answers.
#[test]
fn the_wire_neutral_constructor_rejects_exactly_what_the_json_lane_rejects() {
    let neutral_severity = FindingFilters::new(Some("sev-nope"), None, None).unwrap_err();
    let json_severity =
        FindingFilters::from_args(Some(&serde_json::json!({ "severity": "sev-nope" })))
            .unwrap_err();
    assert_eq!(neutral_severity, json_severity);
    assert!(
        neutral_severity.contains("critical"),
        "got: {neutral_severity}"
    );

    let neutral_limit = FindingFilters::new(None, None, Some(1001)).unwrap_err();
    let json_limit =
        FindingFilters::from_args(Some(&serde_json::json!({ "limit": 1001 }))).unwrap_err();
    assert_eq!(neutral_limit, json_limit);
    assert!(
        neutral_limit.contains("limit must be an integer between 0 and 1000"),
        "got: {neutral_limit}"
    );

    // An in-range limit at the exact cap is accepted by both.
    assert_eq!(
        FindingFilters::new(None, None, Some(1000)).unwrap().limit,
        Some(1000)
    );
}

/// A `rule` NUMBER used to fall through `as_str()` and silently drop the filter (indistinguishable
/// from "no `rule` argument") — now a named type error, same class as `path`/`configPath`/`pattern`.
#[test]
fn a_non_string_rule_argument_is_a_named_type_error() {
    let args = serde_json::json!({ "rule": 42 });
    let err = FindingFilters::from_args(Some(&args)).unwrap_err();
    assert_eq!(err, "`rule` must be a string (got 42)");
}

#[test]
fn rule_filter_is_exact() {
    let findings = vec![finding("a", "info", 0), finding("ab", "info", 1)];
    let filters = FindingFilters {
        min_severity: None,
        rule: Some("a".into()),
        limit: None,
    };
    let shaped = shape_findings(&findings, &filters);
    assert_eq!(shaped["shown"].as_array().unwrap().len(), 1);
    assert_eq!(shaped["shown"][0]["ruleId"], "a");
}

#[test]
fn zero_match_rule_filter_for_a_nonexistent_rule_id_gets_a_disclosure_note() {
    let findings = vec![finding("a", "info", 0), finding("b", "warning", 1)];
    let filters = FindingFilters {
        min_severity: None,
        rule: Some("typo-d-rule-id".into()),
        limit: None,
    };
    let shaped = shape_findings(&findings, &filters);
    assert_eq!(shaped["shown"].as_array().unwrap().len(), 0);
    let note = shaped["note"].as_str().expect("note must be present");
    assert!(note.contains("typo-d-rule-id"));
    assert!(note.contains("byRule"));
    // Discoverability chain: the note must name the `rule-catalog` contract resource by name, not
    // just say "the catalog" with no pointer to where it lives (D10: the catalog was not previously
    // served over MCP at all).
    assert!(note.contains("rule-catalog"));
}

#[test]
fn zero_match_rule_filter_for_a_real_rule_with_no_findings_gets_no_note() {
    // Rule "a" fired elsewhere in the run (present in byRule) but every finding got filtered out by
    // severity — a real, quiet rule, not a bad id. Must NOT get the nonexistent-id disclosure.
    let findings = vec![finding("a", "info", 0)];
    let filters = FindingFilters {
        min_severity: Some("critical".into()),
        rule: Some("a".into()),
        limit: None,
    };
    let shaped = shape_findings(&findings, &filters);
    assert_eq!(shaped["shown"].as_array().unwrap().len(), 0);
    assert!(shaped.get("note").is_none());
}

#[test]
fn a_rule_filter_that_actually_matches_gets_no_note() {
    let findings = vec![finding("a", "info", 0)];
    let filters = FindingFilters {
        min_severity: None,
        rule: Some("a".into()),
        limit: None,
    };
    let shaped = shape_findings(&findings, &filters);
    assert_eq!(shaped["shown"].as_array().unwrap().len(), 1);
    assert!(shaped.get("note").is_none());
}
