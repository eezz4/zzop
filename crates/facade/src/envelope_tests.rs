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
        "version": "0.27.0",
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

/// `profileRules` over the envelope WIRE path — the request field that deliberately did not exist
/// until the engine's Mode A lane fed the shared timing accumulator (see
/// `EnvelopeAnalyzeRequest::profile_rules`). The engine-side timing content is pinned in
/// `crates/engine/src/envelope/tests/rules_and_diagnostics.rs`; this pins the PLUMBING: the field
/// deserializes, reaches `EngineConfig::profile_rules`, and `ruleTimings` comes back non-null.
#[test]
fn analyze_envelope_json_profile_rules_populates_rule_timings_over_the_wire() {
    let out = analyze_envelope_json(
        &tiny_envelope_json(),
        r#"{"sourceId": "legacy", "profileRules": true}"#,
    )
    .expect("analyze_envelope_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    let timings = value["ruleTimings"]
        .as_array()
        .expect("profiled Mode A run must serialize a ruleTimings array");
    // Non-empty without naming specific rules: the zero-config bundled packs' io-scan rules and the
    // whole-graph native analyses (`circular`/`dead-candidates`) all ride the shared accumulator.
    assert!(!timings.is_empty(), "{value}");

    // Default (field absent) stays the pre-knob behavior: `ruleTimings` serialized as null.
    let off = analyze_envelope_json(&tiny_envelope_json(), r#"{"sourceId": "legacy"}"#)
        .expect("analyze_envelope_json should succeed");
    let off: serde_json::Value = serde_json::from_str(&off).expect("valid JSON");
    assert!(off["ruleTimings"].is_null(), "{off}");
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
    assert!(req.client_base.is_none());
}

/// `clientBase` is the CONSUME-side half of the same promise `mountedAt` keeps: a declaration about
/// where a tree sits applies to a Mode A envelope and a natively-parsed tree alike. An adapter emits
/// call sites exactly as the source spells them, so an envelope is if anything MORE likely to need it —
/// no code-extracted base pass runs in this mode at all. And it must stay idempotent here too: the
/// second consume below already carries the base, and re-prefixing it would break a joining key.
#[test]
fn analyze_envelope_json_client_base_prefixes_relative_consume_keys_over_the_wire() {
    let envelope = r#"{
        "format": "zzop-normalized-ast",
        "version": "0.27.0",
        "parser": "jsp-lexical/1",
        "source": "legacy",
        "files": [
            {
                "path": "legacy/Client.jsp",
                "loc": 20,
                "io": {
                    "provides": [],
                    "consumes": [
                        {"kind": "http", "key": "GET /users", "file": "legacy/Client.jsp", "line": 3},
                        {"kind": "http", "key": "GET /api/orders", "file": "legacy/Client.jsp", "line": 4}
                    ]
                }
            }
        ]
    }"#;
    let config = r#"{"sourceId": "legacy", "clientBase": "/api"}"#;
    let out =
        analyze_envelope_json(envelope, config).expect("analyze_envelope_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    let keys: Vec<&str> = value["ir"]["io"]["consumes"]
        .as_array()
        .expect("consumes array")
        .iter()
        .map(|c| c["key"].as_str().expect("key"))
        .collect();
    assert_eq!(
        keys,
        vec!["GET /api/orders", "GET /api/users"],
        "clientBase sent over the analyzeEnvelope config wire must prefix the suffix-only consume and \
         leave the already-based one alone, got: {value}"
    );
}

/// A Mode A envelope whose mutating route's handler calls `save` (declared in the same file, so the
/// edge RESOLVES) — the substrate for the two vocabulary pins below.
fn callgraph_envelope_json() -> String {
    format!(
        r#"{{
        "format": "zzop-normalized-ast",
        "version": "{}",
        "parser": "java-lexical/1",
        "source": "legacy",
        "files": [
            {{
                "path": "src/main/java/OrderController.java",
                "loc": 40,
                "symbols": [
                    {{"id": "src/main/java/OrderController.java#createOrder", "file": "src/main/java/OrderController.java", "name": "createOrder", "kind": "function", "line": 5, "exported": true}},
                    {{"id": "src/main/java/OrderController.java#save", "file": "src/main/java/OrderController.java", "name": "save", "kind": "function", "line": 25, "exported": false}}
                ],
                "io": {{
                    "provides": [{{"kind": "http", "key": "POST /orders", "file": "src/main/java/OrderController.java", "line": 5, "symbol": "createOrder"}}],
                    "consumes": []
                }},
                "calls": [{{"from_symbol": "src/main/java/OrderController.java#createOrder", "callee_name": "save", "line": 7}}]
            }}
        ]
    }}"#,
        zzop_core::NORMALIZED_AST_CONTRACT_VERSION
    )
}

