//! End-to-end HOST-DISPATCH tests over real temp trees (no config file -> zero-config defaults, which
//! inject the build.rs-embedded bundled packs as inline `packDefs` — so `packsLoaded` here reports
//! `source: "inline"` for every bundled pack). These call this crate's entry points EXACTLY as the two
//! host products do — the `zzop` CLI's subcommands and the `zzop-mcp` server's `tools/call` handlers
//! both land on `analyze_summary`/`cross_summary`/`endpoint_summary`/the two validators — so they pin
//! the host-facing answer, from outside the crate, over the real engine. They deliberately do NOT pin
//! the shaping logic's internals; that logic's own unit tests live beside it in `src/` (e.g.
//! `config_warnings`, `output`). The MCP-surface half of this coverage (`tools/list` schema pins and
//! `tools/call` dispatch, through the real wire shape) lives in the `zzop-mcp` package instead
//! (`packages/mcp/src/tools/tests.rs`), since `tools/list`/`tools/call` are MCP-only.
//!
//! Why an integration test rather than a `src/` module: it drives the crate through its PUBLIC surface
//! the way a product does. It moved here in the 2026-07-26 `crates/host` teardown — it used to sit in
//! that crate's deleted pass-through `tools.rs`, testing the wrappers instead of the answers.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// The unfiltered default findings view, built through the WIRE-NEUTRAL constructor — the same call the
/// `zzop` CLI makes for `analyze`/`cross` (no MCP `tools/call` JSON fabricated anywhere).
fn default_filters() -> zzop_summary::FindingFilters {
    zzop_summary::FindingFilters::new(None, None, None).expect("no-filter view always constructs")
}

/// `zzop analyze <path>` / the `analyze_repo` tool's `path` mode, at the seam both products call.
fn analyze(path: &str) -> Result<String, String> {
    zzop_summary::analyze_summary(Some(path), None, &default_filters())
}

/// `zzop analyze --config <file>` / the `analyze_repo` tool's `configPath` mode — the same seam, the
/// other source mode.
fn analyze_with_config(config_path: &str) -> Result<String, String> {
    zzop_summary::analyze_summary(None, Some(config_path), &default_filters())
}

/// `zzop cross <path>...` / `cross --config` / the `cross_repo` tool.
fn cross_repo(paths: &[String], config_path: Option<&str>) -> Result<String, String> {
    zzop_summary::cross_summary(paths, config_path, &default_filters())
}

/// `zzop endpoint <pattern> ...` / the `check_endpoint` tool.
fn check_endpoint(
    pattern: &str,
    path: Option<&str>,
    paths: &[String],
    config_path: Option<&str>,
) -> Result<String, String> {
    zzop_summary::endpoint_summary(pattern, path, paths, config_path)
}

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
        let dir = TempDir(dir);
        // Every fixture in this file is a tree meant to be ANALYZED, and since 2026-07-27 no analysis
        // lane runs without a config. Seeding the starter document here (the same bytes `zzop init`
        // writes) keeps each test about its own subject; a test that needs a different config just
        // writes over this one, since `write` runs after `new`.
        dir.write(
            zzop_config::DEFAULT_CONFIG_FILENAME,
            zzop_config::template::CONFIG_TEMPLATE_JSONC,
        );
        dir
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

#[test]
fn analyze_repo_summary_includes_packs_loaded_and_the_coverage_census() {
    let dir = TempDir::new("zzop-summary-packs-loaded");
    dir.write("a.ts", "export const a = 1;\n");
    let out = analyze(&dir.path().display().to_string()).expect("analyze should succeed");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_packs_loaded_entries(&v["packsLoaded"], "analyze_repo");
    // The engine's per-tree coverage census must ride the summary — this fixture has files but
    // zero io, so the census carries the joinContributionZero BLIND assertion, and a summary that
    // dropped it would have the reader believe "0 findings, fine" about an io-invisible tree.
    let cov = v["coverage"]
        .as_object()
        .unwrap_or_else(|| panic!("analyze summary must carry coverage, got: {v}"));
    // 2, not 1: `a.ts` plus the `zzop.config.jsonc` every analyzable tree now carries. The config is a
    // committed file in the user's own repo, so a run counting it is not new behaviour — it is what any
    // configured tree already did; it is only universal now that a config is required.
    assert_eq!(cov["files"], 2, "got: {v}");
    assert_eq!(cov["ioProvides"], 0, "got: {v}");
    assert_eq!(cov["joinContributionZero"], true, "got: {v}");
}

