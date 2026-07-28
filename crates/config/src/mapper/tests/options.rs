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

// --- default discovery of the user-authored `zzop/rules/` ------------------------------------

/// The authored half of the on-disk split: a repo that simply puts its packs in `zzop/rules/` gets
/// them loaded with no config key at all — the point of the default. `.zzop/` (derived) is one
/// character away and must never be what gets discovered, so the negative half is pinned here too.
#[test]
fn an_undeclared_extra_dirs_discovers_the_authored_zzop_rules_directory() {
    let dir = TempDir::new("zzop-config-authored-packs");
    dir.mkdir(crate::DEFAULT_AUTHORED_PACKS_DIR);
    // The derived sibling exists at the same time — discovery must not confuse the two.
    dir.mkdir(".zzop/cache");

    let mapped = config_to_request(&json!({"roots": ["."]}), dir.path()).unwrap();
    let dirs = analyze_request(&mapped.request)["packsDir"]
        .as_array()
        .unwrap();
    assert_eq!(
        dirs,
        &[serde_json::Value::String(path_to_string(&resolve_path(
            dir.path(),
            crate::DEFAULT_AUTHORED_PACKS_DIR
        )))]
    );
}

/// Missing directory => nothing emitted AND nothing said. Almost every repo is in this state, so a
/// disclosure here would be a warning on every run of every project that authored no packs.
#[test]
fn a_base_without_an_authored_packs_directory_is_silent_not_warned() {
    let dir = TempDir::new("zzop-config-authored-packs-absent");
    let mapped = config_to_request(&json!({"roots": ["."]}), dir.path()).unwrap();
    assert!(analyze_request(&mapped.request).get("packsDir").is_none());
    assert!(
        !mapped.warnings.iter().any(|w| w.contains("zzop/rules")),
        "got: {:?}",
        mapped.warnings
    );
}

/// Fallback, never a merge — the "no second source of truth" constraint. A declared `extraDirs` is the
/// SOLE origin of `packsDir` even when the authored directory is sitting right there, so "where did
/// this pack come from" always has one answer. The empty array is the same rule, read as the explicit
/// opt-out it is: declaring nothing is not the same as declaring none.
#[test]
fn a_declared_extra_dirs_wins_over_the_authored_default_and_never_merges() {
    let dir = TempDir::new("zzop-config-authored-packs-explicit");
    dir.mkdir(crate::DEFAULT_AUTHORED_PACKS_DIR);
    dir.mkdir("vendor-packs");

    let mapped = config_to_request(
        &json!({"roots": ["."], "packs": {"extraDirs": ["./vendor-packs"]}}),
        dir.path(),
    )
    .unwrap();
    let dirs = analyze_request(&mapped.request)["packsDir"]
        .as_array()
        .unwrap();
    assert_eq!(
        dirs,
        &[serde_json::Value::String(path_to_string(&resolve_path(
            dir.path(),
            "./vendor-packs"
        )))],
        "an explicit extraDirs must be the only origin, not the authored default merged with it"
    );

    let opted_out = config_to_request(
        &json!({"roots": ["."], "packs": {"extraDirs": []}}),
        dir.path(),
    )
    .unwrap();
    assert!(
        analyze_request(&opted_out.request)
            .get("packsDir")
            .is_none(),
        "an empty extraDirs is an explicit \"no pack directories\", not an omission to fill"
    );
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

/// The zero-config write contract: a config that never mentions `cacheDir` still gets one, so a
/// zero-config run warms instead of being cold forever. Seals the DEFAULT ITSELF (an accidental revert
/// to "no key" is exactly the silent regression this default was introduced to remove) and that it is
/// `base_dir`-relative like every other path this mapper emits — an unresolved `.zzop/cache` would
/// land wherever the host process happened to be launched from.
#[test]
fn cache_dir_defaults_under_the_base_dir_when_the_key_is_absent() {
    let mapped = config_to_request(&json!({"roots": ["."]}), Path::new("/base")).unwrap();
    let req = analyze_request(&mapped.request);
    assert_eq!(
        req["cacheDir"],
        path_to_string(&resolve_path(
            Path::new("/base"),
            zzop_cache::DEFAULT_CACHE_DIR
        ))
    );
}

/// Every tree of a multi-tree request gets the default too — a monorepo join must not have half its
/// trees warm. (They share ONE directory, which is safe by construction: `CacheKey` carries the tree's
/// `sourceId` in its `scope`, so the path is not a key ingredient.)
#[test]
fn the_cache_dir_default_reaches_every_tree_of_a_multi_tree_request() {
    let mapped = config_to_request(&json!({"roots": ["./a", "./b"]}), Path::new("/base")).unwrap();
    let expected = path_to_string(&resolve_path(
        Path::new("/base"),
        zzop_cache::DEFAULT_CACHE_DIR,
    ));
    for tree in mapped.request["trees"].as_array().unwrap() {
        assert_eq!(tree["cacheDir"], expected);
    }
}

/// The opt-out. Now that omitting the key means "default on", SOMETHING has to mean "off" — this pins
/// what. A falsy value emits no `cacheDir` at all, i.e. byte-for-byte the request an omitted key
/// produced before the default existed, which is what makes the engine run uncached.
///
/// `""` is in this set on purpose and is the reason the check is falsiness rather than `null`-only: it
/// would otherwise resolve to `base_dir` ITSELF and scatter cache entries through the user's repo root.
#[test]
fn a_falsy_cache_dir_turns_the_cache_off_instead_of_naming_a_directory() {
    for off in [json!(null), json!(false), json!(""), json!(0)] {
        let mapped = config_to_request(
            &json!({"roots": ["."], "cacheDir": off}),
            Path::new("/base"),
        )
        .unwrap();
        assert!(
            analyze_request(&mapped.request).get("cacheDir").is_none(),
            "cacheDir: {off} must emit no cacheDir key"
        );
    }
}

/// The opt-out must not cost the author anything else: turning the cache off is not a way to
/// accidentally turn the bundled packs or git collection off too.
#[test]
fn turning_the_cache_off_leaves_the_other_defaults_alone() {
    let mapped = config_to_request(
        &json!({"roots": ["."], "cacheDir": null}),
        Path::new("/base"),
    )
    .unwrap();
    let req = analyze_request(&mapped.request);
    assert_eq!(req["git"], json!({}));
    assert_eq!(req["packDefs"].as_array().unwrap().len(), 12);
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
        12,
        "expected exactly the 12 bundled DSL packs"
    );
    assert!(mapped.warnings.iter().all(|w| !w.contains("bundled pack")));
}

