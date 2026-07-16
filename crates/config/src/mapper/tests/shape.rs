use std::path::Path;

use serde_json::json;

use super::analyze_request;
use crate::mapper::config_to_request;
use crate::mapper::paths::{path_to_string, resolve_path};
use crate::Method;

// --- method selection / sourceId -----------------------------------------------------------

#[test]
fn single_root_selects_analyze_with_no_source_id() {
    let mapped = config_to_request(&json!({"roots": ["."]}), Path::new("/base")).unwrap();
    assert_eq!(mapped.method, Method::Analyze);
    let req = analyze_request(&mapped.request);
    assert!(req.get("sourceId").is_none());
}

#[test]
fn default_config_with_no_roots_key_also_selects_analyze() {
    let base = Path::new("/base");
    let mapped = config_to_request(&json!({}), base).unwrap();
    assert_eq!(mapped.method, Method::Analyze);
    let req = analyze_request(&mapped.request);
    // Compare against the same lexical-resolution helper the mapper itself uses (rather than a
    // hand-written literal) since `resolve_path` rebuilds the path via `PathBuf::push`, which can
    // normalize separators (e.g. to `\` on Windows) relative to the raw input string.
    assert_eq!(req["root"], path_to_string(&resolve_path(base, ".")));
}

#[test]
fn multiple_roots_select_analyze_trees_and_each_gets_a_raw_source_id() {
    let mapped = config_to_request(&json!({"roots": ["./a", "./b"]}), Path::new("/base")).unwrap();
    assert_eq!(mapped.method, Method::AnalyzeTrees);
    let trees = mapped.request["trees"].as_array().unwrap();
    assert_eq!(trees.len(), 2);
    assert_eq!(trees[0]["sourceId"], "./a");
    assert_eq!(trees[1]["sourceId"], "./b");
}

#[test]
fn single_entry_trees_array_still_selects_analyze_trees() {
    let mapped =
        config_to_request(&json!({"trees": [{"root": "./api"}]}), Path::new("/base")).unwrap();
    assert_eq!(mapped.method, Method::AnalyzeTrees);
    assert_eq!(mapped.request["trees"].as_array().unwrap().len(), 1);
}

#[test]
fn tree_source_id_defaults_to_the_raw_configured_root_string() {
    let mapped = config_to_request(
        &json!({"trees": [{"root": "./api"}, {"root": "./web"}]}),
        Path::new("/base"),
    )
    .unwrap();
    let trees = mapped.request["trees"].as_array().unwrap();
    assert_eq!(trees[0]["sourceId"], "./api");
    assert_eq!(trees[1]["sourceId"], "./web");
    // The resolved `root` field, unlike `sourceId`, is absolute.
    assert_ne!(trees[0]["root"], "./api");
}

#[test]
fn explicit_source_id_overrides_the_root_default() {
    let mapped = config_to_request(
        &json!({"trees": [{"root": "./api", "sourceId": "api"}]}),
        Path::new("/base"),
    )
    .unwrap();
    assert_eq!(mapped.request["trees"][0]["sourceId"], "api");
}

#[test]
fn trees_wins_over_roots_silently_when_both_are_present() {
    let mapped = config_to_request(
        &json!({"roots": ["./ignored"], "trees": [{"root": "./api"}]}),
        Path::new("/base"),
    )
    .unwrap();
    assert_eq!(mapped.method, Method::AnalyzeTrees);
    let trees = mapped.request["trees"].as_array().unwrap();
    assert_eq!(trees.len(), 1);
    assert_eq!(trees[0]["sourceId"], "./api");
}

#[test]
fn trees_auto_unexpanded_is_rejected_with_a_pointer_to_expand_auto_trees() {
    let err = config_to_request(&json!({"trees": "auto"}), Path::new("/base")).unwrap_err();
    assert!(err.0.contains("workspaces::expand_auto_trees"));
}

// --- validation gate error texts (verbatim) -----------------------------------------------

