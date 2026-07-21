//! Tests for the crate-root loading entry points.

use super::*;
use crate::test_support::TempDir;

#[test]
fn load_for_root_absent_config_produces_the_zero_config_request() {
    let dir = TempDir::new("zzop-config-lib-absent");
    let loaded = load_for_root(dir.path()).unwrap();
    assert!(loaded.config_path.is_none());
    assert_eq!(loaded.method, Method::Analyze);
    let req = loaded.request.as_object().unwrap();
    assert_eq!(req["root"], dir.path().to_string_lossy().into_owned());
    assert_eq!(req["git"], serde_json::json!({}));
    assert_eq!(req["packDefs"].as_array().unwrap().len(), 14);
}

#[test]
fn load_for_root_present_config_is_discovered_and_mapped() {
    let dir = TempDir::new("zzop-config-lib-present");
    dir.write(
        DEFAULT_CONFIG_FILENAME,
        r#"{ "roots": ["."], "rules": { "toctou": "off" } }"#,
    );
    let loaded = load_for_root(dir.path()).unwrap();
    assert_eq!(
        loaded.config_path,
        Some(dir.path().join(DEFAULT_CONFIG_FILENAME))
    );
    let req = loaded.request.as_object().unwrap();
    assert!(req["disabledRules"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "toctou"));
}

#[test]
fn load_config_file_accepts_a_direct_file_path() {
    let dir = TempDir::new("zzop-config-lib-direct-file");
    let config_path = dir.path().join("custom.jsonc");
    std::fs::write(&config_path, r#"{ "roots": ["."] }"#).unwrap();
    let loaded = load_config_file(&config_path).unwrap();
    assert_eq!(loaded.config_path, Some(config_path));
}

#[test]
fn load_config_file_accepts_a_directory_and_finds_the_default_filename() {
    let dir = TempDir::new("zzop-config-lib-dir");
    dir.write(DEFAULT_CONFIG_FILENAME, r#"{ "roots": ["."] }"#);
    let loaded = load_config_file(dir.path()).unwrap();
    assert_eq!(
        loaded.config_path,
        Some(dir.path().join(DEFAULT_CONFIG_FILENAME))
    );
}

#[test]
fn load_config_file_missing_reports_the_adapted_error_text() {
    let dir = TempDir::new("zzop-config-lib-missing");
    let missing = dir.path().join("nope.jsonc");
    let err = load_config_file(&missing).unwrap_err();
    assert!(err
        .0
        .starts_with(&format!("No config file at {}.", missing.display())));
    assert!(err
        .0
        .contains("Create a zzop.config.jsonc there, or pass a directory that has one."));
    // The JS-CLI-only hint (`zzop init`, `--config`) must NOT survive the port.
    assert!(!err.0.contains("zzop init"));
    assert!(!err.0.contains("--config"));
}

#[test]
fn load_config_file_missing_in_a_directory_also_reports_the_expected_default_filename() {
    let dir = TempDir::new("zzop-config-lib-missing-dir");
    let err = load_config_file(dir.path()).unwrap_err();
    assert!(err.0.starts_with(&format!(
        "No config file at {}.",
        dir.path().join(DEFAULT_CONFIG_FILENAME).display()
    )));
}

#[test]
fn load_config_file_invalid_jsonc_reports_the_adapted_error_text() {
    let dir = TempDir::new("zzop-config-lib-bad-jsonc");
    dir.write(DEFAULT_CONFIG_FILENAME, "{ not valid json");
    let err = load_config_file(dir.path()).unwrap_err();
    assert!(err.0.starts_with(&format!(
        "Invalid JSONC in {}: ",
        dir.path().join(DEFAULT_CONFIG_FILENAME).display()
    )));
}

#[test]
fn load_config_file_non_object_top_level_reports_the_adapted_error_text() {
    let dir = TempDir::new("zzop-config-lib-non-object");
    dir.write(DEFAULT_CONFIG_FILENAME, "[1, 2, 3]");
    let err = load_config_file(dir.path()).unwrap_err();
    assert_eq!(
        err.0,
        format!(
            "Config in {} must be a JSON object.",
            dir.path().join(DEFAULT_CONFIG_FILENAME).display()
        )
    );
}

#[test]
fn load_config_file_unreadable_bytes_report_a_could_not_read_error() {
    // Invalid UTF-8 makes `read_to_string` fail with an I/O error, exercising the "file exists
    // but could not be read" branch (distinct from "missing") portably, without relying on
    // platform-specific file permission behavior.
    let dir = TempDir::new("zzop-config-lib-bad-utf8");
    let config_path = dir.path().join(DEFAULT_CONFIG_FILENAME);
    std::fs::write(&config_path, [0xFFu8, 0xFE, 0x00, 0x01]).unwrap();
    let err = load_config_file(&config_path).unwrap_err();
    assert!(err.0.starts_with(&format!(
        "Could not read config at {}: ",
        config_path.display()
    )));
}

#[test]
fn load_config_file_comments_and_trailing_commas_are_stripped_before_parsing() {
    let dir = TempDir::new("zzop-config-lib-jsonc-quirks");
    dir.write(
        DEFAULT_CONFIG_FILENAME,
        "{\n  // a comment\n  \"roots\": [\".\",],\n}",
    );
    let loaded = load_config_file(dir.path()).unwrap();
    let req = loaded.request.as_object().unwrap();
    assert_eq!(req["root"], dir.path().to_string_lossy().into_owned());
}