#[test]
fn every_tree_in_an_analyze_trees_request_gets_its_own_pack_defs() {
    let mapped = config_to_request(&json!({"roots": ["./a", "./b"]}), Path::new("/base")).unwrap();
    let trees = mapped.request["trees"].as_array().unwrap();
    for tree in trees {
        assert_eq!(tree["packDefs"].as_array().unwrap().len(), 12);
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
    // Nothing is injected here any more (2026-07-27): a config that declares no `vocabulary` sends an
    // EMPTY one, and the engine reads that as "these judgments are not made" rather than as a request
    // for zzop's own names. Asserted rather than merely dropped, because a re-introduced injection would
    // otherwise pass this whole shape test silently. The values a user does get come from the starter
    // template, pinned against the owning constants in `template_tests.rs`.
    assert_eq!(
        actual.as_object_mut().unwrap().remove("vocabulary"),
        Some(json!({})),
        "a config declaring no vocabulary must forward an empty one, never zzop's built-ins"
    );

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
    assert_eq!(pack_defs_len, 12);
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

/// Seals that `git.commitSubjectPatterns` is a RECOGNIZED key that reaches the engine request: it must
/// draw no unknown-key drift warning (the knob dictionary lists it) and must ride through the `git`
/// pass-through verbatim, entry shape included. A key that warned, or that was accepted and forwarded
/// nowhere, is the "recognized but unwired" silent failure this dictionary exists to prevent.
#[test]
fn git_commit_subject_patterns_is_recognized_and_passes_through_verbatim() {
    let dir = TempDir::new("zzop-config-commit-subject-patterns");
    let git = json!({
        "commitSubjectPatterns": [
            {"pattern": r"^Revert\b", "label": "revert"},
            {"pattern": r"PROJ-\d+", "label": "ticket"}
        ]
    });
    let config = json!({"roots": ["."], "git": git.clone()});

    let mapped = config_to_request(&config, dir.path()).unwrap();
    assert_eq!(mapped.request["git"], git);
    assert!(
        mapped
            .warnings
            .iter()
            .all(|w| !w.contains("unknown config key")),
        "a dictionary-listed key must not drift-warn: {:?}",
        mapped.warnings
    );
}
