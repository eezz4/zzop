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
        r#"{"format": "zzop-normalized-ast", "version": "0.27.0"}"#,
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
    assert_eq!(overlays[0]["version"], "0.27.0");
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

/// `vocabulary` was the one declared scope the unknown-key walk never descended into, so a misspelled
/// or RETIRED vocabulary key was accepted in total silence — and a vocabulary key that is silently
/// ignored is the worst shape this config has, because an undeclared vocabulary makes rules fire MORE
/// (see `config-surface.json`'s merge-policy note). The rename of `fsd` -> `featureSlicedDesign` is what
/// forced this: without the walk, every config still carrying the old spelling would keep its FSD layer
/// names and get no hint that none of them reach a run any more.
///
/// Both levels are asserted, because the nested `featureSlicedDesign` scope has its own key list and a
/// walk that stopped one level up would still swallow `featureSlicedDesign.entrY`.
#[test]
fn unknown_key_warning_fires_inside_vocabulary_and_its_nested_scope() {
    let mapped = config_to_request(
        &json!({
            "roots": ["."],
            "vocabulary": {
                "fsd": {"entry": ["pages"]},
                "featureSlicedDesign": {"entry": ["pages"], "bogusNested": 1},
            },
        }),
        Path::new("/base"),
    )
    .unwrap();
    assert!(
        mapped
            .warnings
            .iter()
            .any(|w| w.contains("unknown config key \"vocabulary.fsd\"")),
        "the retired spelling must be named, not swallowed: {:?}",
        mapped.warnings
    );
    assert!(
        mapped.warnings.iter().any(
            |w| w.contains("unknown config key \"vocabulary.featureSlicedDesign.bogusNested\"")
        ),
        "the nested scope must be walked too: {:?}",
        mapped.warnings
    );
    assert!(
        mapped
            .warnings
            .iter()
            .all(|w| !w.contains("vocabulary.featureSlicedDesign.entry")),
        "a VALID nested key must stay silent: {:?}",
        mapped.warnings
    );
}

