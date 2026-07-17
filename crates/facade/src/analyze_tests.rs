//! Unit tests for the `analyze`/`analyzeTrees` entry points (`crate::analyze`).

use crate::test_support::{cycle_and_git_fixture, cycle_fixture, git_available, TempDir};
use crate::{analyze_json, analyze_trees_json};

/// Deterministic-output contract (same input -> byte-identical output): `ir.dep`, `ir.loc`, and
/// `nodes[].tagCounts` are `HashMap`-backed (hasher-randomized iteration order per process), so
/// without explicit ordering two identical `analyze()` calls could emit byte-different JSON purely
/// from map key ordering. `serde_json::to_value`-based equality would NOT catch this
/// (`serde_json::Value`'s `Map` is a `BTreeMap`, so `to_value` silently re-sorts keys) — the
/// assertion compares raw serialized strings, the only way to observe key order.
#[test]
fn analyze_json_is_byte_identical_across_two_runs() {
    if !git_available() {
        eprintln!("skipping analyze_json_is_byte_identical_across_two_runs: git not on PATH");
        return;
    }
    let dir = cycle_and_git_fixture();
    let config = format!(
        r#"{{"root": {:?}, "sourceId": "t", "git": {{}}}}"#,
        dir.path().display()
    );

    let out1 = analyze_json(&config).expect("analyze_json run 1 should succeed");
    let out2 = analyze_json(&config).expect("analyze_json run 2 should succeed");

    // Sanity: the fixture actually exercises multi-key maps, so this test would have failed before the
    // determinism fix (not vacuously passing on empty/single-key maps).
    let value: serde_json::Value = serde_json::from_str(&out1).expect("valid JSON");
    let dep_keys = value["ir"]["dep"].as_object().expect("ir.dep object").len();
    assert!(
        dep_keys >= 2,
        "expected ir.dep to have 2+ keys, got: {value}"
    );
    let a_tag_counts = value["nodes"]
        .as_array()
        .expect("nodes array")
        .iter()
        .find(|n| n["path"] == "a.ts")
        .expect("a.ts node")["tagCounts"]
        .as_object()
        .expect("tagCounts object")
        .len();
    assert!(
        a_tag_counts >= 3,
        "expected a.ts tagCounts to have 3+ keys, got: {value}"
    );

    assert_eq!(
        out1, out2,
        "analyze() must return byte-identical JSON across repeated runs on unchanged input"
    );
}

