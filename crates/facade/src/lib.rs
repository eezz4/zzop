//! `zzop-facade` — the engine's pure JSON facade: the actual `analyze` / `analyzeTrees` / `version`
//! logic, kept napi-free (plain `&str -> Result<String, String>` / `-> String`) so it compiles and has
//! a normal `#[test]` surface under the workspace's default `gnu` toolchain with no feature flags at
//! all.
//!
//! Two consumers share this crate:
//! - `zzop-napi` re-exports every function from here and wraps each one with a thin `#[napi]` shim
//!   under its default-off `addon` feature (`packages/native/src/addon.rs`) — the Node addon build.
//! - `zzop-mcp`, a Node-free binary, calls these functions directly — no napi, no Node process.
//!
//! It lives in its own `rlib`-only crate, separate from `zzop-napi`, because cargo builds a
//! dependency's `cdylib` target even on an `rlib` dependency edge: `zzop-napi`'s `cdylib` half (the
//! Node addon artifact) fails to link under the local `gnu` toolchain with "export ordinal too large"
//! once its `#[napi]` surface is compiled in, and that failure would poison any crate that merely
//! depended on `zzop-napi` for its plain-Rust logic — even one, like `zzop-mcp`, that never touches
//! napi at all. Splitting the napi-free logic into a separate `rlib` crate sidesteps the cdylib link
//! step entirely for every consumer except the Node addon build itself.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use zzop_core::{
    load_dsl_packs, CommonIr, FileNode, Finding, GlobalExclude, NormalizedEnvelope, RulePackDef,
    Severity, Suppression,
};
use zzop_engine::{
    AnalyzeOutput, CacheStats, EngineConfig, GitOptions, MountRule, DEFAULT_SIZE_CAP,
};
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
    /// Inline rule-pack definitions injected as data — the self-contained-binary alternative to
    /// `packs_dir`: a host with no filesystem-resident pack directory (e.g. `zzop-mcp`'s bundled packs,
    /// embedded at compile time by `zzop-config`'s `build.rs`) hands its packs straight to the engine as
    /// `RulePackDef` values instead of pointing at a directory of pack JSON files on disk. Wire name
    /// `packDefs`; `#[serde(default)]` (inherited from this struct's `default` attribute) makes this
    /// genuinely additive — an older or newer JS host that never sends `packDefs` at all gets the
    /// pre-existing `packs_dir`-only behavior, byte-for-byte. Loaded BEFORE `packs_dir` directories in
    /// `base_engine_config`'s seed order, so a directory pack with the same id WINS the collision whole
    /// (a caller's own `packsDir` directory overrides an embedded bundled pack with the same id, mirroring
    /// the JS wrapper's bundled-first `index.js` ordering — see `base_engine_config`'s doc for the full
    /// three-layer collision rule). Scope: `AnalyzeRequest` only (the `analyze`/`analyzeTrees` paths) —
    /// envelope analysis takes packs via `packsDir` only for now; `EnvelopeAnalyzeRequest` has no
    /// equivalent field.
    pub pack_defs: Vec<zzop_core::RulePackDef>,
    pub cache_dir: Option<String>,
    pub git: Option<GitOptionsRequest>,
    pub size_cap: Option<usize>,
    pub disabled_rules: Vec<String>,
    /// Per-rule severity remap (rule id -> `"critical"`/`"warning"`/`"info"`). Reuses `zzop_core::Severity`
    /// (lowercase serde) and `RuleConfig::severity_overrides` directly. Default: empty (no remaps).
    pub severity_overrides: BTreeMap<String, Severity>,
    /// Finding-level accept-list — `{rule, path?}` entries dropping matching findings. Reuses
    /// `zzop_core::Suppression`/`RuleConfig::suppressions` directly. Default: empty (nothing suppressed).
    pub suppressions: Vec<Suppression>,
    /// Config-wide, rule-agnostic finding-level filter — the top-level `"exclude"` config key's napi
    /// exposure (camelCase `globalExcludes`). `{path?, glob?}` entries drop matching findings from EVERY
    /// rule at once (the file is still analyzed; only findings are filtered). Reuses
    /// `zzop_core::GlobalExclude`/`RuleConfig::global_excludes` directly. Default: empty (nothing globally
    /// excluded).
    pub global_excludes: Vec<GlobalExclude>,
    /// Mode-B adapter overlays: partial `NormalizedEnvelope`s (typically just `io` + fragment channels
    /// for a handful of files) merged ON TOP of native TypeScript analysis for this tree — the napi
    /// exposure of `EngineConfig::adapter_overlays`. Each overlay is re-validated and soft-skipped with a
    /// warning if invalid (see `envelope::apply_adapter_overlays`); a structurally-unparseable overlay
    /// fails request deserialization (producer's contract to emit well-formed envelopes). Overlays are
    /// re-applied every run AFTER the native cache, so they need no cache-key participation.
    pub adapter_overlays: Vec<zzop_core::NormalizedEnvelope>,
    /// Deployment-topology "whole-tree" mount point — the napi exposure of an implicit
    /// `zzop_engine::MountRule { dir: String::new(), at: mounted_at }` covering the entire tree (the
    /// engine's own longest-`dir`-wins rule makes this the lowest-specificity entry: any `mounts[]` entry
    /// with a non-empty `dir` beats it on a match). `None` (the default) adds no implicit whole-tree
    /// mount. See `build_engine_config`'s fold order for exactly how this combines with `mounts`. Shape
    /// (must start with `/`, no scheme/placeholder/whitespace) is NOT validated here — that is the
    /// mapper's fail-fast gate (`packages/cli/lib/mapper.js`); the engine's own
    /// `analyze::compose::apply_config_mounts` defensively warns and skips a malformed value as a
    /// last-resort backstop.
    pub mounted_at: Option<String>,
    /// Deployment-topology mounts, in array order — the napi exposure of
    /// `zzop_engine::EngineConfig::mounts` (see that field's doc for the longest-`dir`-wins matching rule
    /// `apply_config_mounts` applies at assemble time). Empty (the default) declares no mounts beyond
    /// `mounted_at`. Same "mapper validates, napi passes through, engine defensively backstops" contract
    /// as `mounted_at`.
    pub mounts: Vec<MountEntryRequest>,
    /// Hosts this tree owns — the napi exposure of `zzop_engine::EngineConfig::hosts` (absolute-URL
    /// consumes to these hosts are re-keyed internal at cross-layer link time, see
    /// `zzop_core::LinkOptions::internal_hosts`). Empty (the default) declares no hosts.
    pub hosts: Vec<String>,
}

