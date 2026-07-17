//! End-to-end tool-summary tests over real temp trees (no config file -> zero-config defaults, which
//! inject the build.rs-embedded bundled packs as inline `packDefs` — so `packsLoaded` here reports
//! `source: "inline"` for every bundled pack). These drive the REAL `tools/call` dispatch (or the
//! thin CLI-facing wrappers `super::analyze`/`super::cross_repo`/`super::check_endpoint`) end to end —
//! they pin the thin facade, not the shaping logic itself (that logic's own unit tests now live beside
//! it in `zzop-summary`, e.g. `zzop_summary`'s `config_warnings` module).

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
            "analyze_envelope",
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
        schema("analyze_envelope")["required"],
        serde_json::json!(["envelopeJson"])
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
    // The schema under-declared `pattern`'s non-emptiness — behavior already enforces it
    // (`zzop-facade`'s queryIo() rejects an empty pattern), the schema just never said so.
    assert_eq!(endpoint["properties"]["pattern"]["minLength"], 1);

    // `limit`'s schema minimum is 0 (not 1): `limit: 0` is a legal "counts only" query.
    assert_eq!(schema("analyze_repo")["properties"]["limit"]["minimum"], 0);
    assert_eq!(
        schema("analyze_repo")["properties"]["limit"]["maximum"],
        1000
    );
}

