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
            "coverage",
            "critical",
            "degraded",
            "disclosure",
            "fileCount",
            "findings",
            "folders",
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
        ["crossLayer", "crossLayerFindings", "disclosure", "trees"],
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