/// One `AnalyzeRequest::mounts` entry: `{dir, at}` — the napi exposure of `zzop_engine::MountRule`,
/// field-for-field. `#[serde(rename_all = "camelCase")]` is a no-op today (`dir`/`at` are already single
/// lowercase words) but kept for consistency with every other request struct at this boundary. No shape
/// validation happens here (empty/leading-slash/scheme/backslash/etc.) — see `AnalyzeRequest::mounts`'s
/// doc for why that is deliberately the mapper's job, not this layer's.
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MountEntryRequest {
    pub dir: String,
    pub at: String,
}

/// `AnalyzeRequest::git`'s payload — mirrors `zzop_engine::GitOptions` field-for-field, as JSON input.
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct GitOptionsRequest {
    pub since: Option<String>,
    pub recent_days: Option<u32>,
    /// Custom commit-type classifier table — the napi exposure of config `git.commitTypePatterns`.
    /// REPLACES `zzop_metrics::default_commit_type_patterns()` entirely when present and non-empty (match
    /// order = array order); absent or an empty array falls back to the default table. See
    /// `zzop_engine::GitOptions::commit_type_patterns`'s doc for the full contract, including how an
    /// invalid regex is handled (skipped, surfaced as a `warnings` entry, never a panic).
    pub commit_type_patterns: Option<Vec<CommitTypePatternRequest>>,
}

/// One `git.commitTypePatterns` config-file entry: `{ pattern: <regex>, tag: <TAG> }`. A dedicated struct
/// (rather than accepting a raw 2-element JSON array over the wire) keeps the shape self-describing for a
/// config-file author; `build_engine_config` flattens the list into the `(String, String)` tuple pairs
/// `zzop_engine::GitOptions::commit_type_patterns` / `zzop_git::CollectOptions::commit_type_patterns` use
/// internally.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitTypePatternRequest {
    pub pattern: String,
    pub tag: String,
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
    /// Per-rule severity remap (rule id -> `"critical"`/`"warning"`/`"info"`). See `AnalyzeRequest`.
    pub severity_overrides: BTreeMap<String, Severity>,
    /// Finding-level accept-list — `{rule, path?}` entries. See `AnalyzeRequest`.
    pub suppressions: Vec<Suppression>,
    /// Config-wide, rule-agnostic finding-level filter. See `AnalyzeRequest::global_excludes`.
    pub global_excludes: Vec<GlobalExclude>,
}