#[test]
fn analyze_repo_merges_facade_level_config_warnings_into_the_reply_channel() {
    // End-to-end over the real engine: a config disabling a rule id that matches nothing produces a
    // facade-level `configWarnings` diagnostic (moved out of the engine's `warnings` channel), and
    // the reply must merge it after the loader's own warnings — not silently drop it at this layer.
    let dir = TempDir::new("zzop-summary-facade-config-warnings");
    dir.write("a.ts", "export const a = 1;\n");
    dir.write(
        "zzop.config.jsonc",
        "{ \"rules\": { \"no-such-rule-xyz\": \"off\" } }",
    );
    let out = analyze(&dir.path().display().to_string()).expect("analyze should succeed");
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
    let dir = TempDir::new("zzop-summary-degraded-cap");
    let over = zzop_summary::output::DEFAULT_DEGRADED_LIMIT + 5;
    for i in 0..over {
        dir.write(&format!("f{i}.ts"), "export const a = 1;\n");
    }
    dir.write("zzop.config.jsonc", "{ \"sizeCap\": 1 }");
    let out = analyze(&dir.path().display().to_string()).expect("analyze should succeed");
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
    let dir = TempDir::new("zzop-summary-rule-overrides-absent");
    dir.write("a.ts", "export const a = 1;\n");
    let out = analyze(&dir.path().display().to_string()).expect("analyze should succeed");
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
    let fe = TempDir::new("zzop-summary-endpoint-single");
    write_fetch_tree(&fe, &["/api/users".to_string()]);
    // Single `path` mode still routes through analyzeTrees (the verdict vocabulary is join facts),
    // so a consume with no provider lands as consumed-unprovided — a definitive answer, not a count.
    let out = check_endpoint("users", Some(&fe.path().display().to_string()), &[], None)
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
    // Forwarded from the analysis, FOLDED (2026-07-29): the counts and the pointer, never the
    // registry's run-invariant prose — this verdict reply is the shortest of the three that carry the
    // channel, so the un-folded array was almost all of it. `crates/summary/tests/disclosure_fold.rs`
    // owns the shape; this only pins that `check_endpoint` still carries the channel at all.
    assert!(
        v["disclosure"]["classes"].as_u64().is_some_and(|n| n > 0),
        "disclosure forwarded from the analysis: {}",
        v["disclosure"]
    );
}

#[test]
fn check_endpoint_not_found_suggests_and_requires_exactly_one_source() {
    let fe = TempDir::new("zzop-summary-endpoint-notfound");
    write_fetch_tree(&fe, &["/api/users".to_string()]);
    let path = fe.path().display().to_string();
    let out = check_endpoint("/internal/users", Some(&path), &[], None).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["verdict"], "not-found");
    assert_eq!(v["suggestions"][0], "GET /api/users");

    // Spelling-free by contract (see `zzop_config::trees`'s WIRE NEUTRALITY note): the guided error
    // names the SHAPES a caller can pass, never one host's argument spelling (`path`/`configPath`).
    let err = check_endpoint("users", None, &[], None).unwrap_err();
    assert!(
        err.contains("pass one tree root, 2+ tree roots, or a config file"),
        "guided no-source error: {err}"
    );
    let err = check_endpoint("users", Some(&path), std::slice::from_ref(&path), None).unwrap_err();
    assert!(
        err.contains("exactly ONE"),
        "guided multi-source error: {err}"
    );
}

#[test]
fn check_endpoint_one_path_in_paths_mode_names_itself_not_cross_repo() {
    // Live-fire misfire: `paths` mode's "at least 2 paths" error is built by `zero_config_trees`,
    // a helper shared with the cross-layer join — before the operation-name parameter, this error
    // always said "cross_repo" even when the endpoint query was the caller that got fewer than 2 paths.
    // The name is now SURFACE-NEUTRAL ("the endpoint query", not `check_endpoint`): the shared library
    // must not put one host's tool spelling in front of the other host's user — see
    // `zzop_config::trees`'s WIRE NEUTRALITY note and the machine pin in
    // `crates/engine/tests/rule_contracts/host_vocabulary.rs`.
    let fe = TempDir::new("zzop-summary-endpoint-one-path");
    write_fetch_tree(&fe, &["/api/users".to_string()]);
    let err = check_endpoint(
        "users",
        None,
        std::slice::from_ref(&fe.path().display().to_string()),
        None,
    )
    .unwrap_err();
    assert!(
        err.contains("the endpoint query needs at least 2 paths"),
        "must name the actual caller, not a sibling operation: {err}"
    );
    assert!(
        !err.contains("cross"),
        "must not name the sibling operation at all: {err}"
    );
}