fn finding_rule_ids(out: &str) -> Vec<String> {
    let value: serde_json::Value = serde_json::from_str(out).expect("valid JSON");
    value["findings"]
        .as_array()
        .expect("findings array")
        .iter()
        .map(|f| f["ruleId"].as_str().expect("ruleId").to_string())
        .collect()
}

/// B4 red->green: a `vocabulary` declared on the `analyzeEnvelope` config wire must REACH Mode A's
/// call-graph pass. Before this field existed, `{"vocabulary": {...}}` deserialized to nothing
/// (`EnvelopeAnalyzeRequest` had no such field and no `deny_unknown_fields`), and the lane ran on the
/// engine default it happened to inherit — a declaration the wire accepted and silently discarded.
#[test]
fn analyze_envelope_json_declared_vocabulary_reaches_the_callgraph_pass() {
    // Undeclared: the product default (built-in vocabulary) does not treat `save` as an auth guard,
    // so the unguarded mutating route fires.
    let base = analyze_envelope_json(&callgraph_envelope_json(), r#"{"sourceId": "legacy"}"#)
        .expect("analyze_envelope_json should succeed");
    assert!(
        finding_rule_ids(&base)
            .iter()
            .any(|id| id == "mutating-route-no-auth"),
        "without a declaration the built-in vocabulary must leave the route flagged: {base}"
    );

    // Declared: the project says `^save$` IS its guard spelling — the same declaration a tree request
    // carries — so the reached `save` symbol clears the route.
    let declared = analyze_envelope_json(
        &callgraph_envelope_json(),
        r#"{"sourceId": "legacy", "vocabulary": {"authGuardPattern": "^save$"}}"#,
    )
    .expect("analyze_envelope_json should succeed");
    assert!(
        !finding_rule_ids(&declared)
            .iter()
            .any(|id| id == "mutating-route-no-auth"),
        "a declared authGuardPattern must clear the route it matches: {declared}"
    );
}

/// The other half of the same contract: a request that declares NO vocabulary gets the PRODUCT
/// default (`VocabularyConfig::built_in()`) assigned explicitly at the facade chokepoint — the same
/// place the bundled-pack default lives — never by falling through to whatever `EngineConfig`'s
/// default happens to be. `verifyToken` matches the built-in auth-guard vocabulary, so the route
/// clears; if an undeclared vocabulary meant "no judgment" here (the tree-lane empty declaration),
/// this route would fire instead.
#[test]
fn analyze_envelope_json_undeclared_vocabulary_keeps_the_product_default() {
    // Rename the callee symbol wholesale: `verifyToken` is in the built-in guard vocabulary.
    let envelope = callgraph_envelope_json().replace("save", "verifyToken");
    let out = analyze_envelope_json(&envelope, r#"{"sourceId": "legacy"}"#)
        .expect("analyze_envelope_json should succeed");
    assert!(
        !finding_rule_ids(&out)
            .iter()
            .any(|id| id == "mutating-route-no-auth"),
        "undeclared vocabulary must keep the built-in guard vocabulary (verifyToken clears): {out}"
    );
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
    // Absolute path + a non-normalized `http` provide key: both join with nothing, neither is a
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
    // A FUTURE version, not a malformed one: a malformed `version` fails to deserialize, and a text
    // that never became an envelope has nothing to run the hint pass over — which would make this
    // assert the opposite of what it is sealing.
    let bad = tiny_envelope_json()
        .replace("\"version\": \"0.27.0\"", "\"version\": \"99.0.0\"")
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