/// The shared "seed `pack_defs`, load `packs_dir`, build the DSL-pack list + `RuleConfig`" step both
/// `build_engine_config` (tree-rooted requests) and `analyze_envelope_json` (envelope requests) need.
///
/// The pack list is built in two layers, in this order:
/// 1. `pack_defs` (inline, data-injected packs — see `AnalyzeRequest::pack_defs`) seed the list first, in
///    array order; a same-id collision AMONG `pack_defs` themselves follows the same later-wins-whole rule
///    as step 2 below (reusing the identical collision loop).
/// 2. `packs_dirs` is loaded in order, one `zzop_core::pack_loader::load_dsl_packs` call per directory, and
///    merged into the same list: if a loaded pack (from any directory, or from step 1's `pack_defs`) shares
///    a `RulePackDef::id` with a pack already in the list, the LATER one REPLACES the earlier one whole —
///    not a rule-level merge inside that pack id. Since directories are always folded in AFTER `pack_defs`,
///    a directory pack always wins a same-id collision against an inline def — this is the intentional
///    override path (see `docs/modules/napi.md`'s "Defaults" section) — the JS wrapper (`index.js`) puts
///    the bundled default pack dir first and any caller-supplied `packsDir` after it, so a caller's pack
///    always wins a collision against a shipped one with the same id, while packs with distinct ids from
///    every source all stay loaded together. Per-directory load errors (a malformed `rules/dsl/*.json`, an
///    unreadable directory) are pushed onto `warnings` rather than failing the whole call — same "surface,
///    don't crash" contract `load_dsl_packs` itself documents; the caller folds `warnings` into the
///    corresponding `AnalyzeOutput`.
#[allow(clippy::too_many_arguments)]
fn base_engine_config(
    source_id: &str,
    pack_defs: &[RulePackDef],
    packs_dirs: &[&str],
    disabled_rules: &[String],
    severity_overrides: &BTreeMap<String, Severity>,
    suppressions: &[Suppression],
    global_excludes: &[GlobalExclude],
    warnings: &mut Vec<String>,
) -> EngineConfig {
    let mut packs: Vec<RulePackDef> = Vec::new();
    for def in pack_defs {
        match packs.iter_mut().find(|existing| existing.id == def.id) {
            Some(slot) => *slot = def.clone(), // later inline def wins whole on a same-id collision
            None => packs.push(def.clone()),
        }
    }
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
            severity_overrides: severity_overrides.clone(),
            suppressions: suppressions.to_vec(),
            global_excludes: global_excludes.to_vec(),
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
    let mut config = base_engine_config(
        &req.source_id,
        &req.pack_defs,
        &packs_dirs,
        &req.disabled_rules,
        &req.severity_overrides,
        &req.suppressions,
        &req.global_excludes,
        warnings,
    );

    config.size_cap = req.size_cap.unwrap_or(DEFAULT_SIZE_CAP);
    config.cache_dir = req.cache_dir.as_ref().map(PathBuf::from);
    config.git = req.git.as_ref().map(|g| GitOptions {
        since: g.since.clone(),
        recent_days: g
            .recent_days
            .unwrap_or_else(|| GitOptions::default().recent_days),
        commit_type_patterns: g.commit_type_patterns.as_ref().map(|patterns| {
            patterns
                .iter()
                .map(|p| (p.pattern.clone(), p.tag.clone()))
                .collect()
        }),
    });
    // Overlays flow to `analyze_tree`'s unconditional `apply_adapter_overlays` merge; no cache-key
    // impact (applied post-cache, re-applied every run regardless of hit/miss).
    config.adapter_overlays = req.adapter_overlays.clone();

    // Deployment-topology mounts: every `mounts[]` entry folds in FIRST, in array order, followed by
    // `mounted_at` as the implicit whole-tree entry (`dir: ""`) LAST. The engine's own
    // `apply_config_mounts` picks the longest matching `dir` on a match and resolves equal-length ties to
    // the first entry — appending `mounted_at` last so an explicit dir entry of equal length wins ties
    // (an explicit `{dir:"", at:"..."}` mount, the one shape that can tie with `mounted_at`'s empty
    // `dir`, is more specific intent than the shorthand and should win). No shape validation happens here
    // (see `AnalyzeRequest::mounted_at`/`mounts`'s docs) — this is a plain, unchecked pass-through.
    let mut mounts: Vec<MountRule> = req
        .mounts
        .iter()
        .map(|m| MountRule {
            dir: m.dir.clone(),
            at: m.at.clone(),
        })
        .collect();
    if let Some(at) = req.mounted_at.clone() {
        mounts.push(MountRule {
            dir: String::new(),
            at,
        });
    }
    config.mounts = mounts;
    config.hosts = req.hosts.clone();

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

/// JSON view over `zzop_engine::CoverageCensus` — the vocab-free structural coverage census (see that
/// type). Every field is a plain scalar copy (`join_contribution_zero` is the active-blindness FACT: this
/// tree extracted no io while analyzing `files > 0`, so it is invisible to the cross-layer join). camelCase
/// like every other output-facing type at this boundary.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CoverageCensusView {
    files: usize,
    symbols: usize,
    import_edges: usize,
    io_provides: usize,
    io_consumes_keyed: usize,
    io_consumes_unresolved: usize,
    degraded: usize,
    join_contribution_zero: bool,
}

