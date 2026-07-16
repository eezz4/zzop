//! End-to-end tool-summary tests over real temp trees (no config file -> zero-config defaults, which
//! inject the build.rs-embedded bundled packs as inline `packDefs` — so `packsLoaded` here reports
//! `source: "inline"` for every bundled pack).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, rel: &str, content: &str) {
        let full = self.0.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(full, content).unwrap();
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn assert_packs_loaded_entries(loaded: &serde_json::Value, context: &str) {
    let arr = loaded
        .as_array()
        .unwrap_or_else(|| panic!("{context}: packsLoaded must be an array, got: {loaded}"));
    assert!(
        !arr.is_empty(),
        "{context}: zero-config injects the bundled packs, so packsLoaded must be non-empty"
    );
    for p in arr {
        assert!(p["id"].is_string(), "{context}: entry missing id: {p}");
        assert!(p["rules"].is_u64(), "{context}: entry missing rules: {p}");
        assert_eq!(
            p["source"], "inline",
            "{context}: zero-config bundled packs arrive as inline packDefs"
        );
    }
    // Deterministic order: sorted by id.
    let ids: Vec<&str> = arr.iter().filter_map(|p| p["id"].as_str()).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(ids, sorted, "{context}: packsLoaded must be id-sorted");
}

/// Pins the `tools/list` surface: tool names, each schema's `required` array, and the
/// source-exclusivity `oneOf` constraints — so the schema surface cannot drift silently (it had
/// zero test coverage before this pin). Values, not just presence: a renamed tool, a dropped
/// `required` entry, or a loosened `oneOf` branch all fail here by name.
#[test]
fn tools_list_pins_names_required_arrays_and_source_exclusivity() {
    let list = super::list();
    let tools = list["tools"].as_array().expect("tools array");
    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    assert_eq!(
        names,
        [
            "analyze_repo",
            "cross_repo",
            "check_endpoint",
            "validate_envelope",
            "validate_rule_pack"
        ]
    );
    let schema = |name: &str| -> &serde_json::Value {
        &tools
            .iter()
            .find(|t| t["name"] == name)
            .unwrap_or_else(|| panic!("tool {name} listed"))["inputSchema"]
    };

    assert_eq!(
        schema("analyze_repo")["required"],
        serde_json::json!(["path"])
    );
    assert_eq!(
        schema("validate_envelope")["required"],
        serde_json::json!(["envelopeJson"])
    );
    assert_eq!(
        schema("validate_rule_pack")["required"],
        serde_json::json!(["packJson"])
    );

    // cross_repo: paths XOR configPath, expressed as `oneOf` (no top-level `required` — neither
    // source is individually required).
    let cross = schema("cross_repo");
    assert!(cross.get("required").is_none());
    assert_eq!(
        cross["oneOf"],
        serde_json::json!([{ "required": ["paths"] }, { "required": ["configPath"] }])
    );

    // check_endpoint: `pattern` always, plus exactly ONE of path/paths/configPath.
    let endpoint = schema("check_endpoint");
    assert_eq!(endpoint["required"], serde_json::json!(["pattern"]));
    assert_eq!(
        endpoint["oneOf"],
        serde_json::json!([
            { "required": ["pattern", "path"] },
            { "required": ["pattern", "paths"] },
            { "required": ["pattern", "configPath"] }
        ])
    );
}

#[test]
fn analyze_repo_summary_includes_packs_loaded_and_the_coverage_census() {
    let dir = TempDir::new("zzop-mcp-packs-loaded");
    dir.write("a.ts", "export const a = 1;\n");
    let out = super::analyze(&dir.path().display().to_string()).expect("analyze should succeed");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_packs_loaded_entries(&v["packsLoaded"], "analyze_repo");
    // The engine's per-tree coverage census must ride the summary — this fixture has files but
    // zero io, so the census carries the joinContributionZero BLIND assertion, and a summary that
    // dropped it would have the reader believe "0 findings, fine" about an io-invisible tree.
    let cov = v["coverage"]
        .as_object()
        .unwrap_or_else(|| panic!("analyze summary must carry coverage, got: {v}"));
    assert_eq!(cov["files"], 1, "got: {v}");
    assert_eq!(cov["ioProvides"], 0, "got: {v}");
    assert_eq!(cov["joinContributionZero"], true, "got: {v}");
}

