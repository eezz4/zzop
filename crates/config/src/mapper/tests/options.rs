use std::path::Path;

use serde_json::json;

use super::analyze_request;
use crate::mapper::config_to_request;
use crate::mapper::paths::{path_to_string, resolve_path};
use crate::mapper::warnings::parse_pack_defs;
use crate::test_support::TempDir;
use crate::Method;

// --- packs.extraDirs resolution ------------------------------------------------------------

#[test]
fn packs_extra_dirs_resolve_against_base_dir_and_are_omitted_when_empty() {
    let mapped = config_to_request(
        &json!({"roots": ["."], "packs": {"extraDirs": ["./zzop-packs"]}}),
        Path::new("/base"),
    )
    .unwrap();
    let req = analyze_request(&mapped.request);
    let dirs = req["packsDir"].as_array().unwrap();
    assert_eq!(
        dirs[0],
        path_to_string(&resolve_path(Path::new("/base"), "./zzop-packs"))
    );

    let mapped_empty = config_to_request(
        &json!({"roots": ["."], "packs": {"extraDirs": []}}),
        Path::new("/base"),
    )
    .unwrap();
    assert!(analyze_request(&mapped_empty.request)
        .get("packsDir")
        .is_none());

    let mapped_none = config_to_request(&json!({"roots": ["."]}), Path::new("/base")).unwrap();
    assert!(analyze_request(&mapped_none.request)
        .get("packsDir")
        .is_none());
}

// --- git / cacheDir / sizeCap passthrough + withDefaults ------------------------------------

#[test]
fn git_defaults_to_empty_object_when_absent() {
    let mapped = config_to_request(&json!({"roots": ["."]}), Path::new("/base")).unwrap();
    let req = analyze_request(&mapped.request);
    assert_eq!(req["git"], json!({}));
}

#[test]
fn git_passthrough_is_not_overwritten_by_the_default() {
    let mapped = config_to_request(
        &json!({"roots": ["."], "git": {"since": "2024-01-01"}}),
        Path::new("/base"),
    )
    .unwrap();
    let req = analyze_request(&mapped.request);
    assert_eq!(req["git"]["since"], "2024-01-01");
}

#[test]
fn cache_dir_resolves_against_base_dir() {
    let mapped = config_to_request(
        &json!({"roots": ["."], "cacheDir": "./.zzop-cache"}),
        Path::new("/base"),
    )
    .unwrap();
    let req = analyze_request(&mapped.request);
    assert_eq!(
        req["cacheDir"],
        path_to_string(&resolve_path(Path::new("/base"), "./.zzop-cache"))
    );
}

#[test]
fn size_cap_passes_through_unchanged() {
    let mapped =
        config_to_request(&json!({"roots": ["."], "sizeCap": 999}), Path::new("/base")).unwrap();
    let req = analyze_request(&mapped.request);
    assert_eq!(req["sizeCap"], 999);
}

// --- packDefs ---------------------------------------------------------------------------------

#[test]
fn pack_defs_carries_every_bundled_pack_with_no_parse_warnings() {
    let mapped = config_to_request(&json!({"roots": ["."]}), Path::new("/base")).unwrap();
    let req = analyze_request(&mapped.request);
    let pack_defs = req["packDefs"].as_array().unwrap();
    assert_eq!(
        pack_defs.len(),
        15,
        "expected exactly the 15 bundled DSL packs"
    );
    assert!(mapped.warnings.iter().all(|w| !w.contains("bundled pack")));
}

#[test]
fn every_tree_in_an_analyze_trees_request_gets_its_own_pack_defs() {
    let mapped = config_to_request(&json!({"roots": ["./a", "./b"]}), Path::new("/base")).unwrap();
    let trees = mapped.request["trees"].as_array().unwrap();
    for tree in trees {
        assert_eq!(tree["packDefs"].as_array().unwrap().len(), 15);
    }
}

#[test]
fn a_bad_bundled_pack_source_becomes_a_warning_and_is_skipped() {
    let mut warnings = Vec::new();
    let defs = parse_pack_defs(
        &[("good.json", "{\"id\":\"g\"}"), ("bad.json", "not json")],
        &mut warnings,
    );
    assert_eq!(defs.len(), 1);
    assert!(warnings
        .iter()
        .any(|w| w.contains("\"bad.json\"") && w.contains("skipped")));
}

// --- representative config -> full request JSON deep-equal fixture -------------------------

#[test]
fn representative_config_maps_to_the_expected_request_shape() {
    let dir = TempDir::new("zzop-config-fixture");
    dir.write("overlay.json", r#"{"marker": "shared-overlay"}"#);

    let config = json!({
        "roots": ["."],
        "packs": {"extraDirs": ["./extra-packs"], "disabled": ["conventions"]},
        "rules": {
            "toctou": "off",
            "n-plus-one": {"severity": "warn", "exclude": ["legacy/", "**/*.gen.ts"]}
        },
        "exclude": ["vendor/"],
        "overlays": ["overlay.json"],
        "git": {"since": "2024-01-01", "recentDays": 14},
        "cacheDir": "./.cache",
        "sizeCap": 500000
    });

    let mapped = config_to_request(&config, dir.path()).unwrap();
    assert_eq!(mapped.method, Method::Analyze);

    let mut actual = mapped.request.clone();
    let pack_defs_len = actual["packDefs"].as_array().unwrap().len();
    actual.as_object_mut().unwrap().remove("packDefs");

    let expected = json!({
        "root": path_to_string(dir.path()),
        "packsDir": [path_to_string(&resolve_path(dir.path(), "./extra-packs"))],
        "disabledRules": ["conventions", "toctou"],
        "severityOverrides": {"n-plus-one": "warning"},
        "suppressions": [
            {"rule": "n-plus-one", "path": "legacy/"},
            {"rule": "n-plus-one", "glob": "**/*.gen.ts"}
        ],
        "globalExcludes": [{"path": "vendor/"}],
        "adapterOverlays": [{"marker": "shared-overlay"}],
        "git": {"since": "2024-01-01", "recentDays": 14},
        "cacheDir": path_to_string(&resolve_path(dir.path(), "./.cache")),
        "sizeCap": 500000
    });

    assert_eq!(actual, expected);
    assert_eq!(pack_defs_len, 15);
    assert!(mapped
        .warnings
        .iter()
        .all(|w| !w.contains("overlay") && !w.contains("unknown config key")));
}

// --- lexical path resolution sanity ---------------------------------------------------------

#[test]
fn resolve_path_normalizes_dot_and_dot_dot_segments() {
    let base = Path::new("/base/dir");
    assert_eq!(resolve_path(base, "."), base);
    assert_eq!(resolve_path(base, "./x"), base.join("x"));
    assert_eq!(resolve_path(base, "../sibling"), Path::new("/base/sibling"));
    assert_eq!(resolve_path(base, "a/../b"), base.join("b"));
}
