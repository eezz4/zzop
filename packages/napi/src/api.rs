//! The actual `analyze` / `analyzeTrees` / `version` logic, kept napi-free (plain `&str -> Result<String,
//! String>` / `-> String`) so it compiles and has a normal `#[test]` surface under the workspace's default
//! `gnu` toolchain with the `addon` feature off — see `lib.rs`'s module doc for why that split exists.
//! `addon.rs` (feature `addon` only) is a thin `#[napi]` pass-through to these three functions.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use zzop_core::{load_dsl_packs, CommonIr, FileNode, Finding, NormalizedEnvelope, RulePackDef};
use zzop_engine::{AnalyzeOutput, CacheStats, EngineConfig, GitOptions, DEFAULT_SIZE_CAP};
use zzop_metrics::{
    CriticalFile, CrossLayerCoChurn, FolderAggregates, HealthIndex, Recommendation, Scores,
    SeamCandidate,
};

/// `packs_dir`'s accepted shapes: a single directory (unchanged, pre-existing wire form) or an array of
/// directories, all loaded and merged (see `base_engine_config`'s doc for the collision rule). `untagged`
/// tries `String` first, falling back to `Vec<String>` — either form deserializes unambiguously since JSON
/// strings and arrays never overlap.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum PacksDir {
    One(String),
    Many(Vec<String>),
}

impl PacksDir {
    /// Normalizes either wire shape into an ordered list of directories to load, in the order given (a
    /// `One` is a single-element list — `base_engine_config` applies the exact same later-wins merge
    /// either way, so this is the only place the two shapes need to be told apart).
    fn as_dirs(&self) -> Vec<&str> {
        match self {
            PacksDir::One(s) => vec![s.as_str()],
            PacksDir::Many(v) => v.iter().map(String::as_str).collect(),
        }
    }
}

/// One tree's request shape: `{root, source_id?, packs_dir?, cache_dir?, git?: {since?,
/// recent_days?}, size_cap?, disabled_rules?: string[]}`. `#[serde(deny_unknown_fields)]` is deliberately
/// NOT set — an older/newer Node host sending an extra field (e.g. a future `scores_config` knob) should
/// degrade to "ignored", not fail the whole call.
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct AnalyzeRequest {
    pub root: String,
    pub source_id: String,
    pub packs_dir: Option<PacksDir>,
    pub cache_dir: Option<String>,
    pub git: Option<GitOptionsRequest>,
    pub size_cap: Option<usize>,
    pub disabled_rules: Vec<String>,
}

/// `AnalyzeRequest::git`'s payload — mirrors `zzop_engine::GitOptions` field-for-field, as JSON input.
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct GitOptionsRequest {
    pub since: Option<String>,
    pub recent_days: Option<u32>,
}

/// `analyzeTrees`'s request shape: `{trees: AnalyzeRequest[]}` — one `EngineConfig` per tree, joined by
/// `zzop_engine::analyze_trees` (multi-tree/cross-layer).
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct AnalyzeTreesRequest {
    pub trees: Vec<AnalyzeRequest>,
}

/// `analyzeEnvelope`'s request shape (`docs/NORMALIZED_AST.md`'s protocol receiver): unlike
/// `AnalyzeRequest` there is no `root`/`cacheDir`/`git`/`sizeCap` — an envelope carries no filesystem
/// location the engine can re-read (see `zzop_engine::analyze_envelope`'s own module doc for exactly
/// which config knobs envelope mode ignores and why).
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct EnvelopeAnalyzeRequest {
    pub source_id: String,
    pub packs_dir: Option<PacksDir>,
    pub disabled_rules: Vec<String>,
}