/// A frontend tree whose only io is `fetch` consumes — one per given path (relative, so with no
/// providing tree they land in `unprovidedConsumes` as `GET <path>` keys, engine order = source order).
fn write_fetch_tree(dir: &TempDir, paths: &[String]) {
    let body: String = paths
        .iter()
        .enumerate()
        .map(|(i, p)| format!("export function call{i}() {{ return fetch('{p}'); }}\n"))
        .collect();
    dir.write("src/api.ts", &body);
}

#[test]
fn check_endpoint_single_path_gives_a_definitive_verdict_over_the_join() {
    let fe = TempDir::new("zzop-mcp-endpoint-single");
    write_fetch_tree(&fe, &["/api/users".to_string()]);
    // Single `path` mode still routes through analyzeTrees (the verdict vocabulary is join facts),
    // so a consume with no provider lands as consumed-unprovided — a definitive answer, not a count.
    let out = super::check_endpoint("users", Some(&fe.path().display().to_string()), &[], None)
        .expect("check_endpoint should succeed");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["verdict"], "consumed-unprovided");
    assert_eq!(v["counts"]["unprovidedConsumes"], 1);
    assert_eq!(
        v["matches"]["unprovidedConsumes"][0]["key"],
        "GET /api/users"
    );
    assert!(v["matches"]["unprovidedConsumes"][0]["file"].is_string());
    // Single-`path` mode names its one tree after the directory (mirroring paths mode's
    // zero_config_trees naming) — never the empty-string "unnamed tree" source tag.
    let dir_name = fe.path().file_name().unwrap().to_str().unwrap();
    assert_eq!(v["matches"]["unprovidedConsumes"][0]["source"], dir_name);
    assert!(
        v["disclosure"].is_array(),
        "disclosure forwarded from the analysis"
    );
}

#[test]
fn check_endpoint_not_found_suggests_and_requires_exactly_one_source() {
    let fe = TempDir::new("zzop-mcp-endpoint-notfound");
    write_fetch_tree(&fe, &["/api/users".to_string()]);
    let path = fe.path().display().to_string();
    let out = super::check_endpoint("/internal/users", Some(&path), &[], None).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["verdict"], "not-found");
    assert_eq!(v["suggestions"][0], "GET /api/users");

    let err = super::check_endpoint("users", None, &[], None).unwrap_err();
    assert!(err.contains("`path`"), "guided no-source error: {err}");
    let err =
        super::check_endpoint("users", Some(&path), std::slice::from_ref(&path), None).unwrap_err();
    assert!(
        err.contains("exactly ONE"),
        "guided multi-source error: {err}"
    );
}

#[test]
fn check_endpoint_config_path_single_tree_names_its_source_like_path_mode() {
    // configPath single-tree mode must not produce `source: ""` matches while the same tree
    // reached via `path` gets dir-named — the two entry modes answer identically.
    let fe = TempDir::new("zzop-mcp-endpoint-cfg-source");
    write_fetch_tree(&fe, &["/api/users".to_string()]);
    fe.write("zzop.config.jsonc", "{}");
    let cp = fe.path().join("zzop.config.jsonc").display().to_string();
    let out = super::check_endpoint("users", None, &[], Some(&cp)).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let expected = fe.path().file_name().unwrap().to_str().unwrap();
    assert_eq!(
        v["matches"]["unprovidedConsumes"][0]["source"], expected,
        "configPath single-tree matches must carry the dir-named source"
    );
}

#[test]
fn check_endpoint_carries_the_config_honesty_channels_like_every_sibling_tool() {
    // paths mode over a tree that HOLDS a zzop.config.jsonc: the config front-end's "paths mode
    // does NOT load it" disclosure must ride the reply as configWarnings (with config: null),
    // never be silently dropped — otherwise the caller believes their config was honored.
    let fe = TempDir::new("zzop-mcp-endpoint-honesty-fe");
    write_fetch_tree(&fe, &["/api/users".to_string()]);
    fe.write("zzop.config.jsonc", "{}");
    let be = TempDir::new("zzop-mcp-endpoint-honesty-be");
    write_fetch_tree(&be, &["/api/other".to_string()]);
    let paths = vec![
        fe.path().display().to_string(),
        be.path().display().to_string(),
    ];
    let out = super::check_endpoint("users", None, &paths, None).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(v["config"].is_null(), "paths mode honors no config file");
    let warnings = v["configWarnings"].as_array().expect("configWarnings");
    assert!(
        warnings
            .iter()
            .any(|w| w.as_str().unwrap_or_default().contains("does NOT load")),
        "ignored-config disclosure must survive: {warnings:?}"
    );
}