/// README-vs-tools-list drift pin: the tools table in `packages/mcp/README.md` went stale once
/// (`analyze_envelope` shipped without a row) with nothing to catch it — closes the same drift class
/// the surface-parity registry closes for output fields. Kept a simple name-presence substring check
/// (like the surface-parity JS test does for field names), not a full table-shape parser: the goal is
/// "a new tool that isn't in the README fails the build," not byte-parity with the markdown table.
#[test]
fn every_tool_name_from_tools_list_appears_in_the_readme() {
    const README: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md"));
    let list = super::list();
    let tools = list["tools"].as_array().expect("tools array");
    for tool in tools {
        let name = tool["name"].as_str().expect("tool name is a string");
        assert!(
            README.contains(name),
            "tool `{name}` from tools/list is missing from packages/mcp/README.md's tools table — \
             add a row (or the README will silently drift stale again)"
        );
    }
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

#[test]
fn analyze_repo_merges_facade_level_config_warnings_into_the_reply_channel() {
    // End-to-end over the real engine: a config disabling a rule id that matches nothing produces a
    // facade-level `configWarnings` diagnostic (moved out of the engine's `warnings` channel), and
    // the reply must merge it after the loader's own warnings — not silently drop it at this layer.
    let dir = TempDir::new("zzop-mcp-facade-config-warnings");
    dir.write("a.ts", "export const a = 1;\n");
    dir.write(
        "zzop.config.jsonc",
        "{ \"rules\": { \"no-such-rule-xyz\": \"off\" } }",
    );
    let out = super::analyze(&dir.path().display().to_string()).expect("analyze should succeed");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let warnings = v["configWarnings"].as_array().expect("configWarnings");
    assert!(
        warnings.iter().any(|w| {
            let w = w.as_str().unwrap_or_default();
            w.contains("no known rule id") && w.contains("no-such-rule-xyz")
        }),
        "the engine's unknown-rule-id diagnostic must survive into the reply's configWarnings: {warnings:?}"
    );
}

#[test]
fn analyze_repo_summary_caps_the_degraded_list_and_discloses_truncation() {
    // The live-fire gap: analyze_repo forwarded the FULL `degraded` path list verbatim, bypassing the
    // same cap/disclosure every other list gets — a token bomb on a large repo. A tiny `sizeCap`
    // forces every file here into the oversized lexical-fallback path, so all of them land in
    // `degraded`.
    let dir = TempDir::new("zzop-mcp-degraded-cap");
    let over = zzop_summary::output::DEFAULT_DEGRADED_LIMIT + 5;
    for i in 0..over {
        dir.write(&format!("f{i}.ts"), "export const a = 1;\n");
    }
    dir.write("zzop.config.jsonc", "{ \"sizeCap\": 1 }");
    let out = super::analyze(&dir.path().display().to_string()).expect("analyze should succeed");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let degraded = v["degraded"].as_array().expect("degraded array");
    assert_eq!(
        degraded.len(),
        zzop_summary::output::DEFAULT_DEGRADED_LIMIT,
        "degraded list must be capped like every other list, got: {v}"
    );
    assert_eq!(
        v["degradedTruncated"]["shown"],
        zzop_summary::output::DEFAULT_DEGRADED_LIMIT
    );
    // The full count survives uncapped as a scalar in the coverage census, and the truncation
    // disclosure's totalMatching agrees with it — the capped list is supplementary detail, never
    // the only source of the count. (>= `over`, not ==: the fixture's own zzop.config.jsonc may
    // itself degrade under the tiny sizeCap — the exact census is the engine's business.)
    let total = v["coverage"]["degraded"]
        .as_u64()
        .unwrap_or_else(|| panic!("coverage.degraded must be a number, got: {v}"));
    assert!(total >= over as u64, "got: {v}");
    assert_eq!(v["degradedTruncated"]["totalMatching"], total, "got: {v}");
}

#[test]
fn analyze_repo_summary_omits_rule_overrides_applied_when_the_engine_does_not_send_it() {
    // No disabledRules/severityOverrides requested here (zero-config, no config file) — the engine's
    // own contract is to OMIT `ruleOverridesApplied` in that case, and the host must forward that
    // omission as an absent key, never as JSON `null` noise (unlike `packsLoaded`, which the engine
    // always sends).
    let dir = TempDir::new("zzop-mcp-rule-overrides-absent");
    dir.write("a.ts", "export const a = 1;\n");
    let out = super::analyze(&dir.path().display().to_string()).expect("analyze should succeed");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(
        v.get("ruleOverridesApplied").is_none(),
        "ruleOverridesApplied must be an absent key, not null, when no overrides were requested: {v}"
    );
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
fn check_endpoint_one_path_in_paths_mode_names_itself_not_cross_repo() {
    // Live-fire misfire: `paths` mode's "at least 2 paths" error is built by `zero_config_trees`,
    // a helper shared with `cross_repo` — before the tool-name parameter, this error always said
    // "cross_repo" even when `check_endpoint` was the caller that actually got fewer than 2 paths.
    let fe = TempDir::new("zzop-mcp-endpoint-one-path");
    write_fetch_tree(&fe, &["/api/users".to_string()]);
    let err = super::check_endpoint(
        "users",
        None,
        std::slice::from_ref(&fe.path().display().to_string()),
        None,
    )
    .unwrap_err();
    assert!(
        err.contains("check_endpoint needs at least 2 paths"),
        "must name the actual caller, not a sibling tool: {err}"
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
    let over = zzop_summary::output::DEFAULT_BUCKET_KEYS_LIMIT + 5;
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
    assert_eq!(keys.len(), zzop_summary::output::DEFAULT_BUCKET_KEYS_LIMIT);
    assert_eq!(keys[0], "GET /api/things/0", "engine order preserved");
    assert_eq!(v["bucketKeysTruncated"]["unprovidedConsumes"], 5);
    // Un-capped buckets appear with their (empty) key lists and no truncation entry.
    assert_eq!(v["bucketKeys"]["unconsumedProvides"], serde_json::json!([]));
    assert!(v["bucketKeysTruncated"].get("unconsumedProvides").is_none());
    // `bucketKeySites` mirrors `bucketKeys` shape (same length after capping) with a locatable
    // "file:line" for the first site behind each key — a key is no longer a bare string with no
    // call site to go look at.
    let sites = v["bucketKeySites"]["unprovidedConsumes"]
        .as_array()
        .expect("bucketKeySites array");
    assert_eq!(sites.len(), keys.len(), "sites must be parallel to keys");
    let site0 = sites[0].as_str().unwrap_or_default();
    assert!(
        site0.contains("api.ts:") && site0.rsplit(':').next().unwrap().parse::<u32>().is_ok(),
        "expected a locatable \"file:line\" site, got: {site0:?}"
    );
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
        // Same omitted-not-null guard as analyze_repo (see
        // analyze_repo_summary_omits_rule_overrides_applied_when_the_engine_does_not_send_it):
        // no overrides were requested by either tree here.
        assert!(
            s.get("ruleOverridesApplied").is_none(),
            "ruleOverridesApplied must be an absent key, not null, when no overrides were \
             requested: {s}"
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
        w.contains("2 sibling directories under") && w.contains(": docs-site, e2e. Add them"),
        "sorted sibling names, dot-dirs/node_modules excluded: {w}"
    );
    assert!(
        w.ends_with("should stay out."),
        "wording must be conditional/non-prescriptive: {w}"
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

// --- Boundary-value torture round: wrong-JSON-type arguments must be a named error, never a
// --- silent fallback (see `crate::args`'s module doc). Every case below goes through the REAL
// --- `tools/call` dispatch (`super::call`), not a lower-level unit, so the sweep proves the wiring
// --- end to end.

fn call_tool(name: &str, arguments: serde_json::Value) -> serde_json::Value {
    let params = serde_json::json!({ "name": name, "arguments": arguments });
    super::call(Some(&params))
}

fn error_text(reply: &serde_json::Value) -> String {
    assert_eq!(reply["isError"], true, "expected isError, got: {reply}");
    reply["content"][0]["text"].as_str().unwrap().to_string()
}

#[test]
fn analyze_repo_rejects_a_non_string_path() {
    let reply = call_tool("analyze_repo", serde_json::json!({ "path": 5 }));
    let err = error_text(&reply);
    assert!(
        err.contains("`path` must be a string (got 5)"),
        "got: {err}"
    );
}

#[test]
fn cross_repo_rejects_a_non_array_paths_and_a_non_string_element() {
    let reply = call_tool("cross_repo", serde_json::json!({ "paths": "not-an-array" }));
    let err = error_text(&reply);
    assert!(
        err.contains("`paths` must be an array of strings"),
        "got: {err}"
    );

    let reply = call_tool("cross_repo", serde_json::json!({ "paths": ["ok", 7] }));
    let err = error_text(&reply);
    assert!(
        err.contains("`paths` entries must be strings (got 7)"),
        "got: {err}"
    );
}

#[test]
fn cross_repo_rejects_a_non_string_config_path() {
    let reply = call_tool("cross_repo", serde_json::json!({ "configPath": true }));
    let err = error_text(&reply);
    assert!(
        err.contains("`configPath` must be a string (got true)"),
        "got: {err}"
    );
}

#[test]
fn check_endpoint_rejects_non_string_pattern_path_and_config_path() {
    let reply = call_tool("check_endpoint", serde_json::json!({ "pattern": 1 }));
    assert!(
        error_text(&reply).contains("`pattern` must be a string (got 1)"),
        "got: {reply}"
    );

    let reply = call_tool(
        "check_endpoint",
        serde_json::json!({ "pattern": "x", "path": null, "configPath": 3 }),
    );
    assert!(
        error_text(&reply).contains("`configPath` must be a string (got 3)"),
        "got: {reply}"
    );
}

/// `docs/NORMALIZED_AST.md`'s worked example (also served as the `example-envelope` MCP contract
/// resource, `crate::embedded`) — a minimal, valid, one-file v1 envelope.
const EXAMPLE_ENVELOPE: &str = include_str!("../../../../examples/jsp-envelope.example.json");

#[test]
fn analyze_envelope_tool_runs_mode_a_end_to_end_through_the_real_tool_call() {
    let reply = call_tool(
        "analyze_envelope",
        serde_json::json!({ "envelopeJson": EXAMPLE_ENVELOPE }),
    );
    assert!(reply.get("isError").is_none(), "got: {reply}");
    let v: serde_json::Value =
        serde_json::from_str(reply["content"][0]["text"].as_str().unwrap()).unwrap();
    assert!(v.get("findings").is_some(), "got: {v}");
    assert!(v.get("coverage").is_some(), "got: {v}");
    assert_packs_loaded_entries(&v["packsLoaded"], "analyze_envelope");
    // Envelope mode has no filesystem root/config file — unlike analyze_repo, neither key rides.
    assert!(v.get("path").is_none(), "got: {v}");
    assert!(v.get("config").is_none(), "got: {v}");
}

#[test]
fn analyze_envelope_tool_requires_the_envelope_json_argument() {
    let reply = call_tool("analyze_envelope", serde_json::json!({}));
    let err = error_text(&reply);
    assert!(err.contains("`envelopeJson`"), "got: {err}");
}

#[test]
fn validate_envelope_and_validate_rule_pack_reject_non_string_json_arguments() {
    let reply = call_tool(
        "validate_envelope",
        serde_json::json!({ "envelopeJson": 1 }),
    );
    assert!(
        error_text(&reply).contains("`envelopeJson` must be a string (got 1)"),
        "got: {reply}"
    );
    let reply = call_tool(
        "validate_rule_pack",
        serde_json::json!({ "packJson": false }),
    );
    assert!(
        error_text(&reply).contains("`packJson` must be a string (got false)"),
        "got: {reply}"
    );
}

#[test]
fn analyze_repo_rejects_an_out_of_range_or_wrong_type_limit_and_a_non_string_severity() {
    let dir = TempDir::new("zzop-mcp-arg-sweep-limit");
    dir.write("a.ts", "export const a = 1;\n");
    let path = dir.path().display().to_string();

    for bad_limit in [
        serde_json::json!(-1),
        serde_json::json!(1001),
        serde_json::json!(999_999),
        serde_json::json!("50"),
        serde_json::json!(3.7),
    ] {
        let reply = call_tool(
            "analyze_repo",
            serde_json::json!({ "path": path, "limit": bad_limit }),
        );
        let err = error_text(&reply);
        assert!(
            err.contains("zzop error: limit must be an integer between 0 and 1000"),
            "limit {bad_limit}: got {err}"
        );
    }

    // limit: 0 must be ACCEPTED (a legal "counts only" query), never rejected.
    let reply = call_tool(
        "analyze_repo",
        serde_json::json!({ "path": path, "limit": 0 }),
    );
    assert!(reply.get("isError").is_none(), "got: {reply}");
    let v: serde_json::Value =
        serde_json::from_str(reply["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(v["findings"]["shown"], serde_json::json!([]));

    // A NUMBER severity must hit the same rejection as an unknown STRING severity, not silently
    // drop the filter.
    let reply = call_tool(
        "analyze_repo",
        serde_json::json!({ "path": path, "severity": 5 }),
    );
    let err = error_text(&reply);
    assert!(err.contains("zzop error: unknown severity 5"), "got: {err}");
}

// --- Item 4: relative path echo. `analyze_repo`/`cross_repo` must echo the RESOLVED absolute path,
// --- never the raw (possibly relative) argument.

/// Serializes the one test in this file that mutates the process cwd (`set_current_dir` is
/// process-global and the test harness runs threads in parallel). `zzop-summary`'s own cwd-reading
/// tests (`paths.rs`) run in a separate test binary/process now that the shaping logic moved out of
/// this crate, so this lock only needs to guard against a future cwd-touching test landing here.
static CWD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn analyze_repo_echoes_the_resolved_absolute_path_not_the_raw_dot_argument() {
    let dir = TempDir::new("zzop-mcp-path-echo");
    dir.write("a.ts", "export const a = 1;\n");
    // Run the analysis with the cwd set to the fixture dir and path "." — the exact live-fire
    // scenario: the reply's `path` field must disclose where "." actually resolved to, not echo the
    // literal "." back.
    // cwd is process-global — hold the shared lock so parallel cwd-reading tests can't misresolve.
    let _cwd_guard = CWD_LOCK.lock().unwrap();
    let original_cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    let result = super::analyze(".");
    std::env::set_current_dir(&original_cwd).unwrap();
    drop(_cwd_guard);
    let out = result.expect("analyze should succeed");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let echoed = v["path"].as_str().expect("path must be a string");
    assert_ne!(
        echoed, ".",
        "must not echo the raw relative argument verbatim"
    );
    assert!(
        Path::new(echoed).is_absolute(),
        "echoed path must be absolute, got: {echoed}"
    );
    // Canonicalize both sides for a Windows-safe comparison (short/long path forms, `\\?\` prefixes).
    assert_eq!(
        fs::canonicalize(echoed).unwrap(),
        fs::canonicalize(dir.path()).unwrap()
    );
}

#[test]
fn cross_repo_sources_echo_the_resolved_absolute_path_in_paths_mode() {
    let fe = TempDir::new("zzop-mcp-cross-path-echo-fe");
    fe.write("a.ts", "export const a = 1;\n");
    let be = TempDir::new("zzop-mcp-cross-path-echo-be");
    be.write("b.ts", "export const b = 1;\n");
    let paths = vec![
        fe.path().display().to_string(),
        be.path().display().to_string(),
    ];
    let out = super::cross_repo(&paths, None).expect("cross_repo should succeed");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    for source in v["sources"].as_array().expect("sources array") {
        let echoed = source["path"]
            .as_str()
            .expect("source path must be a string");
        assert!(
            Path::new(echoed).is_absolute(),
            "source path must be absolute, got: {echoed}"
        );
    }
}

// --- Item 3: does the rule-filter zero-match note actually fire through the REAL `analyze_repo`
// --- tool-call path (not `shape_findings` called directly, which `output/tests.rs` already pins)?

#[test]
fn analyze_repo_rule_filter_zero_match_note_fires_end_to_end_through_the_real_tool_call() {
    let dir = TempDir::new("zzop-mcp-rule-note-e2e");
    dir.write("a.ts", "export const a = 1;\n");
    let reply = call_tool(
        "analyze_repo",
        serde_json::json!({ "path": dir.path().display().to_string(), "rule": "nonexistent-xyz" }),
    );
    assert!(reply.get("isError").is_none(), "got: {reply}");
    let v: serde_json::Value =
        serde_json::from_str(reply["content"][0]["text"].as_str().unwrap()).unwrap();
    let note = v["findings"]["note"]
        .as_str()
        .unwrap_or_else(|| panic!("note must be present end-to-end through tools/call, got: {v}"));
    assert!(note.contains("nonexistent-xyz"), "got: {note}");
}

// --- Item 5/6: `architecture` summary + `gitWindow` forwarding — both gated on real git signals
// --- having run, so these need an actual `.git` history fixture (skipped gracefully when `git` is
// --- not on PATH, same convention `crates/facade`'s own git-gated tests use).

fn git_available() -> bool {
    std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_ok()
}

fn run_git(dir: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("git {args:?} failed to spawn: {e}"));
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A real git repo with two commits touching the same file — enough for `git_active` to be true and
/// `health`/`gitWindow` to be `Some` (see `crates/engine/src/analyze/assemble/metrics.rs`'s gating:
/// `health` is computed whenever `git_active`, independent of commit count or `critical`/
/// `recommendations` size).
fn git_history_fixture() -> TempDir {
    let dir = TempDir::new("zzop-mcp-git-fixture");
    run_git(dir.path(), &["init", "-q"]);
    run_git(dir.path(), &["config", "user.email", "test@example.com"]);
    run_git(dir.path(), &["config", "user.name", "Test User"]);
    dir.write("a.ts", "export function a() { return 1; }\n");
    run_git(dir.path(), &["add", "a.ts"]);
    run_git(dir.path(), &["commit", "-q", "-m", "add a"]);
    dir.write("a.ts", "export function a() { return 2; }\n");
    run_git(dir.path(), &["add", "a.ts"]);
    run_git(dir.path(), &["commit", "-q", "-m", "update a"]);
    dir
}

#[test]
fn analyze_repo_carries_a_compact_architecture_summary_when_git_signals_ran() {
    if !git_available() {
        eprintln!(
            "skipping analyze_repo_carries_a_compact_architecture_summary_when_git_signals_ran: git not on PATH"
        );
        return;
    }
    let dir = git_history_fixture();
    let out = super::analyze(&dir.path().display().to_string()).expect("analyze should succeed");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let architecture = v
        .get("architecture")
        .unwrap_or_else(|| panic!("architecture must be present when git signals ran, got: {v}"));
    assert!(architecture["pain"].is_number(), "got: {architecture}");
    assert!(
        architecture.get("topRecommendation").is_some(),
        "topRecommendation key must be present (possibly null), got: {architecture}"
    );
    assert!(
        architecture["criticalTop"].is_array(),
        "got: {architecture}"
    );
    // Capped, never the full detail: this reply must not also carry the full `recommendations`/
    // `critical` arrays (they stay in the @zzop/cli --json / napi lane, per this tool's description).
    assert!(v.get("recommendations").is_none(), "got: {v}");
    assert!(v.get("critical").is_none(), "got: {v}");

    // `gitWindow` rides alongside, forwarded verbatim (D-git-signal seam, agent W4-B's field).
    let git_window = v
        .get("gitWindow")
        .unwrap_or_else(|| panic!("gitWindow must be forwarded when git signals ran, got: {v}"));
    assert!(!git_window.is_null(), "got: {v}");
    assert!(git_window["recentDays"].is_number(), "got: {git_window}");
}

#[test]
fn analyze_repo_omits_architecture_when_git_signals_did_not_run() {
    // No `.git` directory here — zero-config still REQUESTS git collection (`git: {}` is injected by
    // default), but `git_active` resolves false with no repository present, so health/critical/
    // recommendations all stay empty/`None` and `architecture` must be ABSENT (never a null field).
    let dir = TempDir::new("zzop-mcp-no-git-fixture");
    dir.write("a.ts", "export const a = 1;\n");
    let out = super::analyze(&dir.path().display().to_string()).expect("analyze should succeed");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(
        v.get("architecture").is_none(),
        "architecture must be absent (not null) with no git history, got: {v}"
    );
    if let Some(git_window) = v.get("gitWindow") {
        assert!(
            git_window.is_null(),
            "gitWindow must be null (never a populated window) with no git history, got: {v}"
        );
    }
}