/// The shared "load `packs_dir`, build the DSL-pack list + `RuleConfig`" step both `build_engine_config`
/// (tree-rooted requests) and `analyze_envelope_json` (envelope requests) need.
///
/// `packs_dirs` is loaded in order, one `zzop_core::pack_loader::load_dsl_packs` call per directory, and
/// merged into a single pack list: if two directories each ship a pack with the same `RulePackDef::id`,
/// the LATER directory's pack REPLACES the earlier one whole — not a rule-level merge inside that pack id.
/// This is the intentional override path (see `docs/modules/napi.md`'s "Defaults" section) — the JS
/// wrapper (`index.js`) puts the bundled default pack dir first and any caller-supplied `packsDir` after
/// it, so a caller's pack always wins a collision against a shipped one with the same id, while packs with
/// distinct ids from every directory all stay loaded together. Per-directory load errors (a malformed
/// `rules/dsl/*.json`, an unreadable directory) are pushed onto `warnings` rather than failing the whole
/// call — same "surface, don't crash" contract `load_dsl_packs` itself documents; the caller folds
/// `warnings` into the corresponding `AnalyzeOutput`.
fn base_engine_config(
    source_id: &str,
    packs_dirs: &[&str],
    disabled_rules: &[String],
    warnings: &mut Vec<String>,
) -> EngineConfig {
    let mut packs: Vec<RulePackDef> = Vec::new();
    for dir in packs_dirs {
        let result = load_dsl_packs(Path::new(dir));
        for (path, pack) in result.packs {
            let _ = path; // load order already deterministic (sorted by file name) — path not needed here.
            match packs.iter_mut().find(|existing| existing.id == pack.id) {
                Some(slot) => *slot = pack, // later directory wins whole-pack on a same-id collision
                None => packs.push(pack),
            }
        }
        for err in result.errors {
            warnings.push(format!(
                "packs_dir: failed to load {}: {}",
                err.path.display(),
                err.message
            ));
        }
    }

    EngineConfig {
        source_id: source_id.to_string(),
        packs,
        rule_config: zzop_core::RuleConfig {
            disabled_rules: disabled_rules.to_vec(),
            ..Default::default()
        },
        ..EngineConfig::default()
    }
}

/// Builds one `EngineConfig` from one `AnalyzeRequest` — `base_engine_config` plus the tree-rooted knobs
/// (`size_cap`/`cache_dir`/`git`) an envelope request has no equivalent for.
fn build_engine_config(req: &AnalyzeRequest, warnings: &mut Vec<String>) -> EngineConfig {
    let packs_dirs = req
        .packs_dir
        .as_ref()
        .map(PacksDir::as_dirs)
        .unwrap_or_default();
    let mut config = base_engine_config(&req.source_id, &packs_dirs, &req.disabled_rules, warnings);

    config.size_cap = req.size_cap.unwrap_or(DEFAULT_SIZE_CAP);
    config.cache_dir = req.cache_dir.as_ref().map(PathBuf::from);
    config.git = req.git.as_ref().map(|g| GitOptions {
        since: g.since.clone(),
        recent_days: g
            .recent_days
            .unwrap_or_else(|| GitOptions::default().recent_days),
    });
    config
}

/// A JSON-serializable mirror of `zzop_engine::CacheStats` (which does not itself derive `Serialize` — see
/// `AnalyzeOutputView`'s doc for why this crate mirrors rather than forks/modifies engine types).
/// `#[serde(rename_all = "camelCase")]` is a no-op today (`hits`/`misses` are already one word) — applied
/// for consistency with every other output-facing type at this boundary.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CacheStatsView {
    hits: usize,
    misses: usize,
}

impl From<CacheStats> for CacheStatsView {
    fn from(c: CacheStats) -> Self {
        CacheStatsView {
            hits: c.hits,
            misses: c.misses,
        }
    }
}

