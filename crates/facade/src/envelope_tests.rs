//! Unit tests for the envelope entry points (`crate::envelope`).

use crate::test_support::tiny_envelope_json;
use crate::{analyze_envelope_json, validate_envelope_only_json};

#[test]
fn analyze_envelope_json_suppressions_drop_a_finding() {
    // Same suppression path as `analyze`, exercised through the envelope entry point
    // (`analyze_envelope_json` -> `base_engine_config`). Two files importing each other form a
    // cycle -> a `circular` finding.
    let envelope = r#"{
        "format": "zzop-normalized-ast",
        "version": 1,
        "parser": "test/1",
        "source": "legacy",
        "files": [
            {"path": "a.ts", "loc": 2, "imports": {"b": {"specifier": "b.ts", "original": "default"}}},
            {"path": "b.ts", "loc": 2, "imports": {"a": {"specifier": "a.ts", "original": "default"}}}
        ]
    }"#;
    let baseline = analyze_envelope_json(envelope, r#"{"sourceId": "legacy"}"#)
        .expect("analyze_envelope_json should succeed");
    let baseline_value: serde_json::Value = serde_json::from_str(&baseline).unwrap();
    assert!(
        baseline_value["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["ruleId"] == "circular"),
        "fixture must produce a circular finding without suppression, got: {baseline_value}"
    );

    let suppressed = analyze_envelope_json(
        envelope,
        r#"{"sourceId": "legacy", "suppressions": [{"rule": "circular"}]}"#,
    )
    .expect("analyze_envelope_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&suppressed).unwrap();
    assert!(
        !value["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["ruleId"] == "circular"),
        "suppressions must drop the circular finding in envelope mode, got: {value}"
    );
}

/// Config-diagnostics parity with the `analyze` path (`analyze_tests.rs`'s twin test): a typo'd
/// `disabledRules`/`severityOverrides` entry must land in `configWarnings` in envelope mode too, not
/// `warnings` — the envelope pipeline computes this via the same `run_diagnostics` call the tree
/// pipeline uses.
#[test]
fn analyze_envelope_json_unknown_rule_overrides_land_in_config_warnings_not_warnings() {
    let config = r#"{"sourceId": "legacy", "severityOverrides": {"n-plus-one-typo": "critical"}}"#;
    let out = analyze_envelope_json(&tiny_envelope_json(), config)
        .expect("analyze_envelope_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    let config_warnings: Vec<&str> = value["configWarnings"]
        .as_array()
        .expect("configWarnings array")
        .iter()
        .filter_map(|w| w.as_str())
        .collect();
    assert!(
        config_warnings
            .iter()
            .any(|w| w.contains("severity overrides") && w.contains("n-plus-one-typo")),
        "expected the unknown-severity-override-id self-report in configWarnings, got: {config_warnings:?}"
    );
    let warnings: Vec<&str> = value["warnings"]
        .as_array()
        .expect("warnings array")
        .iter()
        .filter_map(|w| w.as_str())
        .collect();
    assert!(
        !warnings
            .iter()
            .any(|w| w.contains("matching no known rule id")),
        "must NOT duplicate into warnings, got: {warnings:?}"
    );
}

#[test]
fn analyze_envelope_json_round_trips_a_tiny_envelope() {
    let config = r#"{"sourceId": "legacy"}"#;
    let out = analyze_envelope_json(&tiny_envelope_json(), config)
        .expect("analyze_envelope_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    assert_eq!(value["fileCount"], 1);
    let provides = value["ir"]["io"]["provides"]
        .as_array()
        .expect("provides array");
    assert_eq!(provides.len(), 1);
    assert_eq!(provides[0]["key"], "GET /legacy/user.jsp");
}

// --- Deployment-topology mounts over the WIRE path (`docs/NORMALIZED_AST.md`'s "apply uniformly to
// Mode A envelopes and natively-parsed trees alike" promise). The engine's own envelope-mode
// `apply_config_mounts` call is covered by `crates/engine`'s tests with a direct `EngineConfig`;
// what these pin is the REQUEST plumbing — `mountedAt`/`mounts` deserializing out of the
// `analyzeEnvelope` config JSON and reaching that engine call through `analyze_envelope_json`,
// which used to silently drop them (the Mode-A envelope mounts wire gap).

#[test]
fn analyze_envelope_json_mounted_at_rewrites_http_provide_keys_over_the_wire() {
    let config = r#"{"sourceId": "legacy", "mountedAt": "/gateway"}"#;
    let out = analyze_envelope_json(&tiny_envelope_json(), config)
        .expect("analyze_envelope_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    let provides = value["ir"]["io"]["provides"]
        .as_array()
        .expect("provides array");
    assert_eq!(provides.len(), 1);
    assert_eq!(
        provides[0]["key"], "GET /gateway/legacy/user.jsp",
        "mountedAt sent over the analyzeEnvelope config wire must rewrite the http provide key, got: {value}"
    );
}

#[test]
fn analyze_envelope_json_mounts_longer_dir_beats_mounted_at_over_the_wire() {
    // Same fold order as the tree path (`fold_mounts`): mounts[] entries first, `mountedAt` as the
    // implicit `dir: ""` entry LAST — the engine's longest-`dir`-wins rule then picks the explicit
    // entry for the file it covers (`legacy/UserController.jsp` under dir `legacy`).
    let config = r#"{
        "sourceId": "legacy",
        "mountedAt": "/gateway",
        "mounts": [ { "dir": "legacy", "at": "/svc" } ]
    }"#;
    let out = analyze_envelope_json(&tiny_envelope_json(), config)
        .expect("analyze_envelope_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    let provides = value["ir"]["io"]["provides"]
        .as_array()
        .expect("provides array");
    assert_eq!(
        provides[0]["key"], "GET /svc/legacy/user.jsp",
        "the longer-dir mounts[] entry must win over the whole-tree mountedAt shorthand, got: {value}"
    );
}

#[test]
fn envelope_analyze_request_defaults_mounted_at_and_mounts_to_empty() {
    // Absent keys must round-trip to `None`/empty — the pre-field behavior, byte-for-byte.
    let req: crate::EnvelopeAnalyzeRequest =
        serde_json::from_str(r#"{"sourceId": "legacy"}"#).expect("valid config JSON");
    assert!(req.mounted_at.is_none());
    assert!(req.mounts.is_empty());
}

#[test]
fn analyze_envelope_json_rejects_an_invalid_envelope_without_panicking() {
    let bad_envelope = tiny_envelope_json().replace("zzop-normalized-ast", "bogus-format");
    let err = analyze_envelope_json(&bad_envelope, r#"{"sourceId": "legacy"}"#).unwrap_err();
    assert!(err.contains("invalid analyzeEnvelope() envelope JSON"));
    assert!(err.contains("unknown format"));
}

#[test]
fn analyze_envelope_json_rejects_invalid_config_json() {
    let err = analyze_envelope_json(&tiny_envelope_json(), "not json").unwrap_err();
    assert!(err.contains("invalid analyzeEnvelope() config JSON"));
}

#[test]
fn validate_envelope_only_json_reports_valid_for_a_well_formed_envelope() {
    let out = validate_envelope_only_json(&tiny_envelope_json());
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    assert_eq!(value["valid"], true);
    assert_eq!(value["issues"].as_array().expect("issues array").len(), 0);
}

#[test]
fn validate_envelope_only_json_lists_issues_for_a_broken_envelope() {
    let bad = tiny_envelope_json().replace("zzop-normalized-ast", "bogus-format");
    let out = validate_envelope_only_json(&bad);
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    assert_eq!(value["valid"], false);
    let issues = value["issues"].as_array().expect("issues array");
    assert!(
        issues
            .iter()
            .any(|i| i.as_str().unwrap().contains("unknown format")),
        "expected an 'unknown format' issue, got: {value}"
    );
}

#[test]
fn validate_envelope_only_json_names_an_array_root_instead_of_a_field_type_mismatch() {
    // A blind field test fed a JSON ARRAY as `envelopeJson` and got serde's struct-from-sequence
    // fallback error ("invalid type: integer `1`, expected a string ...") — a field-level message that
    // masks the real problem (the root itself is the wrong shape).
    let out = validate_envelope_only_json("[1,2,3]");
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    assert_eq!(value["valid"], false);
    let issues = value["issues"].as_array().expect("issues array");
    assert_eq!(
        issues,
        &vec![serde_json::json!(
            "expected a JSON object envelope, got an array"
        )],
        "got: {value}"
    );
}

#[test]
fn validate_envelope_only_json_never_fails_on_unparseable_input() {
    let out = validate_envelope_only_json("not json");
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    assert_eq!(value["valid"], false);
    let issues = value["issues"].as_array().expect("issues array");
    assert!(issues
        .iter()
        .any(|i| i.as_str().unwrap().contains("invalid JSON")));
}

/// Seals the wire shape: `hints` is a THIRD, always-present field on every envelope validate reply —
/// valid, invalid and unparseable alike. An authoring surface that omitted it when empty would be
/// indistinguishable from a build with no hint pass at all.
#[test]
fn validate_envelope_only_json_always_carries_a_hints_array() {
    for input in [
        tiny_envelope_json(),
        tiny_envelope_json().replace("zzop-normalized-ast", "bogus-format"),
        "not json".to_string(),
        "[1,2,3]".to_string(),
    ] {
        let out = validate_envelope_only_json(&input);
        let value: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
        assert!(
            value["hints"].is_array(),
            "missing hints array for {input}: {value}"
        );
    }
    // The shipped fixture is hint-clean, not merely valid.
    let value: serde_json::Value =
        serde_json::from_str(&validate_envelope_only_json(&tiny_envelope_json())).unwrap();
    assert_eq!(value["hints"].as_array().unwrap().len(), 0, "{value}");
}

/// THE PIN of the hint axis at the wire: a semantically suspicious but structurally conforming
/// envelope stays `"valid": true` while reporting hints. `valid` (and therefore the
/// `zzop validate-envelope` exit code, and whether `analyze_envelope_json` proceeds) is read off the
/// validity result alone — a hint may never flip it.
#[test]
fn hints_are_reported_without_making_a_valid_envelope_invalid() {
    // Absolute path + a non-normalized `http` provide key: both join with nothing, neither is a v1
    // contract violation.
    let suspicious = tiny_envelope_json()
        .replace(
            "\"path\": \"legacy/UserController.jsp\"",
            "\"path\": \"/srv/legacy/UserController.jsp\"",
        )
        .replace("GET /legacy/user.jsp", "get legacy/user.jsp");
    let out = validate_envelope_only_json(&suspicious);
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    assert_eq!(value["valid"], true, "hints must not reject: {value}");
    assert_eq!(value["issues"].as_array().unwrap().len(), 0, "{value}");
    let hints = value["hints"].as_array().expect("hints array");
    assert_eq!(hints.len(), 2, "{value}");
    assert!(
        hints[0].as_str().unwrap().contains("absolute path"),
        "{value}"
    );
    assert!(
        hints[1].as_str().unwrap().contains("GET /legacy/user.jsp"),
        "the hint names the canonical key to emit instead: {value}"
    );
    // And the analysis entry point still accepts it — the two axes really are independent.
    assert!(analyze_envelope_json(&suspicious, r#"{"sourceId": "legacy"}"#).is_ok());
}

/// Seals the axis split on an INVALID envelope: issues and hints are both reported in one round-trip,
/// so a producer does not have to fix one class, re-run, and only then learn about the other.
#[test]
fn an_invalid_envelope_still_reports_its_hints() {
    let bad = tiny_envelope_json()
        .replace("\"version\": 1", "\"version\": 99")
        .replace(
            "\"path\": \"legacy/UserController.jsp\"",
            "\"path\": \"/srv/x.jsp\"",
        );
    let out = validate_envelope_only_json(&bad);
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    assert_eq!(value["valid"], false);
    assert_eq!(value["issues"].as_array().unwrap().len(), 1, "{value}");
    assert_eq!(value["hints"].as_array().unwrap().len(), 1, "{value}");
}

/// Seals the sibling surface: `validate_rule_pack` has no hint pass, so its reply must NOT grow an
/// always-empty `hints` field — an empty array there would claim a judgment that was never made.
#[test]
fn the_rule_pack_validate_reply_does_not_gain_a_hints_field() {
    let out = crate::validate_rule_pack_json(r#"{"id":"p","rules":[]}"#);
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    assert!(value.get("hints").is_none(), "{value}");
}