#[test]
fn cross_repo_summary_lists_bucket_keys_and_discloses_their_truncation() {
    let fe = TempDir::new("zzop-mcp-bucket-keys-fe");
    // 25 distinct unprovided consume keys: over the 20-key bucketKeys cap by 5.
    let over = crate::output::DEFAULT_BUCKET_KEYS_LIMIT + 5;
    let paths: Vec<String> = (0..over).map(|i| format!("/api/things/{i}")).collect();
    write_fetch_tree(&fe, &paths);
    let be = TempDir::new("zzop-mcp-bucket-keys-be");
    be.write("b.ts", "export const b = 2;\n");
    let roots = vec![
        fe.path().display().to_string(),
        be.path().display().to_string(),
    ];
    let out = super::cross_repo(&roots, None).expect("cross_repo should succeed");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let keys = v["bucketKeys"]["unprovidedConsumes"]
        .as_array()
        .expect("bucketKeys array");
    assert_eq!(keys.len(), crate::output::DEFAULT_BUCKET_KEYS_LIMIT);
    assert_eq!(keys[0], "GET /api/things/0", "engine order preserved");
    assert_eq!(v["bucketKeysTruncated"]["unprovidedConsumes"], 5);
    // Un-capped buckets appear with their (empty) key lists and no truncation entry.
    assert_eq!(v["bucketKeys"]["unconsumedProvides"], serde_json::json!([]));
    assert!(v["bucketKeysTruncated"].get("unconsumedProvides").is_none());
}