#[test]
fn unknown_key_warning_fires_inside_a_mounts_entry() {
    let mapped = config_to_request(
        &json!({"trees": [{"root": ".", "topology": {"mounts": [{"dir": "a", "at": "/a", "bogus": 1}]}}]}),
        Path::new("/base"),
    )
    .unwrap();
    assert!(mapped
        .warnings
        .iter()
        .any(|w| w.contains("unknown config key \"trees[0].topology.mounts[0].bogus\"")));
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

// --- retired keys: still not forwarded, and no longer silent about it ------------------------

/// `failOn`/`format`/`report` were recognized-but-inert for months: accepted here, forwarded into no
/// request, read by no binary. This pins BOTH halves of the 2026-07-26 retirement — they are still not
/// forwarded (that never changed), and each one now costs the author a warning that says it was removed
/// and what to do instead, rather than the silence that let someone believe they had configured a CI
/// gate. The generic unknown-key wording ("a typo, or a key from a different zzop version") would be a
/// wrong guess for a key that WAS valid, so its absence is asserted too.
#[test]
fn retired_presentation_keys_warn_as_removed_and_stay_unforwarded() {
    let mapped = config_to_request(
        &json!({"roots": ["."], "failOn": "critical", "format": "json", "report": {"dir": "out"}}),
        Path::new("/base"),
    )
    .unwrap();

    for (key, remedy) in [
        ("failOn", "gate a build by reading the severities"),
        ("format", "emits JSON and only JSON"),
        ("report", "no zzop binary writes report files"),
    ] {
        let warning = mapped
            .warnings
            .iter()
            .find(|w| w.contains(&format!("\"{key}\"")))
            .unwrap_or_else(|| panic!("no warning named {key}; got: {:?}", mapped.warnings));
        assert!(
            warning.contains("unknown config key") && warning.contains("REMOVED"),
            "warning for {key} must open like every other unknown key and then say it was removed: {warning}"
        );
        assert!(
            warning.contains(remedy),
            "warning for {key} must be actionable, not just a rejection: {warning}"
        );
        assert!(
            !warning.contains("a typo, or a key from a different zzop version"),
            "a retired key is not a typo — that guess must not appear: {warning}"
        );
    }

    // Walking INTO the retired `report` object would bury the one honest sentence under sub-key noise.
    assert!(
        !mapped.warnings.iter().any(|w| w.contains("report.dir")),
        "got: {:?}",
        mapped.warnings
    );

    let req = analyze_request(&mapped.request);
    assert!(req.get("failOn").is_none());
    assert!(req.get("format").is_none());
    assert!(req.get("report").is_none());
}

// --- vocabulary entries that can never match -----------------------------------------------------

/// The exact declaration that motivated this warning. `sensitive-response-field` normalizes the field
/// name it reads (lowercase, separators dropped) before the lookup, so a camelCase entry is inert — and
/// before 2026-08-05 nothing said so on any lane.
#[test]
fn a_vocabulary_entry_that_cannot_survive_its_own_normalizer_is_warned_with_the_spelling_that_works(
) {
    let mapped = config_to_request(
        &json!({
            "roots": ["."],
            "vocabulary": {"sensitiveResponseFieldExactNames": ["sessionToken"]},
        }),
        Path::new("/base"),
    )
    .unwrap();
    let hit = mapped
        .warnings
        .iter()
        .find(|w| w.contains("sensitiveResponseFieldExactNames"))
        .unwrap_or_else(|| panic!("expected a warning, got: {:?}", mapped.warnings));
    assert!(hit.contains("can never match"), "{hit}");
    assert!(
        hit.contains("\"sessiontoken\""),
        "the warning must name the spelling that WOULD work: {hit}"
    );
}

/// The HTTP convention spelling of this header is `Idempotency-Key`, so it is the entry a user is most
/// likely to declare in a form that cannot match. The separator is fine; only the case is not.
#[test]
fn the_conventional_http_header_spelling_is_warned_and_only_its_case_is_corrected() {
    let mapped = config_to_request(
        &json!({
            "roots": ["."],
            "vocabulary": {"idempotencyHeaderNames": ["Idempotency-Key"]},
        }),
        Path::new("/base"),
    )
    .unwrap();
    let hit = mapped
        .warnings
        .iter()
        .find(|w| w.contains("idempotencyHeaderNames"))
        .unwrap_or_else(|| panic!("expected a warning, got: {:?}", mapped.warnings));
    assert!(
        hit.contains("\"idempotency-key\""),
        "the hyphen is part of the header token and must survive: {hit}"
    );
}

/// The over-firing this check must not do. `secretParamNames` is compared with case folding ONLY, so
/// both separator spellings are legitimate entries — a blanket normalization would have merged two real
/// declarations into one and warned about a correct config.
#[test]
fn separator_spellings_are_not_warned_for_a_key_whose_rule_keeps_separators() {
    let mapped = config_to_request(
        &json!({
            "roots": ["."],
            "vocabulary": {"secretParamNames": ["api_key", "api-key", "apikey"]},
        }),
        Path::new("/base"),
    )
    .unwrap();
    assert!(
        mapped
            .warnings
            .iter()
            .all(|w| !w.contains("can never match")),
        "no entry here is unmatchable: {:?}",
        mapped.warnings
    );
}

/// A key the rules compare verbatim is not checked at all — the table is short on purpose, and a key
/// absent from it must stay silent rather than get a normalization it does not have.
#[test]
fn a_verbatim_compared_vocabulary_key_is_never_warned_about_case() {
    let mapped = config_to_request(
        &json!({
            "roots": ["."],
            "vocabulary": {"javaSourceRoot": "src/main/java", "retryWrappers": ["withRetry"]},
        }),
        Path::new("/base"),
    )
    .unwrap();
    assert!(
        mapped
            .warnings
            .iter()
            .all(|w| !w.contains("can never match")),
        "verbatim keys must not be normalized: {:?}",
        mapped.warnings
    );
}