#[test]
fn check_endpoint_config_path_single_tree_names_its_source_like_path_mode() {
    // configPath single-tree mode must not produce `source: ""` matches while the same tree
    // reached via `path` gets dir-named — the two entry modes answer identically.
    let fe = TempDir::new("zzop-summary-endpoint-cfg-source");
    write_fetch_tree(&fe, &["/api/users".to_string()]);
    fe.write("zzop.config.jsonc", "{}");
    let cp = fe.path().join("zzop.config.jsonc").display().to_string();
    let out = check_endpoint("users", None, &[], Some(&cp)).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let expected = fe.path().file_name().unwrap().to_str().unwrap();
    assert_eq!(
        v["matches"]["unprovidedConsumes"][0]["source"], expected,
        "configPath single-tree matches must carry the dir-named source"
    );
}

#[test]
fn check_endpoint_carries_the_config_honesty_channels_like_every_sibling_tool() {
    // paths mode over trees that each HOLD a zzop.config.jsonc. Both are loaded (2026-07-27 — before
    // that this mode ignored them and disclosed the ignoring), and `config` still reads null because no
    // ONE config governs the run. That combination is exactly what a reader could misread, so the
    // disclosure inverted rather than disappeared: it now names the configs that WERE loaded.
    let fe = TempDir::new("zzop-summary-endpoint-honesty-fe");
    write_fetch_tree(&fe, &["/api/users".to_string()]);
    let be = TempDir::new("zzop-summary-endpoint-honesty-be");
    write_fetch_tree(&be, &["/api/other".to_string()]);
    let paths = vec![
        fe.path().display().to_string(),
        be.path().display().to_string(),
    ];
    let out = check_endpoint("users", None, &paths, None).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(
        v["config"].is_null(),
        "paths mode has N configs, so it names none as THE one"
    );
    let warnings = v["configWarnings"].as_array().expect("configWarnings");
    let loaded = warnings
        .iter()
        .filter_map(|w| w.as_str())
        .find(|w| w.contains("paths mode loaded each tree's own"))
        .unwrap_or_else(|| panic!("loaded-config disclosure must ride the reply: {warnings:?}"));
    for dir in [fe.path(), be.path()] {
        assert!(
            loaded.contains(&dir.display().to_string()),
            "the disclosure must name every config it loaded, missing {}: {loaded}",
            dir.display()
        );
    }
}