/// A JSON-serializable *view* over `&zzop_engine::AnalyzeOutput`.
///
/// `AnalyzeOutput` (and its small `CacheStats` payload) do not derive `Serialize`, so this is a
/// **by-reference, zero-copy view**: every field is borrowed straight out of the real `AnalyzeOutput`
/// (the only copies are the two `usize`s in `CacheStatsView`).
///
/// ## Casing contract
/// The entire JSON tree returned by `analyze`/`analyzeTrees`/`analyzeEnvelope` is camelCase: this struct
/// and every output-facing type reachable from it carry `#[serde(rename_all = "camelCase")]`, including
/// the native-rule payload types that reach `Finding.data` via `serde_json::to_value`. Note a
/// struct-level `rename_all` only governs that struct's own fields, not nested types — each nested type
/// needs its own attribute.
///
/// `zzop_core::SourceSymbol` doubles as the deserialize target for `docs/NORMALIZED_AST.md`'s frozen v1
/// external-parser envelope input contract (`FileProjection.symbols`): output is camelCase like
/// everything else, while per-field `#[serde(alias = ...)]` attributes keep accepting the frozen
/// contract's snake_case names on the way in (zzop only ever receives an envelope, never emits one).
///
/// `Finding.data` is the one exception, by design: it is opaque `serde_json::Value` authored ad hoc per
/// rule, never a `#[derive(Serialize)]` struct with a uniform convention to enforce — see
/// `docs/modules/napi.md`'s "Output data shapes" section for the per-rule shapes.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AnalyzeOutputView<'a> {
    ir: &'a CommonIr,
    findings: &'a [Finding],
    degraded: &'a [String],
    file_count: usize,
    nodes: &'a [FileNode],
    scores: &'a Option<Scores>,
    health: &'a Option<HealthIndex>,
    recommendations: &'a [Recommendation],
    critical: &'a [CriticalFile],
    seams: &'a [SeamCandidate],
    folders: &'a Option<FolderAggregates>,
    layer_co_churn: &'a Option<Vec<CrossLayerCoChurn>>,
    warnings: &'a [String],
    cache: Option<CacheStatsView>,
    rule_timings: &'a Option<Vec<zzop_core::dsl::RuleTiming>>,
}

impl<'a> AnalyzeOutputView<'a> {
    fn of(output: &'a AnalyzeOutput) -> Self {
        AnalyzeOutputView {
            ir: &output.ir,
            findings: &output.findings,
            degraded: &output.degraded,
            file_count: output.file_count,
            nodes: &output.nodes,
            scores: &output.scores,
            health: &output.health,
            recommendations: &output.recommendations,
            critical: &output.critical,
            seams: &output.seams,
            folders: &output.folders,
            layer_co_churn: &output.layer_co_churn,
            warnings: &output.warnings,
            cache: output.cache.map(CacheStatsView::from),
            rule_timings: &output.rule_timings,
        }
    }
}

/// `analyze(configJson)`: deserializes `configJson` into an `AnalyzeRequest`, builds
/// an `EngineConfig` (loading DSL packs from `packs_dir` via `zzop_core::load_dsl_packs` when given), runs
/// `zzop_engine::analyze_tree`, and serializes the result. Every failure mode (bad JSON, a pack directory
/// that doesn't exist) returns `Err(message)` — never a panic; `addon.rs` maps `Err` to a napi `Error`.
pub fn analyze_json(config_json: &str) -> Result<String, String> {
    let req: AnalyzeRequest = serde_json::from_str(config_json)
        .map_err(|e| format!("zzop-napi: invalid analyze() config JSON: {e}"))?;
    if req.root.is_empty() {
        return Err("zzop-napi: analyze() config is missing required field \"root\"".to_string());
    }
    let root = PathBuf::from(&req.root);

    let mut warnings = Vec::new();
    let config = build_engine_config(&req, &mut warnings);
    let mut output = zzop_engine::analyze_tree(&root, &config);
    warnings.append(&mut output.warnings);
    output.warnings = warnings;

    serde_json::to_string(&AnalyzeOutputView::of(&output))
        .map_err(|e| format!("zzop-napi: failed to serialize analyze() output: {e}"))
}