impl From<&zzop_engine::CoverageCensus> for CoverageCensusView {
    fn from(c: &zzop_engine::CoverageCensus) -> Self {
        CoverageCensusView {
            files: c.files,
            symbols: c.symbols,
            import_edges: c.import_edges,
            io_provides: c.io_provides,
            io_consumes_keyed: c.io_consumes_keyed,
            io_consumes_unresolved: c.io_consumes_unresolved,
            degraded: c.degraded,
            join_contribution_zero: c.join_contribution_zero,
        }
    }
}

/// JSON view over one `zzop_engine::BlindnessClass` — an entry in the pinned silent-failure-class
/// registry (see that type). Static content, identical every run, surfaced so a consumer learns which
/// classes of blindness zzop does and does NOT yet detect (`status`). All fields are `&'static str`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BlindnessClassView {
    id: &'static str,
    group: &'static str,
    summary: &'static str,
    status: &'static str,
}

/// The full registry as a serializable list — attached at the top level of every entry point's output
/// (a run-global honesty channel, never per-tree, so it is emitted once regardless of tree count).
fn disclosure_views() -> Vec<BlindnessClassView> {
    zzop_engine::blindness_registry()
        .iter()
        .map(|c| BlindnessClassView {
            id: c.id,
            group: c.group,
            summary: c.summary,
            status: c.status.as_str(),
        })
        .collect()
}

/// Single-tree output root (`analyze`/`analyzeEnvelope`): the `AnalyzeOutputView` fields, flattened, plus
/// the run-global `disclosure` registry as a sibling. `#[serde(flatten)]` keeps the existing single-tree
/// shape byte-for-byte (every prior field stays at the root) and only adds `disclosure`.
#[derive(Serialize)]
struct SingleTreeOutputView<'a> {
    #[serde(flatten)]
    output: AnalyzeOutputView<'a>,
    disclosure: Vec<BlindnessClassView>,
}

