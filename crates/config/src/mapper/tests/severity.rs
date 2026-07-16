use std::path::Path;

use serde_json::json;

use super::analyze_request;
use crate::mapper::config_to_request;
use crate::mapper::paths::is_glob_pattern;
use crate::mapper::severity::{normalize_severity, SeverityValue};

// --- severity aliases ------------------------------------------------------------------------

#[test]
fn severity_aliases_cover_every_documented_bucket() {
    for off in ["off", "none", "disable", "disabled", "OFF", " Off "] {
        assert_eq!(
            normalize_severity(&json!(off), None).unwrap(),
            SeverityValue::Off
        );
    }
    for critical in ["critical", "error", "err", "high", "CRITICAL", " Error "] {
        assert_eq!(
            normalize_severity(&json!(critical), None).unwrap(),
            SeverityValue::Engine("critical")
        );
    }
    for warning in ["warning", "warn", "medium", "WARN"] {
        assert_eq!(
            normalize_severity(&json!(warning), None).unwrap(),
            SeverityValue::Engine("warning")
        );
    }
    for info in ["info", "information", "note", "low", "INFO"] {
        assert_eq!(
            normalize_severity(&json!(info), None).unwrap(),
            SeverityValue::Engine("info")
        );
    }
}

#[test]
fn unknown_severity_error_text_lists_every_alias() {
    let err = normalize_severity(&json!("bogus"), Some("circular")).unwrap_err();
    assert_eq!(
        err.0,
        "Unknown severity \"bogus\" for \"circular\". Expected one of: off, none, disable, disabled, \
         critical, error, err, high, warning, warn, medium, info, information, note, low."
    );
}

#[test]
fn non_string_severity_error_text_matches_js() {
    let err = normalize_severity(&json!(5), Some("circular")).unwrap_err();
    assert_eq!(
        err.0,
        "Invalid severity 5 for \"circular\": expected a string."
    );
}

#[test]
fn severity_off_routes_a_rule_into_disabled_rules() {
    let mapped = config_to_request(
        &json!({"roots": ["."], "rules": {"toctou": "off"}}),
        Path::new("/base"),
    )
    .unwrap();
    let req = analyze_request(&mapped.request);
    let disabled = req["disabledRules"].as_array().unwrap();
    assert!(disabled.iter().any(|v| v == "toctou"));
    assert!(req.get("severityOverrides").is_none());
}

#[test]
fn severity_object_form_off_also_routes_to_disabled_rules() {
    let mapped = config_to_request(
        &json!({"roots": ["."], "rules": {"toctou": {"severity": "off"}}}),
        Path::new("/base"),
    )
    .unwrap();
    let req = analyze_request(&mapped.request);
    assert!(req["disabledRules"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "toctou"));
}

#[test]
fn severity_warn_becomes_an_engine_severity_override() {
    let mapped = config_to_request(
        &json!({"roots": ["."], "rules": {"n-plus-one": "warn"}}),
        Path::new("/base"),
    )
    .unwrap();
    let req = analyze_request(&mapped.request);
    assert_eq!(req["severityOverrides"]["n-plus-one"], "warning");
}

// --- glob vs. substring split ------------------------------------------------------------------

#[test]
fn bracketed_dynamic_segment_stays_a_substring_path_not_a_glob() {
    assert!(!is_glob_pattern("app/[locale]/page.tsx"));
    let mapped = config_to_request(
        &json!({"roots": ["."], "exclude": ["app/[locale]/"]}),
        Path::new("/base"),
    )
    .unwrap();
    let req = analyze_request(&mapped.request);
    let excludes = req["globalExcludes"].as_array().unwrap();
    assert_eq!(excludes[0]["path"], "app/[locale]/");
    assert!(excludes[0].get("glob").is_none());
}

#[test]
fn star_and_brace_patterns_are_treated_as_globs() {
    for pattern in ["**/*.spec.ts", "src/{a,b}.ts", "file?.ts"] {
        assert!(
            is_glob_pattern(pattern),
            "{pattern} should be detected as a glob"
        );
    }
    let mapped = config_to_request(
        &json!({"roots": ["."], "exclude": ["**/*.spec.ts"]}),
        Path::new("/base"),
    )
    .unwrap();
    let req = analyze_request(&mapped.request);
    assert_eq!(req["globalExcludes"][0]["glob"], "**/*.spec.ts");
}

#[test]
fn rules_exclude_entries_split_glob_vs_substring_independently() {
    let mapped = config_to_request(
        &json!({"roots": ["."], "rules": {"toctou": {"exclude": ["legacy/", "**/*.gen.ts"]}}}),
        Path::new("/base"),
    )
    .unwrap();
    let req = analyze_request(&mapped.request);
    let suppressions = req["suppressions"].as_array().unwrap();
    assert_eq!(suppressions.len(), 2);
    assert_eq!(suppressions[0]["path"], "legacy/");
    assert_eq!(suppressions[1]["glob"], "**/*.gen.ts");
    for s in suppressions {
        assert_eq!(s["rule"], "toctou");
    }
}
