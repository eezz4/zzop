//! Unit tests for `validate_rule_pack_json` (`crate::rule_pack`) — the pre-load, structure-only
//! rule-pack check — plus the rule-pack JSON Schema's own basic sanity (it must parse, and its
//! matcher vocabulary must match `zzop_core::dsl::def`'s serde tags).

use crate::validate_rule_pack_json;

/// A real bundled pack, byte-for-byte (the same embed `zzop-config`/`zzop-mcp` ship) — every
/// shipped pack must self-evidently pass its own pre-load validator.
const BUNDLED_SECURITY_PACK: &str = include_str!("../../../rules/dsl/security/security.json");

/// The authored JSON Schema for the rule-pack shape (embedded by `zzop-mcp` as
/// `zzop://contract/rule-pack-schema`).
const RULE_PACK_SCHEMA: &str = include_str!("../../../docs/contracts/rule-pack.schema.json");

fn report(pack_json: &str) -> serde_json::Value {
    serde_json::from_str(&validate_rule_pack_json(pack_json)).expect("report is valid JSON")
}

#[test]
fn a_bundled_pack_validates_clean() {
    let v = report(BUNDLED_SECURITY_PACK);
    assert_eq!(v["valid"], true, "got: {v}");
    assert_eq!(v["issues"].as_array().expect("issues array").len(), 0);
}

#[test]
fn every_bundled_pack_validates_clean() {
    // The embed IS the shipped bundle — all of it must pass the validator it now fronts.
    for (rel, source) in zzop_config::BUNDLED_PACK_SOURCES {
        let v = report(source);
        assert_eq!(
            v["valid"], true,
            "bundled pack {rel} failed validation: {v}"
        );
    }
}

#[test]
fn unparseable_input_reports_invalid_without_erring() {
    let v = report("{ not json");
    assert_eq!(v["valid"], false);
    assert!(!v["issues"].as_array().unwrap().is_empty());
}

#[test]
fn a_missing_required_field_is_a_named_issue() {
    // Drop `rules` — the loader's serde judgment, verbatim.
    let v = report(r#"{"id": "p"}"#);
    assert_eq!(v["valid"], false);
    let issues = v["issues"].as_array().unwrap();
    assert!(
        issues
            .iter()
            .any(|i| i.as_str().unwrap().contains("missing field `rules`")),
        "got: {v}"
    );
}

#[test]
fn a_too_new_schema_version_is_a_named_issue() {
    let v = report(r#"{"id": "p", "schema_version": 999, "rules": []}"#);
    assert_eq!(v["valid"], false);
    assert!(
        v["issues"][0]
            .as_str()
            .unwrap()
            .contains("newer DSL schema"),
        "got: {v}"
    );
}

#[test]
fn a_non_compiling_regex_is_a_named_issue() {
    let broken = BUNDLED_SECURITY_PACK.replacen(r#""(?i)\\.(ts|tsx)$""#, r#""(?i)\\.(ts|tsx$""#, 1);
    assert_ne!(broken, BUNDLED_SECURITY_PACK, "the replace must have hit");
    let v = report(&broken);
    assert_eq!(v["valid"], false, "got: {v}");
    let issue = v["issues"][0].as_str().unwrap();
    assert!(issue.contains("`file_pattern`"), "got: {v}");
    assert!(issue.contains("never fire"), "got: {v}");
}

#[test]
fn the_rule_pack_schema_parses_and_names_all_four_matcher_kinds() {
    let schema: serde_json::Value =
        serde_json::from_str(RULE_PACK_SCHEMA).expect("rule-pack schema must be valid JSON");
    assert_eq!(schema["$schema"], "http://json-schema.org/draft-07/schema#");
    // The matcher discriminator vocabulary must match `zzop_core::dsl::def::Matcher`'s serde tags.
    let text = RULE_PACK_SCHEMA;
    for kind in ["line-scan", "method-scan", "symbol-scan", "io-scan"] {
        assert!(
            text.contains(&format!("\"{kind}\"")),
            "schema must document matcher kind {kind}"
        );
    }
    // Severity vocabulary must match `zzop_core::Severity`'s lowercase serde form.
    for sev in ["critical", "warning", "info"] {
        assert!(
            text.contains(&format!("\"{sev}\"")),
            "missing severity {sev}"
        );
    }
}