#[test]
fn analyze_json_round_trips_a_cycle_fixture() {
    let dir = cycle_fixture();
    let config = format!(r#"{{"root": {:?}, "sourceId": "t"}}"#, dir.path().display());
    let out = analyze_json(&config).expect("analyze_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    let findings = value["findings"].as_array().expect("findings array");
    assert!(
        findings.iter().any(|f| f["ruleId"] == "circular"),
        "expected a circular finding, got: {value}"
    );
    assert_eq!(value["fileCount"], 2);
}

#[test]
fn analyze_json_emits_a_camelcase_coverage_census() {
    // The cycle fixture has 2 mutually-importing files with exported functions and NO io, so it is
    // the canonical `joinContributionZero` case: files > 0, but 0 provides / 0 consumes.
    let dir = cycle_fixture();
    let config = format!(r#"{{"root": {:?}, "sourceId": "t"}}"#, dir.path().display());
    let out = analyze_json(&config).expect("analyze_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    let cov = value["coverage"].as_object().expect("coverage object");
    assert_eq!(cov["files"], 2);
    assert_eq!(cov["ioProvides"], 0);
    assert_eq!(cov["ioConsumesKeyed"], 0);
    assert_eq!(cov["ioConsumesUnresolved"], 0);
    assert_eq!(cov["joinContributionZero"], true);
    // Symbols and import edges are populated (a <-> b cycle over two exported functions).
    assert!(cov["symbols"].as_u64().expect("symbols number") >= 2);
    assert!(cov["importEdges"].as_u64().expect("importEdges number") >= 2);
    assert_eq!(cov["degraded"], 0);
}

#[test]
fn analyze_json_emits_the_disclosure_registry_at_the_root() {
    let dir = cycle_fixture();
    let config = format!(r#"{{"root": {:?}, "sourceId": "t"}}"#, dir.path().display());
    let out = analyze_json(&config).expect("analyze_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    let reg = value["disclosure"]
        .as_array()
        .expect("disclosure array at root");
    assert_eq!(reg.len(), zzop_engine::blindness_registry().len());
    // Each entry is camelCase {id, group, summary, status} with a known status token.
    for entry in reg {
        assert!(entry["id"].is_string());
        assert!(entry["group"].is_string());
        assert!(entry["summary"].is_string());
        let status = entry["status"].as_str().expect("status string");
        assert!(matches!(status, "asserted" | "partial" | "notYetDetected"));
    }
    // The Stage-1 signal is registered as an asserted class.
    let consume_side = reg
        .iter()
        .find(|e| e["id"] == "consume-side-unextracted")
        .expect("consume-side-unextracted registered");
    assert_eq!(consume_side["status"], "asserted");
    // The single-tree flatten kept the prior root fields intact alongside `disclosure`.
    assert_eq!(value["fileCount"], 2);
    assert!(value["coverage"].is_object());
}

/// Exact top-level key-set pins for both output roots. Every field is already pinned individually
/// by some test, but only an EXACT set assertion catches a future field being ADDED unpinned (or
/// renamed) — individual pins keep passing right through that. `serde_json`'s `Map` is a `BTreeMap`
/// here (no `preserve_order` feature), so `.keys()` is alphabetically sorted and the expected lists
/// below are written in that order.
#[test]
fn analyze_json_top_level_key_set_is_pinned_exactly() {
    let dir = cycle_fixture();
    let config = format!(r#"{{"root": {:?}, "sourceId": "t"}}"#, dir.path().display());
    let out = analyze_json(&config).expect("analyze_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    let keys: Vec<&str> = value
        .as_object()
        .expect("root object")
        .keys()
        .map(String::as_str)
        .collect();
    // `AnalyzeOutputView`'s fields (flattened by `SingleTreeOutputView`) plus `disclosure`.
    assert_eq!(
        keys,
        [
            "cache",
            "configWarnings",
            "coverage",
            "critical",
            "degraded",
            "disclosure",
            "fileCount",
            "findings",
            "folders",
            "gitWindow",
            "health",
            "ir",
            "layerCoChurn",
            "nodes",
            "packsLoaded",
            "recommendations",
            "ruleTimings",
            "scores",
            "seams",
            "warnings",
        ],
        "single-tree output root keys drifted — pin the new/renamed field here AND in its own test"
    );
}

#[test]
fn analyze_trees_json_top_level_key_set_is_pinned_exactly() {
    let dir = cycle_fixture();
    let config = format!(
        r#"{{"trees": [{{"root": {:?}, "sourceId": "t"}}]}}"#,
        dir.path().display()
    );
    let out = analyze_trees_json(&config).expect("analyze_trees_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    let keys: Vec<&str> = value
        .as_object()
        .expect("root object")
        .keys()
        .map(String::as_str)
        .collect();
    // `MultiAnalyzeOutputView`'s fields.
    assert_eq!(
        keys,
        [
            "crossLayer",
            "crossLayerFindings",
            "disclosure",
            "trees",
            "warnings"
        ],
        "multi-tree output root keys drifted — pin the new/renamed field here AND in its own test"
    );
    // Each `trees[]` entry (`TreeEntryView`) is part of the same wire surface — pinned with it.
    let entry_keys: Vec<&str> = value["trees"][0]
        .as_object()
        .expect("tree entry object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(entry_keys, ["output", "root", "sourceId"]);
}

#[test]
fn analyze_json_severity_overrides_remap_a_finding_severity() {
    // `circular` defaults to `warning` (rules-graph). A `severityOverrides` request entry must
    // promote it to `critical` on the way through `base_engine_config` -> `RuleConfig` ->
    // `merge_findings`'s `apply_severity_override`.
    let dir = cycle_fixture();
    let config = format!(
        r#"{{"root": {:?}, "severityOverrides": {{"circular": "critical"}}}}"#,
        dir.path().display()
    );
    let out = analyze_json(&config).expect("analyze_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    let findings = value["findings"].as_array().expect("findings array");
    let circular = findings
        .iter()
        .find(|f| f["ruleId"] == "circular")
        .expect("expected a circular finding");
    assert_eq!(
        circular["severity"], "critical",
        "severityOverrides must remap circular warning -> critical, got: {value}"
    );
}

/// A typo'd `disabledRules`/`severityOverrides` entry is a config problem — the self-report must land
/// in `configWarnings`, never `warnings` (2026-07-17: a blind agent checking `configWarnings` alone used
/// to see `[]` and wrongly conclude the typo went undetected).
#[test]
fn analyze_json_unknown_rule_overrides_land_in_config_warnings_not_warnings() {
    let dir = cycle_fixture();
    let config = format!(
        r#"{{"root": {:?}, "disabledRules": ["circular-typo"], "severityOverrides": {{"n-plus-one-typo": "critical"}}}}"#,
        dir.path().display()
    );
    let out = analyze_json(&config).expect("analyze_json should succeed");
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
            .any(|w| w.contains("disabled rules") && w.contains("circular-typo")),
        "expected the unknown-disabled-rule-id self-report in configWarnings, got: {config_warnings:?}"
    );
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
fn analyze_json_rule_overrides_applied_lists_only_the_id_that_actually_matched() {
    // D13③: `disabledRules` carries a real id (`circular`) AND a typo (`no-such-rule-typo`).
    // `ruleOverridesApplied.disabled` must name only the real one — the typo appears in NEITHER list,
    // only in the pre-existing unknown-id diagnostic (`warnings`).
    let dir = cycle_fixture();
    let config = format!(
        r#"{{"root": {:?}, "disabledRules": ["circular", "no-such-rule-typo"]}}"#,
        dir.path().display()
    );
    let out = analyze_json(&config).expect("analyze_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    let applied = &value["ruleOverridesApplied"];
    assert_eq!(
        applied["disabled"],
        serde_json::json!(["circular"]),
        "expected only the real id in ruleOverridesApplied.disabled, got: {value}"
    );
    assert_eq!(
        applied["severityRemapped"],
        serde_json::json!([]),
        "expected an empty severityRemapped (no severityOverrides requested), got: {value}"
    );
}

#[test]
fn analyze_json_omits_rule_overrides_applied_when_nothing_was_requested() {
    // The quieter of the two documented conventions (`zzop_engine::RuleOverridesApplied`'s doc): a
    // caller who never touched `disabledRules`/`severityOverrides` sees no `ruleOverridesApplied` key
    // at all, not an always-present empty object.
    let dir = cycle_fixture();
    let config = format!(r#"{{"root": {:?}}}"#, dir.path().display());
    let out = analyze_json(&config).expect("analyze_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    assert!(
        value.get("ruleOverridesApplied").is_none(),
        "expected no ruleOverridesApplied key when nothing was requested, got: {value}"
    );
}

#[test]
fn analyze_json_suppressions_drop_a_finding() {
    // A `suppressions` request entry for `circular` (no path) must drop the finding entirely via
    // `merge_findings`'s `is_suppressed` filter — the same fixture would otherwise emit one.
    let dir = cycle_fixture();
    let config = format!(
        r#"{{"root": {:?}, "suppressions": [{{"rule": "circular"}}]}}"#,
        dir.path().display()
    );
    let out = analyze_json(&config).expect("analyze_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    let findings = value["findings"].as_array().expect("findings array");
    assert!(
        !findings.iter().any(|f| f["ruleId"] == "circular"),
        "suppressions must drop the circular finding, got: {value}"
    );
}

#[test]
fn analyze_json_global_excludes_drop_a_finding_from_any_rule() {
    // A top-level `globalExcludes` request entry with a glob matching every file in the fixture must
    // drop the `circular` finding, exactly like a per-rule suppression would — but rule-agnostically
    // (no `rule` field on the entry at all).
    let dir = cycle_fixture();
    let config = format!(
        r#"{{"root": {:?}, "globalExcludes": [{{"glob": "**/*.ts"}}]}}"#,
        dir.path().display()
    );
    let out = analyze_json(&config).expect("analyze_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    let findings = value["findings"].as_array().expect("findings array");
    assert!(
        !findings.iter().any(|f| f["ruleId"] == "circular"),
        "globalExcludes must drop the circular finding, got: {value}"
    );
}

#[test]
fn analyze_json_rejects_invalid_json_without_panicking() {
    let err = analyze_json("not json").unwrap_err();
    assert!(err.contains("invalid analyze() config JSON"));
}

#[test]
fn analyze_json_rejects_missing_root() {
    let err = analyze_json(r#"{"sourceId": "t"}"#).unwrap_err();
    assert!(err.contains("root"));
}

/// D14 pin (`analyze` path): a request with a missing `sourceId` names its tree after the root's
/// directory basename at the facade chokepoint (`apply_source_id_default`) — observed through the
/// engine's overlay source-mismatch warning, which must name the defaulted id (the same id query
/// output shows), never the unnamed tree `""`.
#[test]
fn analyze_json_defaults_an_empty_source_id_to_the_root_dir_name() {
    let dir = cycle_fixture();
    let basename = dir
        .path()
        .file_name()
        .and_then(|s| s.to_str())
        .expect("fixture dir has a UTF-8 basename")
        .to_string();
    // A mismatched-source overlay WITH io facts is exactly the shape that makes the engine name the
    // tree's source_id in a warning (the blind-field-test D14 confusion this defaults away).
    let config = format!(
        r#"{{"root": {:?}, "adapterOverlays": [{{"format": "zzop-normalized-ast", "version": 1, "parser": "adapter-x/1", "source": "not-this-tree", "files": [{{"path": "external/x.jsp", "loc": 1, "io": {{"provides": [{{"kind": "http", "key": "GET /x", "file": "external/x.jsp", "line": 1}}], "consumes": []}}}}]}}]}}"#,
        dir.path().display()
    );
    let out = analyze_json(&config).expect("analyze_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    let mismatch = value["warnings"]
        .as_array()
        .expect("warnings array")
        .iter()
        .filter_map(|w| w.as_str())
        .find(|w| w.contains("declares a different source"))
        .expect("expected an overlay source-mismatch warning");
    assert!(
        mismatch.contains(&format!("\"{basename}\"")),
        "the warning must name the defaulted (dir-basename) tree id, got: {mismatch}"
    );
    assert!(
        !mismatch.contains("\"\""),
        "the unnamed-tree id \"\" must never appear, got: {mismatch}"
    );
}

/// D14 pin (`analyzeTrees` path): each entry with a missing `sourceId` gets the same dir-basename
/// default, echoed in the entry's own `sourceId` output field — while an explicit `sourceId` is
/// never overridden.
#[test]
fn analyze_trees_json_defaults_each_empty_source_id_and_never_overrides_an_explicit_one() {
    let unnamed = TempDir::new("zzop-facade-unnamed");
    unnamed.write("a.ts", "export const x = 1;\n");
    let named = TempDir::new("zzop-facade-named");
    named.write("b.ts", "export const y = 1;\n");

    let config = format!(
        r#"{{"trees": [{{"root": {:?}}}, {{"root": {:?}, "sourceId": "explicit"}}]}}"#,
        unnamed.path().display(),
        named.path().display()
    );
    let out = analyze_trees_json(&config).expect("analyze_trees_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");

    let expected = unnamed
        .path()
        .file_name()
        .and_then(|s| s.to_str())
        .expect("fixture dir has a UTF-8 basename");
    assert_eq!(
        value["trees"][0]["sourceId"], expected,
        "an unnamed tree must be named after its root's directory basename"
    );
    assert_eq!(
        value["trees"][1]["sourceId"], "explicit",
        "an explicit sourceId must never be overridden"
    );
}

#[test]
fn analyze_trees_json_joins_two_trees_and_rejects_empty_input() {
    let fe = TempDir::new("zzop-facade-fe");
    fe.write(
        "client.ts",
        "import axios from 'axios';\nexport const load = () => axios.get('/api/users');\n",
    );
    let be = TempDir::new("zzop-facade-be");
    be.write(
        "server.ts",
        "import { apiRoutes } from './router';\napiRoutes.get('/api/users', () => {});\n",
    );

    let config = format!(
        r#"{{"trees": [{{"root": {:?}, "sourceId": "fe"}}, {{"root": {:?}, "sourceId": "be"}}]}}"#,
        fe.path().display(),
        be.path().display()
    );
    let out = analyze_trees_json(&config).expect("analyze_trees_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(value["trees"].as_array().unwrap().len(), 2);
    assert!(value["crossLayer"].is_object());
    // `cross-layer/*` native rule findings — camelCase-keyed like every other output field (see
    // `MultiAnalyzeOutputView::cross_layer_findings`'s doc). This fixture's single matching route has no
    // duplicate/mismatch/skew/near-miss/shared-table signal, so an empty array (not absent, not null) is
    // the honest result here.
    assert_eq!(value["crossLayerFindings"].as_array().unwrap().len(), 0);

    let empty_err = analyze_trees_json(r#"{"trees": []}"#).unwrap_err();
    assert!(empty_err.contains("at least one entry"));
}

/// A blind field test passed an envelope JSON FILE's own path as an analysis root: the walk treated
/// the file as a bogus empty directory, so the output claimed "0 files" in `warnings` while
/// `coverage.files`/`fileCount` read 1 (the file itself, picked up as the walk's sole entry) — a
/// self-contradictory result. `analyze_json` must reject a non-directory root outright instead.
#[test]
fn analyze_json_rejects_a_file_path_as_root() {
    let dir = TempDir::new("zzop-facade-file-as-root");
    dir.write("envelope.json", r#"{"format": "zzop-normalized-ast"}"#);
    let file_path = dir.path().join("envelope.json");
    let config = format!(r#"{{"root": {:?}}}"#, file_path.display());
    let err = analyze_json(&config).unwrap_err();
    assert!(
        err.contains(&format!("root is not a directory: {}", file_path.display())),
        "got: {err}"
    );
    assert!(
        err.contains("validate_envelope's input, not an analysis root"),
        "got: {err}"
    );
}

/// Same gate, `analyzeTrees` path — the second entry of a two-tree request names a file; even though
/// the first entry is a perfectly good directory, the whole call must reject (fail fast, not silently
/// drop the bad tree).
#[test]
fn analyze_trees_json_rejects_a_file_path_as_a_tree_root() {
    let good = TempDir::new("zzop-facade-trees-good-root");
    good.write("a.ts", "export const x = 1;\n");
    let bad = TempDir::new("zzop-facade-trees-bad-root");
    bad.write("envelope.json", r#"{"format": "zzop-normalized-ast"}"#);
    let file_path = bad.path().join("envelope.json");

    let config = format!(
        r#"{{"trees": [{{"root": {:?}, "sourceId": "good"}}, {{"root": {:?}, "sourceId": "bad"}}]}}"#,
        good.path().display(),
        file_path.display()
    );
    let err = analyze_trees_json(&config).unwrap_err();
    assert!(
        err.contains(&format!("root is not a directory: {}", file_path.display())),
        "got: {err}"
    );
}

/// The nonexistent-root case is deliberately UNTOUCHED by the new gate: `analyze_tree`'s own leading
/// scope-warning self-report already behaves sanely for it (see
/// `zzop_engine`'s `nonexistent_root_self_reports_as_the_leading_warning`), so `analyze_json` must
/// still succeed (not error) and report the tree as empty, exactly as before this fix.
#[test]
fn analyze_json_keeps_the_nonexistent_root_warning_behavior() {
    let dir = TempDir::new("zzop-facade-nonexistent-root-base");
    let missing = dir.path().join("does-not-exist-anywhere");
    let config = format!(r#"{{"root": {:?}}}"#, missing.display());
    let out = analyze_json(&config).expect("a merely nonexistent root must not error");
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    assert_eq!(value["fileCount"], 0);
    assert!(value["warnings"]
        .as_array()
        .expect("warnings array")
        .iter()
        .filter_map(|w| w.as_str())
        .any(|w| w.contains("does not exist or is not a directory")));
}

/// `gitWindow` — the operative `recentDays`/`since` git-window knobs (`zzop_engine::AnalyzeOutput::
/// git_window`'s doc): neither knob was echoed anywhere before this field existed, so a consumer
/// diffing two runs' `scores`/`health` numbers could not tell which window produced which output.

#[test]
fn analyze_json_git_window_echoes_the_resolved_default_recent_days() {
    if !git_available() {
        eprintln!("skipping analyze_json_git_window_echoes_the_resolved_default_recent_days: git not on PATH");
        return;
    }
    let dir = cycle_and_git_fixture();
    let config = format!(
        r#"{{"root": {:?}, "sourceId": "t", "git": {{}}}}"#,
        dir.path().display()
    );
    let out = analyze_json(&config).expect("analyze_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    assert_eq!(
        value["gitWindow"],
        serde_json::json!({"recentDays": 30, "since": null}),
        "an unset recentDays must echo the resolved default (30), got: {value}"
    );
}

#[test]
fn analyze_json_git_window_echoes_an_explicit_recent_days() {
    if !git_available() {
        eprintln!(
            "skipping analyze_json_git_window_echoes_an_explicit_recent_days: git not on PATH"
        );
        return;
    }
    let dir = cycle_and_git_fixture();
    let config = format!(
        r#"{{"root": {:?}, "sourceId": "t", "git": {{"recentDays": 7}}}}"#,
        dir.path().display()
    );
    let out = analyze_json(&config).expect("analyze_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    assert_eq!(
        value["gitWindow"]["recentDays"], 7,
        "explicit recentDays must be echoed verbatim, got: {value}"
    );
}

#[test]
fn analyze_json_git_window_echoes_since() {
    if !git_available() {
        eprintln!("skipping analyze_json_git_window_echoes_since: git not on PATH");
        return;
    }
    let dir = cycle_and_git_fixture();
    let config = format!(
        r#"{{"root": {:?}, "sourceId": "t", "git": {{"since": "2020-01-01"}}}}"#,
        dir.path().display()
    );
    let out = analyze_json(&config).expect("analyze_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    assert_eq!(
        value["gitWindow"]["since"], "2020-01-01",
        "an explicit since must be echoed verbatim, got: {value}"
    );
}

#[test]
fn analyze_json_git_window_is_absent_when_git_did_not_run() {
    let dir = cycle_fixture();
    let config = format!(r#"{{"root": {:?}, "sourceId": "t"}}"#, dir.path().display());
    let out = analyze_json(&config).expect("analyze_json should succeed");
    let value: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    assert!(
        value["gitWindow"].is_null(),
        "gitWindow must be null when EngineConfig::git was None, got: {value}"
    );
}