impl<'a> SingleTreeOutputView<'a> {
    fn of(output: &'a AnalyzeOutput) -> Self {
        SingleTreeOutputView {
            output: AnalyzeOutputView::of(output),
            disclosure: disclosure_views(),
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
    /// Structural coverage census — always present (post-aggregate, never git-gated).
    coverage: CoverageCensusView,
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
            coverage: CoverageCensusView::from(&output.coverage),
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

    serde_json::to_string(&SingleTreeOutputView::of(&output))
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
        /// The 22 `cross-layer/*` native rules run over `cross_layer` (`zzop_engine::analyze_trees`'s own
        /// `MultiAnalyzeOutput::cross_layer_findings` field — a plain `&'a [Finding]` borrow, same
        /// zero-copy-view convention as every other field on this struct, since `Finding` already derives
        /// `Serialize` in `zzop-core`).
        cross_layer_findings: &'a [Finding],
        /// Run-global silent-failure-class registry — emitted once (not per tree), same content as the
        /// single-tree output's `disclosure`.
        disclosure: Vec<BlindnessClassView>,
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
        disclosure: disclosure_views(),
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
        &[], // `EnvelopeAnalyzeRequest` has no `pack_defs` — envelope analysis takes packs via `packsDir` only.
        &packs_dirs,
        &req.disabled_rules,
        &req.severity_overrides,
        &req.suppressions,
        &req.global_excludes,
        &mut warnings,
    );
    let mut output = zzop_engine::analyze_envelope(&envelope, &config);
    warnings.append(&mut output.warnings);
    output.warnings = warnings;

    serde_json::to_string(&SingleTreeOutputView::of(&output))
        .map_err(|e| format!("zzop-napi: failed to serialize analyzeEnvelope() output: {e}"))
}

/// A JSON-serializable `{valid, issues}` report — [`validate_envelope_only_json`]'s only output shape.
#[derive(Serialize)]
struct ValidateReport {
    valid: bool,
    issues: Vec<String>,
}

/// `validateEnvelopeOnly(envelopeJson)`: runs `zzop_core::validate_envelope` alone — no `configJson`, no
/// pack loading, no `zzop_engine::analyze_envelope` — and reports the result as a JSON `{"valid": bool,
/// "issues": ["..."]}`. This is `analyze_envelope_json`'s validation half (see its use of
/// `zzop_core::validate_envelope` above) split out on its own so an external adapter author gets fast,
/// offline "is my envelope well-formed" feedback (`zzop adapter validate <path>`) without needing a full
/// engine run or even a `configJson` at all.
///
/// Unlike every other `*_json` function in this module, this one never fails: an unparseable or
/// semantically invalid envelope still produces an ordinary `{"valid": false, "issues": [...]}` report,
/// not an `Err` — a validity CHECK cannot itself be "wrong" the way a malformed request can, so there is
/// nothing here for `addon.rs`'s `catch` to turn into a JS `Error` except an actual panic.
pub fn validate_envelope_only_json(envelope_json: &str) -> String {
    let report = match zzop_core::validate_envelope(envelope_json) {
        Ok(_) => ValidateReport {
            valid: true,
            issues: Vec::new(),
        },
        Err(issues) => ValidateReport {
            valid: false,
            issues,
        },
    };

    serde_json::to_string(&report).unwrap_or_else(|e| {
        format!(
            r#"{{"valid":false,"issues":["zzop-napi: failed to serialize validate report: {e}"]}}"#
        )
    })
}

/// `version()`: this crate's own Cargo version plus every parser's
/// `PARSER_FINGERPRINT` (`zzop-cache`'s cache-key ingredient — see `zzop_parser_typescript::PARSER_FINGERPRINT`'s
/// doc), so a host app can log/report exactly which parser build produced a given analysis without needing
/// its own copy of those constants.
///
/// Note: `env!("CARGO_PKG_VERSION")` now resolves to `zzop-facade`'s own crate version, not
/// `zzop-napi`'s — identical today since every workspace crate shares `version.workspace = true`, but a
/// trap if versions ever diverge, since the string below still prefixes the number with `"zzop-napi/"`.
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
    /// `crates/engine/tests/analyze_git.rs`'s `git_fixture_repo`) built to exercise every `HashMap`-typed
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
    fn analyze_envelope_json_suppressions_drop_a_finding() {
        // Same suppression path, exercised through the envelope entry point (`analyze_envelope_json` ->
        // `base_engine_config`). Two files importing each other form a cycle -> a `circular` finding.
        let envelope = r#"{
            "format": "zzop-normalized-ast",
            "version": 1,
            "parser": "test/1",
            "source": "legacy",
            "files": [
                {"path": "a.ts", "loc": 2, "imports": {"b": {"specifier": "b.ts", "original": "default"}}},
                {"path": "b.ts", "loc": 2, "imports": {"a": {"specifier": "a.ts", "original": "default"}}}
            ]
        }"#;
        let baseline = analyze_envelope_json(envelope, r#"{"sourceId": "legacy"}"#)
            .expect("analyze_envelope_json should succeed");
        let baseline_value: serde_json::Value = serde_json::from_str(&baseline).unwrap();
        assert!(
            baseline_value["findings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["ruleId"] == "circular"),
            "fixture must produce a circular finding without suppression, got: {baseline_value}"
        );

        let suppressed = analyze_envelope_json(
            envelope,
            r#"{"sourceId": "legacy", "suppressions": [{"rule": "circular"}]}"#,
        )
        .expect("analyze_envelope_json should succeed");
        let value: serde_json::Value = serde_json::from_str(&suppressed).unwrap();
        assert!(
            !value["findings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["ruleId"] == "circular"),
            "suppressions must drop the circular finding in envelope mode, got: {value}"
        );
    }

    #[test]
    fn analyze_request_adapter_overlays_flow_into_engine_config() {
        // Plumbing-only: proves the napi-facing `adapterOverlays` JSON field deserializes into
        // `AnalyzeRequest::adapter_overlays` and survives `build_engine_config` into
        // `EngineConfig::adapter_overlays` unchanged. The overlay MERGE itself (into a real
        // `analyze_tree` run) is already covered end-to-end by
        // `crates/engine/tests/analyze_adapter_overlay.rs` — this test never touches a filesystem
        // root, since `build_engine_config` doesn't need one to build the config.
        let config_json = r#"{
            "root": "unused",
            "sourceId": "t",
            "adapterOverlays": [
                {
                    "format": "zzop-normalized-ast",
                    "version": 1,
                    "parser": "test-adapter/1",
                    "source": "legacy",
                    "files": [
                        {
                            "path": "a.ts",
                            "loc": 10,
                            "io": {
                                "provides": [
                                    {"kind": "http", "key": "GET /foo", "file": "a.ts", "line": 1}
                                ],
                                "consumes": []
                            }
                        }
                    ]
                }
            ]
        }"#;
        let req: AnalyzeRequest =
            serde_json::from_str(config_json).expect("valid AnalyzeRequest JSON");
        assert_eq!(
            req.adapter_overlays.len(),
            1,
            "expected the field to deserialize"
        );

        let mut warnings = Vec::new();
        let config = build_engine_config(&req, &mut warnings);
        assert_eq!(
            config.adapter_overlays.len(),
            1,
            "expected adapterOverlays to flow into EngineConfig::adapter_overlays"
        );
        assert_eq!(config.adapter_overlays[0].parser, "test-adapter/1");
        assert_eq!(
            config.adapter_overlays[0].files[0].io.provides[0].key, "GET /foo",
            "expected the overlay's io.provides entry to survive the round trip"
        );
    }

    #[test]
    fn analyze_request_git_commit_type_patterns_flow_into_engine_config() {
        // Plumbing-only, same spirit as `analyze_request_adapter_overlays_flow_into_engine_config`: proves
        // the napi-facing `git.commitTypePatterns` JSON field deserializes into
        // `GitOptionsRequest::commit_type_patterns` and survives `build_engine_config` into
        // `EngineConfig::git`'s `GitOptions::commit_type_patterns` unchanged, as `(String, String)` tuple
        // pairs. The end-to-end tagging behavior (a custom table actually reclassifying a commit) is
        // covered by `crates/engine/tests/analyze_git.rs`'s git-fixture tests instead.
        let config_json = r#"{
            "root": "unused",
            "sourceId": "t",
            "git": {
                "commitTypePatterns": [
                    { "pattern": "^\\s*corrige\\b", "tag": "FIX" },
                    { "pattern": "^\\s*nouveau\\b", "tag": "FEAT" }
                ]
            }
        }"#;
        let req: AnalyzeRequest =
            serde_json::from_str(config_json).expect("valid AnalyzeRequest JSON");
        let git_req = req.git.as_ref().expect("expected git to deserialize");
        let patterns = git_req
            .commit_type_patterns
            .as_ref()
            .expect("expected commitTypePatterns to deserialize");
        assert_eq!(patterns.len(), 2);
        assert_eq!(patterns[0].pattern, "^\\s*corrige\\b");
        assert_eq!(patterns[0].tag, "FIX");

        let mut warnings = Vec::new();
        let config = build_engine_config(&req, &mut warnings);
        let git_cfg = config.git.expect("expected EngineConfig::git to be Some");
        assert_eq!(
            git_cfg.commit_type_patterns,
            Some(vec![
                ("^\\s*corrige\\b".to_string(), "FIX".to_string()),
                ("^\\s*nouveau\\b".to_string(), "FEAT".to_string()),
            ])
        );
    }