#[test]
fn validate_rule_pack_tool_reports_shape_verdicts_and_never_is_error_on_bad_input() {
    // A structurally valid pack (a real bundled one) -> {valid: true, issues: []}.
    let bundled = zzop_config::BUNDLED_PACK_SOURCES[0].1;
    let params = serde_json::json!({
        "name": "validate_rule_pack",
        "arguments": { "packJson": bundled }
    });
    let reply = super::call(Some(&params));
    assert!(reply.get("isError").is_none(), "got: {reply}");
    let report: serde_json::Value =
        serde_json::from_str(reply["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(report["valid"], true, "got: {report}");

    // A broken pack (missing `rules`) -> a NORMAL reply carrying {valid: false, issues: [named]},
    // not an isError — invalid input is the tool's answer, not its failure.
    let params = serde_json::json!({
        "name": "validate_rule_pack",
        "arguments": { "packJson": "{\"id\": \"p\"}" }
    });
    let reply = super::call(Some(&params));
    assert!(reply.get("isError").is_none(), "got: {reply}");
    let report: serde_json::Value =
        serde_json::from_str(reply["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(report["valid"], false, "got: {report}");
    assert!(
        report["issues"][0]
            .as_str()
            .unwrap()
            .contains("missing field `rules`"),
        "got: {report}"
    );

    // A missing `packJson` argument IS a tool-level error (the caller's call shape is wrong).
    let params = serde_json::json!({ "name": "validate_rule_pack", "arguments": {} });
    let reply = super::call(Some(&params));
    assert_eq!(reply["isError"], true, "got: {reply}");
}

#[test]
fn cross_repo_summary_includes_per_source_packs_loaded_and_coverage() {
    // fe extracts io (fetch consumes); be analyzes a file but contributes ZERO io to the join —
    // the engine asserts that (`joinContributionZero`), and the per-source summary entry must
    // surface it: a blind reader of "N findings, fine" for an io-invisible tree is exactly the
    // silent failure this project exists to disclose.
    let fe = TempDir::new("zzop-mcp-packs-loaded-fe");
    write_fetch_tree(&fe, &["/api/users".to_string()]);
    let be = TempDir::new("zzop-mcp-packs-loaded-be");
    be.write("b.ts", "export const b = 2;\n");
    let paths = vec![
        fe.path().display().to_string(),
        be.path().display().to_string(),
    ];
    let out = super::cross_repo(&paths, None).expect("cross_repo should succeed");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let sources = v["sources"].as_array().expect("sources array");
    assert_eq!(sources.len(), 2);
    for s in sources {
        assert_packs_loaded_entries(&s["packsLoaded"], "cross_repo source");
        assert!(
            s["coverage"].is_object(),
            "every source entry must carry its coverage census, got: {s}"
        );
    }
    let by_dir = |dir: &TempDir| {
        let name = dir.path().file_name().unwrap().to_str().unwrap();
        sources
            .iter()
            .find(|s| s["sourceId"] == name)
            .unwrap_or_else(|| panic!("source {name} listed"))
    };
    assert_eq!(by_dir(&fe)["coverage"]["joinContributionZero"], false);
    assert_eq!(by_dir(&fe)["coverage"]["ioConsumesKeyed"], 1);
    assert_eq!(
        by_dir(&be)["coverage"]["joinContributionZero"],
        true,
        "the no-io tree's blind assertion must be visible in the summary"
    );
}

#[test]
fn cross_repo_paths_mode_discloses_unanalyzed_sibling_directories() {
    // The live-fire gap: a monorepo's e2e/ tree was never passed to the join and nothing said so.
    // Both analyzed roots share one parent, so the parent's other subdirectories are enumerated
    // (sorted, dot-dirs and node_modules excluded) as a configWarnings entry.
    let parent = TempDir::new("zzop-mcp-sibling-scope");
    parent.write(
        "fe/src/api.ts",
        "export const a = () => fetch('/api/users');\n",
    );
    parent.write("be/src/b.ts", "export const b = 2;\n");
    for name in ["e2e", "docs-site"] {
        fs::create_dir_all(parent.path().join(name)).unwrap();
    }
    fs::create_dir_all(parent.path().join(".hidden")).unwrap();
    fs::create_dir_all(parent.path().join("node_modules")).unwrap();
    let paths = vec![
        parent.path().join("fe").display().to_string(),
        parent.path().join("be").display().to_string(),
    ];
    let out = super::cross_repo(&paths, None).expect("cross_repo should succeed");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let warnings = v["configWarnings"].as_array().expect("configWarnings");
    let w = warnings
        .iter()
        .filter_map(|w| w.as_str())
        .find(|w| w.contains("sibling"))
        .unwrap_or_else(|| panic!("sibling disclosure must ride configWarnings: {warnings:?}"));
    assert!(
        w.contains("2 sibling directories under") && w.contains(": docs-site, e2e — "),
        "sorted sibling names, dot-dirs/node_modules excluded: {w}"
    );
    assert!(
        w.ends_with("pass them as paths or add them to the config's trees"),
        "got: {w}"
    );
}

#[test]
fn cross_repo_config_mode_discloses_unanalyzed_sibling_directories() {
    // Same disclosure through the config-first mode: the config's trees resolve to absolute roots
    // under one parent, and the unanalyzed e2e/ sibling is named.
    let parent = TempDir::new("zzop-mcp-sibling-config");
    parent.write(
        "fe/src/api.ts",
        "export const a = () => fetch('/api/users');\n",
    );
    parent.write("be/src/b.ts", "export const b = 2;\n");
    fs::create_dir_all(parent.path().join("e2e")).unwrap();
    parent.write(
        "zzop.config.jsonc",
        "{\n  \"trees\": [\n    { \"root\": \"./fe\", \"sourceId\": \"fe\" },\n    { \"root\": \"./be\", \"sourceId\": \"be\" }\n  ]\n}\n",
    );
    let cp = parent
        .path()
        .join("zzop.config.jsonc")
        .display()
        .to_string();
    let out = super::cross_repo(&[], Some(&cp)).expect("cross_repo should succeed");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let warnings = v["configWarnings"].as_array().expect("configWarnings");
    let w = warnings
        .iter()
        .filter_map(|w| w.as_str())
        .find(|w| w.contains("sibling"))
        .unwrap_or_else(|| panic!("sibling disclosure must ride configWarnings: {warnings:?}"));
    assert!(
        w.contains("1 sibling directory under") && w.contains("is not part of this join: e2e"),
        "got: {w}"
    );
}