#[test]
fn top_level_config_must_be_a_json_object() {
    let err = config_to_request(&json!(null), Path::new("/base")).unwrap_err();
    assert_eq!(err.0, "Config must be a JSON object.");
    let err = config_to_request(&json!([1, 2]), Path::new("/base")).unwrap_err();
    assert_eq!(err.0, "Config must be a JSON object.");
}

#[test]
fn rules_must_be_an_object() {
    let err = config_to_request(&json!({"rules": []}), Path::new("/base")).unwrap_err();
    assert_eq!(
        err.0,
        "rules must be an object mapping rule ids to a severity or a rule object."
    );
}

#[test]
fn falsy_rules_values_are_treated_as_absent_not_an_error() {
    for v in [json!(null), json!(false), json!(0), json!("")] {
        let mapped =
            config_to_request(&json!({"roots": ["."], "rules": v}), Path::new("/base")).unwrap();
        let req = analyze_request(&mapped.request);
        assert!(req.get("severityOverrides").is_none());
    }
}

#[test]
fn roots_shape_errors_match_js_text() {
    let err = config_to_request(&json!({"roots": []}), Path::new("/base")).unwrap_err();
    assert_eq!(err.0, "roots must be a non-empty array of directory paths.");
    let err = config_to_request(&json!({"roots": [""]}), Path::new("/base")).unwrap_err();
    assert_eq!(err.0, "roots entries must be non-empty strings.");
}

#[test]
fn trees_shape_errors_match_js_text() {
    let err = config_to_request(&json!({"trees": []}), Path::new("/base")).unwrap_err();
    assert_eq!(
        err.0,
        "trees, when present, must be a non-empty array of { root, sourceId }."
    );
    let err =
        config_to_request(&json!({"trees": [{"sourceId": "x"}]}), Path::new("/base")).unwrap_err();
    assert_eq!(
        err.0,
        "trees[0] must be an object with a non-empty \"root\" string."
    );
}

#[test]
fn packs_must_be_an_object() {
    for v in [json!([]), json!("x"), json!(5), json!(true)] {
        let err = config_to_request(&json!({"packs": v}), Path::new("/base")).unwrap_err();
        assert_eq!(
            err.0,
            "packs must be an object ({ \"extraDirs\": [...], \"disabled\": [...] })."
        );
    }
}

#[test]
fn falsy_packs_values_are_treated_as_absent_not_an_error() {
    for v in [json!(null), json!(false), json!(0), json!("")] {
        let mapped =
            config_to_request(&json!({"roots": ["."], "packs": v}), Path::new("/base")).unwrap();
        let req = analyze_request(&mapped.request);
        assert!(req.get("packsDir").is_none());
        assert!(req.get("disabledRules").is_none());
    }
}

#[test]
fn packs_extra_dirs_must_be_an_array() {
    let err =
        config_to_request(&json!({"packs": {"extraDirs": "x"}}), Path::new("/base")).unwrap_err();
    assert_eq!(
        err.0,
        "packs.extraDirs must be an array of directory paths."
    );
}

#[test]
fn exclude_shape_errors_match_js_text() {
    let err = config_to_request(
        &json!({"roots": ["."], "exclude": "legacy/"}),
        Path::new("/base"),
    )
    .unwrap_err();
    assert_eq!(
        err.0,
        "exclude must be an array of path substrings or globs."
    );
    let err = config_to_request(
        &json!({"roots": ["."], "exclude": [123]}),
        Path::new("/base"),
    )
    .unwrap_err();
    assert_eq!(err.0, "exclude entries must be strings.");
}

#[test]
fn rules_exclude_shape_errors_match_js_text() {
    let err = config_to_request(
        &json!({"rules": {"toctou": {"exclude": "legacy/"}}}),
        Path::new("/base"),
    )
    .unwrap_err();
    assert_eq!(
        err.0,
        "rules.toctou.exclude must be an array of path substrings or globs."
    );
}
