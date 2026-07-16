use std::path::Path;

use serde_json::json;

use super::analyze_request;
use crate::mapper::config_to_request;
use crate::test_support::TempDir;

// --- overlays ---------------------------------------------------------------------------------

#[test]
fn overlay_happy_path_attaches_parsed_json_to_adapter_overlays() {
    let dir = TempDir::new("zzop-config-overlay-happy");
    dir.write(
        "overlay.json",
        r#"{"format": "zzop-normalized-ast", "version": 1}"#,
    );
    let mapped = config_to_request(
        &json!({"roots": ["."], "overlays": ["overlay.json"]}),
        dir.path(),
    )
    .unwrap();
    let req = analyze_request(&mapped.request);
    let overlays = req["adapterOverlays"].as_array().unwrap();
    assert_eq!(overlays.len(), 1);
    assert_eq!(overlays[0]["format"], "zzop-normalized-ast");
    assert_eq!(overlays[0]["version"], 1);
    assert!(mapped.warnings.iter().all(|w| !w.contains("overlay")));
}

#[test]
fn missing_overlay_file_produces_a_warning_and_is_skipped() {
    let dir = TempDir::new("zzop-config-overlay-missing");
    let mapped = config_to_request(
        &json!({"roots": ["."], "overlays": ["does-not-exist.json"]}),
        dir.path(),
    )
    .unwrap();
    let req = analyze_request(&mapped.request);
    assert!(req.get("adapterOverlays").is_none());
    assert!(mapped.warnings.iter().any(|w| {
        w.contains("overlay \"does-not-exist.json\"")
            && w.contains("could not be read")
            && w.contains("This overlay is skipped.")
    }));
}

// --- io error text stays locale-independent English ---------------------------------------------

#[test]
fn missing_overlay_warning_carries_the_stable_english_io_label_not_os_strerror() {
    // `io::Error`'s Display renders in the OS UI language on Windows (Korean on a Korean host); the
    // warning must instead carry the fixed-vocabulary `ErrorKind` label from `crate::io_error_label`
    // so config warnings honor the English-output contract on every locale.
    let dir = TempDir::new("zzop-config-overlay-io-label");
    let mapped = config_to_request(
        &json!({"roots": ["."], "overlays": ["does-not-exist.json"]}),
        dir.path(),
    )
    .unwrap();
    assert!(
        mapped
            .warnings
            .iter()
            .any(|w| w.contains("could not be read: NotFound (os error ")),
        "expected the stable ErrorKind label in the read-failure warning, got: {:?}",
        mapped.warnings
    );
}

#[test]
fn io_error_label_is_fixed_vocabulary_english_derived_from_error_kind() {
    // Code 2 is "file not found" on every supported platform (ENOENT / ERROR_FILE_NOT_FOUND).
    let with_code = std::io::Error::from_raw_os_error(2);
    assert_eq!(crate::io_error_label(&with_code), "NotFound (os error 2)");
    // A synthesized error with no OS code renders as the bare ErrorKind name.
    let without_code = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "synthetic");
    assert_eq!(crate::io_error_label(&without_code), "PermissionDenied");
}

#[test]
fn unparseable_overlay_file_produces_a_warning_and_is_skipped() {
    let dir = TempDir::new("zzop-config-overlay-bad-json");
    dir.write("bad.json", "{not json");
    let mapped = config_to_request(
        &json!({"roots": ["."], "overlays": ["bad.json"]}),
        dir.path(),
    )
    .unwrap();
    let req = analyze_request(&mapped.request);
    assert!(req.get("adapterOverlays").is_none());
    assert!(mapped.warnings.iter().any(|w| {
        w.contains("overlay \"bad.json\"")
            && w.contains("is not valid JSON")
            && w.contains("skipped")
    }));
}