/// `bucketKeys` is UNCAPPED (2026-07-29 — the cap, its `bucketKeysTruncated` disclosure, and
/// `snapshot.mjs`'s abort-on-truncation path were all deleted together). 25 keys is deliberately the size
/// that used to truncate: it was 5 over the old 20-key cap, so this test fails the moment a cap returns.
#[test]
fn cross_repo_summary_lists_every_bucket_key_uncapped() {
    let fe = TempDir::new("zzop-summary-bucket-keys-fe");
    let over = 25;
    let paths: Vec<String> = (0..over).map(|i| format!("/api/things/{i}")).collect();
    write_fetch_tree(&fe, &paths);
    let be = TempDir::new("zzop-summary-bucket-keys-be");
    be.write("b.ts", "export const b = 2;\n");
    let roots = vec![
        fe.path().display().to_string(),
        be.path().display().to_string(),
    ];
    let out = cross_repo(&roots, None).expect("cross_repo should succeed");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let keys = v["bucketKeys"]["unprovidedConsumes"]
        .as_array()
        .expect("bucketKeys array");
    assert_eq!(keys.len(), over, "every distinct key, no cap");
    assert_eq!(keys[0], "GET /api/things/0", "engine order preserved");
    assert!(
        v.get("bucketKeysTruncated").is_none(),
        "the truncation field is gone entirely, not merely empty: {}",
        v["bucketKeysTruncated"]
    );
    // An empty bucket still appears, with its (empty) key list.
    assert_eq!(v["bucketKeys"]["unconsumedProvides"], serde_json::json!([]));
    // `bucketKeySites` mirrors `bucketKeys` shape with a locatable
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
fn both_products_reach_the_same_offline_validators_through_this_crate() {
    // The `validate-rule-pack` / `validate-envelope` CLI subcommands and the MCP
    // `validate_rule_pack`/`validate_envelope` tools call these exact re-exports (structure-only checks
    // owned by `zzop-facade`, surfaced here so a host needs only this crate) — so a terminal check and
    // a tool call agree by construction.
    let bundled = zzop_config::BUNDLED_PACK_SOURCES[0].1;
    let ok: serde_json::Value =
        serde_json::from_str(&zzop_summary::validate_rule_pack_json(bundled)).unwrap();
    assert_eq!(ok["valid"], true, "got: {ok}");
    let bad: serde_json::Value =
        serde_json::from_str(&zzop_summary::validate_rule_pack_json("{\"id\":\"p\"}")).unwrap();
    assert_eq!(bad["valid"], false, "got: {bad}");

    // The embedded minimal example envelope is valid; arbitrary non-envelope JSON (an array) is not.
    let example = zzop_summary::contracts::find("example-envelope")
        .unwrap()
        .content;
    let ok: serde_json::Value =
        serde_json::from_str(&zzop_summary::validate_envelope_only_json(example)).unwrap();
    assert_eq!(ok["valid"], true, "got: {ok}");
    let bad: serde_json::Value =
        serde_json::from_str(&zzop_summary::validate_envelope_only_json("[]")).unwrap();
    assert_eq!(bad["valid"], false, "got: {bad}");
}

#[test]
fn cross_repo_summary_includes_per_source_packs_loaded_and_coverage() {
    // fe extracts io (fetch consumes); be analyzes a file but contributes ZERO io to the join —
    // the engine asserts that (`joinContributionZero`), and the per-source summary entry must
    // surface it: a blind reader of "N findings, fine" for an io-invisible tree is exactly the
    // silent failure this project exists to disclose.
    let fe = TempDir::new("zzop-summary-packs-loaded-fe");
    write_fetch_tree(&fe, &["/api/users".to_string()]);
    let be = TempDir::new("zzop-summary-packs-loaded-be");
    be.write("b.ts", "export const b = 2;\n");
    let paths = vec![
        fe.path().display().to_string(),
        be.path().display().to_string(),
    ];
    let out = cross_repo(&paths, None).expect("cross_repo should succeed");
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
    let parent = TempDir::new("zzop-summary-sibling-scope");
    parent.write(
        "fe/src/api.ts",
        "export const a = () => fetch('/api/users');\n",
    );
    parent.write("be/src/b.ts", "export const b = 2;\n");
    // Each analyzed tree needs its own config in paths mode (2026-07-27); the SIBLINGS deliberately get
    // none — they are the directories this test is about not analyzing.
    for tree in ["fe", "be"] {
        parent.write(
            &format!("{tree}/{}", zzop_config::DEFAULT_CONFIG_FILENAME),
            zzop_config::template::CONFIG_TEMPLATE_JSONC,
        );
    }
    for name in ["e2e", "docs-site"] {
        fs::create_dir_all(parent.path().join(name)).unwrap();
    }
    fs::create_dir_all(parent.path().join(".hidden")).unwrap();
    fs::create_dir_all(parent.path().join("node_modules")).unwrap();
    let paths = vec![
        parent.path().join("fe").display().to_string(),
        parent.path().join("be").display().to_string(),
    ];
    let out = cross_repo(&paths, None).expect("cross_repo should succeed");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let warnings = v["configWarnings"].as_array().expect("configWarnings");
    // "sibling director", not "sibling": paths mode's loaded-configs disclosure quotes the absolute
    // config paths, and this test's own temp root is named ...-sibling-scope-... — the looser needle
    // matched THAT warning first and reported a fake failure of a warning that was present.
    let w = warnings
        .iter()
        .filter_map(|w| w.as_str())
        .find(|w| w.contains("sibling director"))
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
    let parent = TempDir::new("zzop-summary-sibling-config");
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
    let out = cross_repo(&[], Some(&cp)).expect("cross_repo should succeed");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let warnings = v["configWarnings"].as_array().expect("configWarnings");
    let w = warnings
        .iter()
        .filter_map(|w| w.as_str())
        .find(|w| w.contains("sibling director"))
        .unwrap_or_else(|| panic!("sibling disclosure must ride configWarnings: {warnings:?}"));
    assert!(
        w.contains("1 sibling directory under") && w.contains("is not part of this join: e2e"),
        "got: {w}"
    );
}

/// Seals `analyze`'s SECOND source mode (`zzop analyze --config` / the `analyze_repo` tool's
/// `configPath`): a single-tree config sitting somewhere OTHER than the tree root is analyzable, and the
/// reply echoes the tree root it actually analyzed — never the config file's own path, which was never
/// a tree.
#[test]
fn analyze_config_mode_runs_the_configs_one_tree_and_echoes_that_tree_root() {
    let parent = TempDir::new("zzop-summary-analyze-config");
    parent.write("app/src/a.ts", "export const a = 1;\n");
    fs::create_dir_all(parent.path().join("ci")).unwrap();
    parent.write("ci/zzop.config.jsonc", "{\n  \"roots\": [\"../app\"]\n}\n");
    let cp = parent
        .path()
        .join("ci")
        .join("zzop.config.jsonc")
        .display()
        .to_string();
    let out = analyze_with_config(&cp).expect("analyze --config should succeed");
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(
        v["config"].as_str().map(str::to_string),
        Some(cp.clone()),
        "the honored config file must be echoed"
    );
    let echoed = v["path"].as_str().expect("path echo");
    assert!(
        echoed.ends_with("app"),
        "the echoed path must be the analyzed TREE root, not the config file: {echoed}"
    );
    assert!(v["fileCount"].as_u64().unwrap_or(0) >= 1, "{out}");
}

/// Seals the source-mode exclusivity of the same seam: both sources, or neither, is a named error from
/// the SHARED handler — so the CLI and the MCP tool cannot drift on which combinations are legal.
#[test]
fn analyze_rejects_both_sources_and_no_source_by_name() {
    let both =
        zzop_summary::analyze_summary(Some("."), Some("x.jsonc"), &default_filters()).unwrap_err();
    assert!(both.contains("not both"), "{both}");
    let neither = zzop_summary::analyze_summary(None, None, &default_filters()).unwrap_err();
    assert!(neither.contains("pass a tree root"), "{neither}");
}

// --- Item 4: relative path echo. `analyze`/`cross_repo` must echo the RESOLVED absolute path,
// --- never the raw (possibly relative) argument.

/// Serializes the one test in this file that mutates the process cwd (`set_current_dir` is
/// process-global and the test harness runs threads in parallel). `zzop-config`'s own cwd-reading
/// absolutization tests (`crates/config/src/paths.rs`) run in a separate test binary/process, so this
/// lock only needs to guard against a future cwd-touching test landing in THIS binary.
static CWD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn analyze_repo_echoes_the_resolved_absolute_path_not_the_raw_dot_argument() {
    let dir = TempDir::new("zzop-summary-path-echo");
    dir.write("a.ts", "export const a = 1;\n");
    // Run the analysis with the cwd set to the fixture dir and path "." — the exact live-fire
    // scenario: the reply's `path` field must disclose where "." actually resolved to, not echo the
    // literal "." back.
    // cwd is process-global — hold the shared lock so parallel cwd-reading tests can't misresolve.
    let _cwd_guard = CWD_LOCK.lock().unwrap();
    let original_cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    let result = analyze(".");
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
    let fe = TempDir::new("zzop-summary-cross-path-echo-fe");
    fe.write("a.ts", "export const a = 1;\n");
    let be = TempDir::new("zzop-summary-cross-path-echo-be");
    be.write("b.ts", "export const b = 1;\n");
    let paths = vec![
        fe.path().display().to_string(),
        be.path().display().to_string(),
    ];
    let out = cross_repo(&paths, None).expect("cross_repo should succeed");
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

// --- Item 3's rule-filter zero-match-note pin needs a `rule` filter argument, which only the MCP
// --- tool surface accepts (the CLI's `analyze` subcommand runs the unfiltered default view) — that
// --- coverage lives in the `zzop-mcp` package instead (`analyze_repo_rule_filter_zero_match_note_
// --- fires_end_to_end_through_the_real_tool_call`).

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
    let dir = TempDir::new("zzop-summary-git-fixture");
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
    let out = analyze(&dir.path().display().to_string()).expect("analyze should succeed");
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
    // `critical` arrays (they stay in the raw `zzop-facade` embedding lane, per this tool's description).
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
    let dir = TempDir::new("zzop-summary-no-git-fixture");
    dir.write("a.ts", "export const a = 1;\n");
    let out = analyze(&dir.path().display().to_string()).expect("analyze should succeed");
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