    #[test]
    fn analyze_request_git_without_commit_type_patterns_leaves_it_none() {
        // Absence must round-trip to `None` (falls back to the default table downstream), not an empty
        // `Some(vec![])` that would also be treated as "fall back" but is a different wire shape to pin.
        let config_json = r#"{"root": "unused", "sourceId": "t", "git": {}}"#;
        let req: AnalyzeRequest =
            serde_json::from_str(config_json).expect("valid AnalyzeRequest JSON");
        let git_req = req.git.as_ref().expect("expected git to deserialize");
        assert!(git_req.commit_type_patterns.is_none());

        let mut warnings = Vec::new();
        let config = build_engine_config(&req, &mut warnings);
        let git_cfg = config.git.expect("expected EngineConfig::git to be Some");
        assert!(git_cfg.commit_type_patterns.is_none());
    }

    #[test]
    fn analyze_request_mounted_at_mounts_hosts_flow_into_engine_config() {
        // Plumbing-only, same spirit as `analyze_request_adapter_overlays_flow_into_engine_config`: proves
        // `mountedAt`/`mounts`/`hosts` deserialize and that `build_engine_config` folds every `mounts[]`
        // entry in array order FIRST, followed by `mountedAt` as the implicit `dir: ""` entry LAST — so
        // the engine's first-wins equal-length tie-break favors an explicit mount over the shorthand.
        let config_json = r#"{
            "root": "unused",
            "sourceId": "t",
            "mountedAt": "/gateway",
            "mounts": [
                { "dir": "apps/api", "at": "/api" },
                { "dir": "apps/admin", "at": "/admin" }
            ],
            "hosts": ["internal.example.com"]
        }"#;
        let req: AnalyzeRequest =
            serde_json::from_str(config_json).expect("valid AnalyzeRequest JSON");
        assert_eq!(req.mounted_at.as_deref(), Some("/gateway"));
        assert_eq!(req.mounts.len(), 2);
        assert_eq!(req.hosts, vec!["internal.example.com".to_string()]);