#[test]
fn overlay_paths_resolve_against_the_tree_root_not_base_dir() {
    let dir = TempDir::new("zzop-config-overlay-tree-relative");
    dir.write("sub/overlay.json", r#"{"marker": "sub"}"#);
    dir.write("overlay.json", r#"{"marker": "top"}"#);
    let mapped = config_to_request(
        &json!({"trees": [{"root": "sub", "overlays": ["overlay.json"]}]}),
        dir.path(),
    )
    .unwrap();
    let overlays = mapped.request["trees"][0]["adapterOverlays"]
        .as_array()
        .unwrap();
    assert_eq!(overlays[0]["marker"], "sub");
}

#[test]
fn shared_and_tree_overlays_both_apply_in_order() {
    let dir = TempDir::new("zzop-config-overlay-shared-and-tree");
    dir.write("shared.json", r#"{"marker": "shared"}"#);
    dir.write("tree.json", r#"{"marker": "tree"}"#);
    let mapped = config_to_request(
        &json!({"trees": [{"root": ".", "overlays": ["tree.json"]}], "overlays": ["shared.json"]}),
        dir.path(),
    )
    .unwrap();
    let overlays = mapped.request["trees"][0]["adapterOverlays"]
        .as_array()
        .unwrap();
    assert_eq!(overlays.len(), 2);
    assert_eq!(overlays[0]["marker"], "shared");
    assert_eq!(overlays[1]["marker"], "tree");
}

#[test]
fn overlays_shape_errors_match_js_text() {
    let err = config_to_request(
        &json!({"roots": ["."], "overlays": "valid.json"}),
        Path::new("/base"),
    )
    .unwrap_err();
    assert_eq!(err.0, "overlays must be an array of file paths.");
    let err = config_to_request(
        &json!({"roots": ["."], "overlays": [123]}),
        Path::new("/base"),
    )
    .unwrap_err();
    assert_eq!(
        err.0,
        "overlays entries must be non-empty strings (paths to overlay JSON files)."
    );
}

// --- unknown-key warnings at 3+ scopes -----------------------------------------------------

#[test]
fn unknown_key_warnings_fire_at_top_packs_and_tree_scopes() {
    let mapped = config_to_request(
        &json!({
            "roots": ["."],
            "bogusTopLevel": true,
            "packs": {"bogusPacksKey": 1},
        }),
        Path::new("/base"),
    )
    .unwrap();
    assert!(mapped
        .warnings
        .iter()
        .any(|w| w.contains("unknown config key \"bogusTopLevel\"")
            && w.contains("at the top level")));
    assert!(mapped
        .warnings
        .iter()
        .any(|w| w.contains("unknown config key \"packs.bogusPacksKey\"")
            && w.contains("under \"packs\"")));

    let mapped2 = config_to_request(
        &json!({"trees": [{"root": ".", "bogusTreeKey": 1}]}),
        Path::new("/base"),
    )
    .unwrap();
    assert!(mapped2.warnings.iter().any(|w| w
        .contains("unknown config key \"trees[0].bogusTreeKey\"")
        && w.contains("under \"trees[0]\"")));
}

#[test]
fn unknown_key_warning_fires_inside_a_mounts_entry() {
    let mapped = config_to_request(
        &json!({"trees": [{"root": ".", "mounts": [{"dir": "a", "at": "/a", "bogus": 1}]}]}),
        Path::new("/base"),
    )
    .unwrap();
    assert!(mapped
        .warnings
        .iter()
        .any(|w| w.contains("unknown config key \"trees[0].mounts[0].bogus\"")));
}

#[test]
fn unknown_key_warning_fires_inside_a_rule_object() {
    let mapped = config_to_request(
        &json!({"rules": {"toctou": {"severity": "off", "bogus": 1}}}),
        Path::new("/base"),
    )
    .unwrap();
    assert!(mapped
        .warnings
        .iter()
        .any(|w| w.contains("unknown config key \"rules.toctou.bogus\"")));
}

#[test]
fn known_keys_never_warn() {
    let mapped = config_to_request(
        &json!({
            "roots": ["."],
            "packs": {"extraDirs": [], "disabled": []},
            "git": {"since": "2024-01-01"},
            "rules": {"toctou": {"severity": "warn", "exclude": ["a"]}},
        }),
        Path::new("/base"),
    )
    .unwrap();
    assert!(mapped
        .warnings
        .iter()
        .all(|w| !w.contains("unknown config key")));
}

// --- CLI-presentation keys are known but never forwarded -----------------------------------

#[test]
fn fail_on_format_report_are_known_but_not_forwarded() {
    let mapped = config_to_request(
        &json!({"roots": ["."], "failOn": "critical", "format": "json", "report": {"dir": "out"}}),
        Path::new("/base"),
    )
    .unwrap();
    assert!(mapped
        .warnings
        .iter()
        .all(|w| !w.contains("unknown config key")));
    let req = analyze_request(&mapped.request);
    assert!(req.get("failOn").is_none());
    assert!(req.get("format").is_none());
    assert!(req.get("report").is_none());
}