/// `analyzeTrees(configJson)`: deserializes `configJson` into an `AnalyzeTreesRequest`
/// (`{trees: AnalyzeRequest[]}`), runs `zzop_engine::analyze_tree` once per entry, then
/// `zzop_engine::analyze_trees`'s cross-layer join over every tree's `IoFacts`. Per-tree DSL-pack-load
/// warnings are attached to that same tree's own `AnalyzeOutput.warnings` (not globally pooled), so a
/// consumer can tell which tree a warning came from.
pub fn analyze_trees_json(config_json: &str) -> Result<String, String> {
    let req: AnalyzeTreesRequest = serde_json::from_str(config_json)
        .map_err(|e| format!("zzop-napi: invalid analyzeTrees() config JSON: {e}"))?;
    if req.trees.is_empty() {
        return Err(
            "zzop-napi: analyzeTrees() config must include at least one entry in \"trees\""
                .to_string(),
        );
    }
    for (i, tree_req) in req.trees.iter().enumerate() {
        if tree_req.root.is_empty() {
            return Err(format!(
                "zzop-napi: analyzeTrees() trees[{i}] is missing required field \"root\""
            ));
        }
    }

    let mut per_tree_warnings: Vec<Vec<String>> = Vec::with_capacity(req.trees.len());
    let mut trees: Vec<(PathBuf, EngineConfig)> = Vec::with_capacity(req.trees.len());
    for tree_req in &req.trees {
        let mut warnings = Vec::new();
        let config = build_engine_config(tree_req, &mut warnings);
        per_tree_warnings.push(warnings);
        trees.push((PathBuf::from(&tree_req.root), config));
    }

    let mut result = zzop_engine::analyze_trees(&trees);
    for (warnings, (_, _, output)) in per_tree_warnings.into_iter().zip(result.trees.iter_mut()) {
        let mut merged = warnings;
        merged.append(&mut output.warnings);
        output.warnings = merged;
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct TreeEntryView<'a> {
        root: String,
        source_id: &'a str,
        output: AnalyzeOutputView<'a>,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct MultiAnalyzeOutputView<'a> {
        trees: Vec<TreeEntryView<'a>>,
        cross_layer: &'a zzop_core::CrossLayerResult,
        /// The 6 `cross-layer/*` native rules run over `cross_layer` (`zzop_engine::analyze_trees`'s own
        /// `MultiAnalyzeOutput::cross_layer_findings` field — a plain `&'a [Finding]` borrow, same
        /// zero-copy-view convention as every other field on this struct, since `Finding` already derives
        /// `Serialize` in `zzop-core`).
        cross_layer_findings: &'a [Finding],
    }

    let view = MultiAnalyzeOutputView {
        trees: result
            .trees
            .iter()
            .map(|(root, source_id, output)| TreeEntryView {
                root: root.display().to_string(),
                source_id,
                output: AnalyzeOutputView::of(output),
            })
            .collect(),
        cross_layer: &result.cross_layer,
        cross_layer_findings: &result.cross_layer_findings,
    };

    serde_json::to_string(&view)
        .map_err(|e| format!("zzop-napi: failed to serialize analyzeTrees() output: {e}"))
}

/// `analyzeEnvelope(envelopeJson, configJson)` (`docs/NORMALIZED_AST.md`'s protocol receiver): validates
/// `envelopeJson` against the v1 Normalized AST contract (`zzop_core::validate_envelope` — a wrong
/// `format`/too-new `version`/empty or duplicate `path`/inverted `body_start`..`body_end` all fail here
/// with a structured, joined message, never a panic), deserializes `configJson` into an
/// `EnvelopeAnalyzeRequest`, and runs `zzop_engine::analyze_envelope`. Same JSON-string-in/JSON-string-out,
/// `AnalyzeOutputView`-serialized shape as `analyze_json`/`analyze_trees_json`.
pub fn analyze_envelope_json(envelope_json: &str, config_json: &str) -> Result<String, String> {
    let envelope: NormalizedEnvelope =
        zzop_core::validate_envelope(envelope_json).map_err(|errors| {
            format!(
                "zzop-napi: invalid analyzeEnvelope() envelope JSON: {}",
                errors.join("; ")
            )
        })?;
    let req: EnvelopeAnalyzeRequest = serde_json::from_str(config_json)
        .map_err(|e| format!("zzop-napi: invalid analyzeEnvelope() config JSON: {e}"))?;

    let mut warnings = Vec::new();
    let packs_dirs = req
        .packs_dir
        .as_ref()
        .map(PacksDir::as_dirs)
        .unwrap_or_default();
    let config = base_engine_config(
        &req.source_id,
        &packs_dirs,
        &req.disabled_rules,
        &mut warnings,
    );
    let mut output = zzop_engine::analyze_envelope(&envelope, &config);
    warnings.append(&mut output.warnings);
    output.warnings = warnings;

    serde_json::to_string(&AnalyzeOutputView::of(&output))
        .map_err(|e| format!("zzop-napi: failed to serialize analyzeEnvelope() output: {e}"))
}

/// `version()`: this crate's own Cargo version plus every parser's
/// `PARSER_FINGERPRINT` (`zzop-cache`'s cache-key ingredient — see `zzop_parser_typescript::PARSER_FINGERPRINT`'s
/// doc), so a host app can log/report exactly which parser build produced a given analysis without needing
/// its own copy of those constants.
pub fn version_string() -> String {
    format!(
        "zzop-napi/{} zzop-parser-typescript={} zzop-parser-prisma={}",
        env!("CARGO_PKG_VERSION"),
        zzop_parser_typescript::PARSER_FINGERPRINT,
        zzop_parser_prisma::PARSER_FINGERPRINT,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
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
            let dir =
                std::env::temp_dir().join(format!("{prefix}-{}-{nanos}-{n}", std::process::id()));
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

    fn cycle_fixture() -> TempDir {
        let dir = TempDir::new("zzop-napi-fixture");
        dir.write(
            "a.ts",
            "import { b } from './b';\nexport function a() { return b(); }\n",
        );
        dir.write(
            "b.ts",
            "import { a } from './a';\nexport function b() { return a(); }\n",
        );
        dir
    }

    fn git_available() -> bool {
        Command::new("git").arg("--version").output().is_ok()
    }

    fn run_git(dir: &Path, args: &[&str]) {
        let out = Command::new("git")
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

    /// A real git repo (same `git init`/`config`/`commit` pattern as
    /// `packages/engine/tests/analyze_git.rs`'s `git_fixture_repo`) built to exercise every `HashMap`-typed
    /// field reachable from `AnalyzeOutputView` in one fixture:
    /// - `ir.dep` gets 2+ keys: `a.ts` imports both `b.ts` and `c.ts`, `b.ts` imports `c.ts`.
    /// - `ir.loc` gets 3 keys (one per file).
    /// - `a.ts`'s `tag_counts` gets 3 distinct tags (FEAT/FIX/DOCS) from 3 separately-tagged commits, so
    ///   sorting actually has something to do (a single-key map would trivially "sort").
    fn cycle_and_git_fixture() -> TempDir {
        let dir = TempDir::new("zzop-napi-determinism-fixture");
        run_git(dir.path(), &["init", "-q"]);
        run_git(dir.path(), &["config", "user.email", "test@example.com"]);
        run_git(dir.path(), &["config", "user.name", "Test User"]);

        dir.write("c.ts", "export function c() { return 1; }\n");
        run_git(dir.path(), &["add", "c.ts"]);
        run_git(dir.path(), &["commit", "-q", "-m", "[FEAT] add c"]);

        dir.write(
            "b.ts",
            "import { c } from './c';\nexport function b() { return c(); }\n",
        );
        dir.write(
            "a.ts",
            "import { b } from './b';\nimport { c } from './c';\nexport function a() { return b() + c(); }\n",
        );
        run_git(dir.path(), &["add", "a.ts", "b.ts"]);
        run_git(dir.path(), &["commit", "-q", "-m", "[FEAT] add a and b"]);

        dir.write(
            "a.ts",
            "import { b } from './b';\nimport { c } from './c';\nexport function a() { return b() + c() + 1; }\n",
        );
        run_git(dir.path(), &["add", "a.ts"]);
        run_git(dir.path(), &["commit", "-q", "-m", "[FIX] correct a"]);

        dir.write(
            "a.ts",
            "// updated docs\nimport { b } from './b';\nimport { c } from './c';\nexport function a() { return b() + c() + 1; }\n",
        );
        run_git(dir.path(), &["add", "a.ts"]);
        run_git(dir.path(), &["commit", "-q", "-m", "[DOCS] document a"]);

        dir
    }

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
    fn analyze_json_reports_a_bad_packs_dir_as_a_warning_not_a_failure() {
        let dir = cycle_fixture();
        let config = format!(
            r#"{{"root": {:?}, "packsDir": {:?}}}"#,
            dir.path().display(),
            dir.path().join("no-such-dir").display()
        );
        let out = analyze_json(&config).expect("analyze_json should still succeed");
        let value: serde_json::Value = serde_json::from_str(&out).unwrap();
        let warnings = value["warnings"].as_array().expect("warnings array");
        assert!(warnings
            .iter()
            .any(|w| w.as_str().unwrap().contains("packs_dir")));
    }

    /// A one-rule DSL pack JSON matching `packages/core/src/pack_loader.rs`'s `valid_pack` shape — field
    /// names are the DSL's own snake_case (packs are DSL-authored files, not part of this boundary's
    /// camelCase JS-facing config contract). A `line-scan` rule flagging `line_pattern` inside any `.ts`
    /// file.
    fn dsl_pack_json(pack_id: &str, rule_id: &str, line_pattern: &str) -> String {
        format!(
            r#"{{
                "id": "{pack_id}",
                "framework": "any",
                "rules": [
                    {{
                        "id": "{rule_id}",
                        "severity": "warning",
                        "message": "msg",
                        "matcher": {{
                            "type": "line-scan",
                            "file_pattern": "\\.ts$",
                            "line_pattern": "{line_pattern}"
                        }}
                    }}
                ]
            }}"#
        )
    }

    #[test]
    fn analyze_json_packs_dir_array_loads_and_merges_every_directory() {
        let dir = cycle_fixture();
        dir.write("marker.ts", "// MARKER_A\n// MARKER_B\n");

        let packs_a = TempDir::new("zzop-napi-packs-a");
        packs_a.write("pack-a.json", &dsl_pack_json("pack-a", "r1", "MARKER_A"));
        let packs_b = TempDir::new("zzop-napi-packs-b");
        packs_b.write("pack-b.json", &dsl_pack_json("pack-b", "r1", "MARKER_B"));

        let config = format!(
            r#"{{"root": {:?}, "packsDir": [{:?}, {:?}]}}"#,
            dir.path().display(),
            packs_a.path().display(),
            packs_b.path().display()
        );
        let out = analyze_json(&config).expect("analyze_json should succeed");
        let value: serde_json::Value = serde_json::from_str(&out).unwrap();
        let findings = value["findings"].as_array().expect("findings array");
        assert!(
            findings.iter().any(|f| f["ruleId"] == "pack-a/r1"),
            "expected pack-a's rule to fire, got: {value}"
        );
        assert!(
            findings.iter().any(|f| f["ruleId"] == "pack-b/r1"),
            "expected pack-b's rule to fire, got: {value}"
        );
    }

    #[test]
    fn analyze_json_packs_dir_array_same_pack_id_later_directory_wins_whole_pack() {
        let dir = cycle_fixture();
        dir.write("marker.ts", "// OLD_MARKER\n// NEW_MARKER\n");

        let packs_old = TempDir::new("zzop-napi-packs-old");
        packs_old.write(
            "custom.json",
            &dsl_pack_json("custom", "marker-old", "OLD_MARKER"),
        );
        let packs_new = TempDir::new("zzop-napi-packs-new");
        packs_new.write(
            "custom.json",
            &dsl_pack_json("custom", "marker-new", "NEW_MARKER"),
        );

        let config = format!(
            r#"{{"root": {:?}, "packsDir": [{:?}, {:?}]}}"#,
            dir.path().display(),
            packs_old.path().display(),
            packs_new.path().display()
        );
        let out = analyze_json(&config).expect("analyze_json should succeed");
        let value: serde_json::Value = serde_json::from_str(&out).unwrap();
        let findings = value["findings"].as_array().expect("findings array");
        assert!(
            findings.iter().any(|f| f["ruleId"] == "custom/marker-new"),
            "expected the later directory's rule to fire, got: {value}"
        );
        assert!(
            !findings.iter().any(|f| f["ruleId"] == "custom/marker-old"),
            "expected the earlier directory's same-id pack to be fully replaced (not merged), got: {value}"
        );
    }

    #[test]
    fn analyze_json_packs_dir_array_bad_entry_warns_but_other_entries_still_load() {
        let dir = cycle_fixture();
        dir.write("marker.ts", "// MARKER_A\n");

        let packs_good = TempDir::new("zzop-napi-packs-good");
        packs_good.write("pack-a.json", &dsl_pack_json("pack-a", "r1", "MARKER_A"));
        let bad_dir = dir.path().join("no-such-packs-dir");

        let config = format!(
            r#"{{"root": {:?}, "packsDir": [{:?}, {:?}]}}"#,
            dir.path().display(),
            bad_dir.display(),
            packs_good.path().display()
        );
        let out = analyze_json(&config).expect("analyze_json should still succeed");
        let value: serde_json::Value = serde_json::from_str(&out).unwrap();
        let warnings = value["warnings"].as_array().expect("warnings array");
        assert!(
            warnings
                .iter()
                .any(|w| w.as_str().unwrap().contains("packs_dir")),
            "expected a packs_dir warning for the bad directory, got: {value}"
        );
        let findings = value["findings"].as_array().expect("findings array");
        assert!(
            findings.iter().any(|f| f["ruleId"] == "pack-a/r1"),
            "expected pack-a's rule to still fire despite the bad directory, got: {value}"
        );
    }

    #[test]
    fn analyze_trees_json_joins_two_trees_and_rejects_empty_input() {
        let fe = TempDir::new("zzop-napi-fe");
        fe.write(
            "client.ts",
            "import axios from 'axios';\nexport const load = () => axios.get('/api/users');\n",
        );
        let be = TempDir::new("zzop-napi-be");
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

    fn tiny_envelope_json() -> String {
        r#"{
            "format": "zzop-normalized-ast",
            "version": 1,
            "parser": "jsp-lexical/1",
            "source": "legacy",
            "files": [
                {
                    "path": "legacy/UserController.jsp",
                    "loc": 40,
                    "io": {
                        "provides": [
                            {"kind": "http", "key": "GET /legacy/user.jsp", "file": "legacy/UserController.jsp", "line": 5}
                        ],
                        "consumes": []
                    }
                }
            ]
        }"#
        .to_string()
    }

    #[test]
    fn analyze_envelope_json_round_trips_a_tiny_envelope() {
        let config = r#"{"sourceId": "legacy"}"#;
        let out = analyze_envelope_json(&tiny_envelope_json(), config)
            .expect("analyze_envelope_json should succeed");
        let value: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
        assert_eq!(value["fileCount"], 1);
        let provides = value["ir"]["io"]["provides"]
            .as_array()
            .expect("provides array");
        assert_eq!(provides.len(), 1);
        assert_eq!(provides[0]["key"], "GET /legacy/user.jsp");
    }

    #[test]
    fn analyze_envelope_json_rejects_an_invalid_envelope_without_panicking() {
        let bad_envelope = tiny_envelope_json().replace("zzop-normalized-ast", "bogus-format");
        let err = analyze_envelope_json(&bad_envelope, r#"{"sourceId": "legacy"}"#).unwrap_err();
        assert!(err.contains("invalid analyzeEnvelope() envelope JSON"));
        assert!(err.contains("unknown format"));
    }

    #[test]
    fn analyze_envelope_json_rejects_invalid_config_json() {
        let err = analyze_envelope_json(&tiny_envelope_json(), "not json").unwrap_err();
        assert!(err.contains("invalid analyzeEnvelope() config JSON"));
    }

    #[test]
    fn version_string_includes_parser_fingerprints() {
        let v = version_string();
        assert!(v.contains("zzop-parser-typescript="));
        assert!(v.contains("zzop-parser-prisma="));
    }
}