        let mut warnings = Vec::new();
        let config = build_engine_config(&req, &mut warnings);
        assert_eq!(
            config.mounts.len(),
            3,
            "expected both mounts[] entries first, then mountedAt"
        );
        assert_eq!(config.mounts[0].dir, "apps/api");
        assert_eq!(config.mounts[0].at, "/api");
        assert_eq!(config.mounts[1].dir, "apps/admin");
        assert_eq!(config.mounts[1].at, "/admin");
        assert_eq!(
            config.mounts[2].dir, "",
            "mountedAt becomes the dir \"\" entry, appended LAST so an explicit equal-length dir entry \
             (e.g. an explicit {{dir:\"\", at:...}} mount) wins the engine's first-wins tie-break over \
             the mountedAt shorthand"
        );
        assert_eq!(config.mounts[2].at, "/gateway");
        assert_eq!(config.hosts, vec!["internal.example.com".to_string()]);
    }

    #[test]
    fn analyze_request_without_mounted_at_omits_the_implicit_whole_tree_mount() {
        let config_json = r#"{
            "root": "unused",
            "sourceId": "t",
            "mounts": [ { "dir": "apps/api", "at": "/api" } ]
        }"#;
        let req: AnalyzeRequest =
            serde_json::from_str(config_json).expect("valid AnalyzeRequest JSON");
        assert!(req.mounted_at.is_none());

        let mut warnings = Vec::new();
        let config = build_engine_config(&req, &mut warnings);
        assert_eq!(
            config.mounts.len(),
            1,
            "no mountedAt -> no implicit dir \"\" entry"
        );
        assert_eq!(config.mounts[0].dir, "apps/api");
        assert_eq!(config.mounts[0].at, "/api");
    }

    #[test]
    fn analyze_request_defaults_mounted_at_mounts_hosts_to_empty() {
        let config_json = r#"{"root": "unused", "sourceId": "t"}"#;
        let req: AnalyzeRequest =
            serde_json::from_str(config_json).expect("valid AnalyzeRequest JSON");
        assert!(req.mounted_at.is_none());
        assert!(req.mounts.is_empty());
        assert!(req.hosts.is_empty());

        let mut warnings = Vec::new();
        let config = build_engine_config(&req, &mut warnings);
        assert!(config.mounts.is_empty());
        assert!(config.hosts.is_empty());
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

    /// A one-rule DSL pack JSON matching `crates/core/src/pack_loader.rs`'s `valid_pack` shape — field
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
    fn analyze_json_pack_defs_inline_pack_fires_without_packs_dir() {
        // No `packsDir` at all — the inline `packDefs` pack must load and fire on its own.
        let dir = cycle_fixture();
        dir.write("marker.ts", "// MARKER_A\n");

        let config = format!(
            r#"{{"root": {:?}, "packDefs": [{}]}}"#,
            dir.path().display(),
            dsl_pack_json("pack-a", "r1", "MARKER_A")
        );
        let out = analyze_json(&config).expect("analyze_json should succeed");
        let value: serde_json::Value = serde_json::from_str(&out).unwrap();
        let findings = value["findings"].as_array().expect("findings array");
        assert!(
            findings.iter().any(|f| f["ruleId"] == "pack-a/r1"),
            "expected the inline packDefs rule to fire without any packsDir, got: {value}"
        );
    }

    #[test]
    fn analyze_json_pack_defs_collision_with_packs_dir_directory_pack_wins() {
        // Same pack id from both an inline `packDefs` entry and a `packsDir` directory: the directory
        // pack loads AFTER `pack_defs` in `base_engine_config`'s seed order, so it must win the
        // collision whole — same "later wins whole" rule, applied across the two layers.
        let dir = cycle_fixture();
        dir.write("marker.ts", "// INLINE_MARKER\n// DIR_MARKER\n");

        let packs_dir_pack = TempDir::new("zzop-napi-packs-dir-collision");
        packs_dir_pack.write(
            "custom.json",
            &dsl_pack_json("custom", "marker-dir", "DIR_MARKER"),
        );

        let config = format!(
            r#"{{"root": {:?}, "packDefs": [{}], "packsDir": {:?}}}"#,
            dir.path().display(),
            dsl_pack_json("custom", "marker-inline", "INLINE_MARKER"),
            packs_dir_pack.path().display()
        );
        let out = analyze_json(&config).expect("analyze_json should succeed");
        let value: serde_json::Value = serde_json::from_str(&out).unwrap();
        let findings = value["findings"].as_array().expect("findings array");
        assert!(
            findings.iter().any(|f| f["ruleId"] == "custom/marker-dir"),
            "expected the packsDir directory pack's rule to fire (a directory pack beats an inline def \
             with the same id), got: {value}"
        );
        assert!(
            !findings.iter().any(|f| f["ruleId"] == "custom/marker-inline"),
            "expected the inline packDefs pack to be fully replaced by the packsDir directory pack, \
             got: {value}"
        );
    }

    #[test]
    fn analyze_json_pack_defs_same_id_later_def_wins() {
        // Two inline `packDefs` entries sharing an id: the later array entry must replace the earlier
        // one whole, mirroring the packsDir array's own same-id collision rule.
        let dir = cycle_fixture();
        dir.write("marker.ts", "// OLD_MARKER\n// NEW_MARKER\n");

        let config = format!(
            r#"{{"root": {:?}, "packDefs": [{}, {}]}}"#,
            dir.path().display(),
            dsl_pack_json("custom", "marker-old", "OLD_MARKER"),
            dsl_pack_json("custom", "marker-new", "NEW_MARKER")
        );
        let out = analyze_json(&config).expect("analyze_json should succeed");
        let value: serde_json::Value = serde_json::from_str(&out).unwrap();
        let findings = value["findings"].as_array().expect("findings array");
        assert!(
            findings.iter().any(|f| f["ruleId"] == "custom/marker-new"),
            "expected the later inline def's rule to fire, got: {value}"
        );
        assert!(
            !findings.iter().any(|f| f["ruleId"] == "custom/marker-old"),
            "expected the earlier inline def with the same id to be fully replaced, got: {value}"
        );
    }

    #[test]
    fn analyze_request_defaults_pack_defs_to_empty() {
        // `packDefs` absent from request JSON must behave identically to before this field existed —
        // an empty `Vec`, contributing nothing to `base_engine_config`'s seed layer.
        let config_json = r#"{"root": "unused", "sourceId": "t"}"#;
        let req: AnalyzeRequest =
            serde_json::from_str(config_json).expect("valid AnalyzeRequest JSON");
        assert!(
            req.pack_defs.is_empty(),
            "packDefs absent from request JSON must default to empty"
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
    fn validate_envelope_only_json_reports_valid_for_a_well_formed_envelope() {
        let out = validate_envelope_only_json(&tiny_envelope_json());
        let value: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
        assert_eq!(value["valid"], true);
        assert_eq!(value["issues"].as_array().expect("issues array").len(), 0);
    }

    #[test]
    fn validate_envelope_only_json_lists_issues_for_a_broken_envelope() {
        let bad = tiny_envelope_json().replace("zzop-normalized-ast", "bogus-format");
        let out = validate_envelope_only_json(&bad);
        let value: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
        assert_eq!(value["valid"], false);
        let issues = value["issues"].as_array().expect("issues array");
        assert!(
            issues
                .iter()
                .any(|i| i.as_str().unwrap().contains("unknown format")),
            "expected an 'unknown format' issue, got: {value}"
        );
    }

    #[test]
    fn validate_envelope_only_json_never_fails_on_unparseable_input() {
        let out = validate_envelope_only_json("not json");
        let value: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
        assert_eq!(value["valid"], false);
        let issues = value["issues"].as_array().expect("issues array");
        assert!(issues
            .iter()
            .any(|i| i.as_str().unwrap().contains("invalid JSON")));
    }

    #[test]
    fn version_string_includes_parser_fingerprints() {
        let v = version_string();
        assert!(v.contains("zzop-parser-typescript="));
        assert!(v.contains("zzop-parser-prisma="));
    }
}
