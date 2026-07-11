//! zzop-engine — the fused execution engine: per-file parse + DSL-rule pass (`pipeline.rs`), then a
//! whole-graph assembly pass (`analyze.rs`), composed for public consumption by `analyze_tree`.
//!
//! Implements the two design principles this crate is built against:
//! - **Fused execution**: per-file DSL rules run **inside** the parse pass, before that file's AST is
//!   dropped; whole-graph rules run after, over the assembled `CommonIr`; a parse failure or oversized file
//!   degrades to a lexical fallback and is recorded, never crashes the pipeline.
//! - **The 2-layer IR**: a
//!   Normalized-AST layer that never leaves the parser's own call frame, projected into the
//!   language-neutral Common IR the engine and rules consume; oxlint-style single traversal (no per-rule
//!   re-walk) + rayon file parallelism.
//!
//! ## Deliberate implementation choices (see also doc comments at each cited site)
//! - **TS parse-failure signal** (`pipeline::parse_typescript`'s doc): `zzop-parser-typescript`'s public
//!   API swallows a swc syntax error into an empty, successful-looking result — there is no `Result`/
//!   panic this crate can observe to tell "broken file" apart from "legitimately empty file" without a
//!   direct swc dependency (rejected — this crate depends only on the parser crates, never swc itself).
//!   This crate adds one local, swc-independent heuristic (`looks_structurally_broken`: brace/paren/
//!   bracket balance, comment/string aware) as a pre-check ahead of the real parse call. `catch_unwind`
//!   still wraps the actual parse calls underneath as defense in depth.
//! - **IoFacts / io-scan wiring**: wired in `pipeline::process_file` via `crate::io::extract_file_io`,
//!   one call per well-formed TypeScript file, against a **single-file slice** of the two TS-side
//!   adapters that were designed for a project-wide call. File-local constant/const-map indirection
//!   still resolves; a shared constants module imported from ANOTHER file does not resolve at that
//!   one-file call site, but is no longer lost — `analyze::late_resolve_cross_file_consumes` merges
//!   every file's constant-map fragment and re-resolves the consume before assembly freezes
//!   `MinimalIr::io`. A sub-router mounted from a different file is the one shape still unresolved — the
//!   endpoint is simply absent from `provides` rather than crashing or fabricating a wrong key.
//! - **TS dep-graph resolution scope**: `resolve::build_dep` runs once, in the assembly phase
//!   (`analyze::assemble`), over every TypeScript-dispatched file's `rel` path and `ImportMap` — never
//!   per-file inside the fused pass. A file's own outgoing edges depend on which *other* files exist in
//!   the tree, so it cannot be resolved until every file has been walked and dispatched. Only
//!   TypeScript-dispatched files participate — Prisma has no import syntax, and lexical-only files were
//!   never parsed for imports in the first place.

mod analyze;
mod cache;
mod coverage;
mod dead_exports;
mod disclosure;
mod dispatch;
mod envelope;
mod file_routes;
mod framework_silence;
mod io;
mod pipeline;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use zzop_core::{
    dsl::RuleTiming, CommonIr, FileNode, Finding, RuleConfig, RulePackDef, RuleRegistry, SourceIo,
};
use zzop_metrics::{
    CriticalFile, CrossLayerCoChurn, FolderAggregates, HealthIndex, Recommendation, Scores,
    ScoresConfig, SeamCandidate,
};

pub use coverage::CoverageCensus;
pub use disclosure::{blindness_registry, BlindnessClass, DisclosureStatus};
pub use dispatch::{DispatchConfig, Language};
pub use envelope::analyze_envelope;
pub use io::IoOptions;

/// Composes every crate's own `register_native_analyses` into one `RuleRegistry` — the engine aggregator
/// half of the extensibility contract (`rules/README.md`'s "Adding a rule" section). The kernel
/// (`zzop_core`) itself registers nothing; native analyses live in their owning crates.
pub fn register_all_native(registry: &mut RuleRegistry) {
    zzop_rules_graph::register_native_analyses(registry);
    zzop_rules_http::register_native_analyses(registry);
    zzop_rules_cross_layer::register_native_analyses(registry);
    zzop_rules_schema::register_native_analyses(registry);
    zzop_metrics::register_native_analyses(registry);
}

/// A size cap above which a file skips structural parsing entirely and falls back to a lexical count
/// (`pipeline::process_file`'s oversized branch) — the "graceful degrade on oversized file" half of the
/// fusion contract. ~1.5MB: large enough that no realistic hand-written source file hits it, small enough
/// that a generated/vendored/minified file does not blow up parse time or memory in the per-file pass.
pub const DEFAULT_SIZE_CAP: usize = 1_500_000;

/// Engine configuration for one `analyze_tree` call. `packs` is already-loaded `RulePackDef`s (e.g. via
/// `zzop_core::pack_loader::load_dsl_packs`) — this crate does not read `rules/dsl/*.json` off disk itself,
/// keeping "where rule packs live" a caller concern (a CLI, a test, an N-API host).
pub struct EngineConfig {
    /// Tags the assembled `CommonIr`'s `source` field (zzop's multi-tree / cross-layer-join convention).
    pub source_id: String,
    pub dispatch: DispatchConfig,
    /// Files strictly larger than this (in bytes) skip structural parsing (see `DEFAULT_SIZE_CAP`).
    pub size_cap: usize,
    pub rule_config: RuleConfig,
    pub packs: Vec<RulePackDef>,
    /// Router-identifier-name config for the per-file Hono-route provide projection — see `crate::io`.
    pub io: IoOptions,
    /// When `Some`, `analyze_tree` runs `zzop_git::collect` over `root` and, if it succeeds, builds real
    /// `FileNode`s from the collected history and computes `scores`/`health`/`recommendations`/
    /// `critical`/`seams`. `None` (the default) leaves those fields empty/`None`; no git process is ever
    /// spawned. A `Some` on a non-git root does not panic — see `AnalyzeOutput::warnings`.
    pub git: Option<GitOptions>,
    /// Override for `zzop_metrics::compute_scores`'s threshold/vocabulary config. Only consulted when
    /// `git` is `Some` and collection succeeds.
    pub scores_config: ScoresConfig,
    /// When `Some`, `analyze_tree` opens (creating if absent) a `zzop_cache::AnalysisCache` at this path
    /// and drives the fused per-file pass through it: a file whose content hash + parser fingerprint +
    /// ruleset fingerprint already has a cached IR *and* findings entry skips parsing and rule
    /// evaluation entirely. `None` (the default) never touches a cache directory. A cache directory that
    /// fails to open degrades to "cache off" for that call plus a `warnings` entry — never a panic.
    pub cache_dir: Option<PathBuf>,
    /// Rule profiling — the ESLint `TIMING=1` / oxlint rule-timing equivalent. `false` (the default)
    /// leaves `AnalyzeOutput::rule_timings` at `None` with zero added cost. `true` times each DSL rule
    /// and each whole-graph native analysis that actually runs. Profiling never changes
    /// `findings`/`ir` — only which optional field is populated.
    pub profile_rules: bool,
    /// Partial envelopes (`io` + fragment channels only, typically) merged onto the native per-file
    /// artifacts before whole-tree assembly — the external-adapter injection point for a framework
    /// adapter that wants to participate in a NATIVE `analyze_tree` run without reimplementing a parser
    /// (contrast with Mode A, `analyze_envelope`, a full envelope standing in for the entire tree).
    /// Empty (the default) runs no overlay processing. Each overlay is
    /// `zzop_core::validate_envelope`-checked; an invalid one is skipped with a `warnings` entry.
    pub adapter_overlays: Vec<zzop_core::NormalizedEnvelope>,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            source_id: String::new(),
            dispatch: DispatchConfig::default(),
            size_cap: DEFAULT_SIZE_CAP,
            rule_config: RuleConfig::default(),
            packs: Vec::new(),
            io: IoOptions::default(),
            git: None,
            scores_config: ScoresConfig::default(),
            cache_dir: None,
            profile_rules: false,
            adapter_overlays: Vec::new(),
        }
    }
}

/// Git-history collection options for `EngineConfig::git` — a thin mirror of `zzop_git::CollectOptions`.
/// `zzop_metrics::default_commit_type_patterns()` is used for the commit-type vocabulary UNLESS
/// `commit_type_patterns` supplies a custom table (see that field's doc).
#[derive(Debug, Clone)]
pub struct GitOptions {
    /// `git log --since=<since>`; `None` = full history.
    pub since: Option<String>,
    /// Window, in days, for each `FileNode`'s `recent_*` fields.
    pub recent_days: u32,
    /// Custom commit-type classifier table (regex source, TAG pairs, in match order) — the config-file
    /// wire path for `git.commitTypePatterns` (napi `GitOptionsRequest::commit_type_patterns`). When
    /// `Some` and non-empty, this REPLACES `zzop_metrics::default_commit_type_patterns()` entirely (same
    /// "later table wins whole, not merged" semantics the default table's own REVERT-first ordering
    /// depends on) — match order is array order. `None`, or `Some(vec![])`, falls back to the default
    /// table. See `analyze::diagnostics::collect_git` for where this is applied, and
    /// `zzop_git::tags::CommitClassifiers::compile`'s doc for what happens to a pattern that fails to
    /// compile as a regex (skipped, never a panic; `collect_git` additionally surfaces a `warnings` entry
    /// naming any such pattern, since a silently-inert custom pattern is exactly the narrowed-scope
    /// degradation this codebase's self-report contract exists for).
    pub commit_type_patterns: Option<Vec<(String, String)>>,
}

impl Default for GitOptions {
    fn default() -> Self {
        GitOptions {
            since: None,
            recent_days: 30,
            commit_type_patterns: None,
        }
    }
}

/// The result of one `analyze_tree` call: the assembled tree-wide Common IR, every finding
/// (per-file DSL + whole-graph native, merged/sorted via `zzop_core::merge_findings`), which files
/// degraded to a lexical fallback, and the total file count the walk visited.
///
/// `nodes` is always populated (dep-graph + LOC only when `EngineConfig::git` is `None`, real
/// git-derived churn/authors/lifecycle when collection succeeded). `scores`/`health`/`recommendations`/
/// `critical`/`seams`/`layer_co_churn` are the git-history-dependent analyses: they stay at their empty
/// value whenever `EngineConfig::git` is `None` or git collection failed (see `warnings`). `folders` is
/// the one exception: it only needs `nodes`/the dep graph (both built unconditionally), so it is `Some`
/// regardless of `git`.
pub struct AnalyzeOutput {
    pub ir: CommonIr,
    pub findings: Vec<Finding>,
    pub degraded: Vec<String>,
    pub file_count: usize,
    /// Structural coverage census — see `CoverageCensus`. Always present (post-aggregate, never
    /// git-gated).
    pub coverage: CoverageCensus,
    /// Per non-relative import specifier: how many files import it + the first importing file. Plumbing
    /// for `cross-layer/sdk-import-no-visible-consume` (the tree IR drops package imports during dep
    /// resolution) — not part of the serialized output surface.
    pub package_imports: Vec<PackageImportSummary>,
    pub nodes: Vec<FileNode>,
    pub scores: Option<Scores>,
    pub health: Option<HealthIndex>,
    pub recommendations: Vec<Recommendation>,
    pub critical: Vec<CriticalFile>,
    pub seams: Vec<SeamCandidate>,
    /// Folder-granularity rollup over `nodes`/`ir.ir.dep` at `zzop_metrics::DEFAULT_FOLDER_DEPTH`. Unlike
    /// `scores`/`health`, this is NOT git-gated — `nodes` and the dep graph are built unconditionally, so
    /// this is `Some` on every call that reaches assembly (never a stand-in for "ran and found nothing":
    /// an empty-but-real tree still gets `Some` with empty `Vec`s).
    pub folders: Option<FolderAggregates>,
    /// Cross-layer co-churn: commit co-changes between files in different architectural layers
    /// (`zzop_metrics::layer_of`, using `EngineConfig::scores_config`'s `hierarchy_shared_dirs`
    /// vocabulary). Git-gated exactly like `scores`/`health`: `None` when git is inactive, `Some`
    /// (possibly an empty `Vec`) when collection succeeded.
    pub layer_co_churn: Option<Vec<CrossLayerCoChurn>>,
    /// Non-fatal diagnostics — e.g. git collection failing, or the cache directory failing to open.
    /// Analysis still completes normally in either case.
    pub warnings: Vec<String>,
    /// Per-file cache hit/miss counts for this call, or `None` when `EngineConfig::cache_dir` was `None`
    /// (including when a `Some` `cache_dir` failed to open — see `warnings`). A file only counts as a
    /// hit when BOTH its IR and findings cache entries were reused; a ruleset-only change that reuses
    /// the IR but re-runs rules still counts that file as a miss.
    pub cache: Option<CacheStats>,
    /// Per-rule / per-native-analysis wall-clock timing (`EngineConfig::profile_rules`), or `None` when
    /// profiling was off. When `Some`, one entry per DSL rule id and per whole-graph native analysis id
    /// that actually ran, sorted by `nanos` descending with a deterministic `rule_id`-ascending
    /// tie-break. `nanos` is wall-clock: expect run-to-run jitter — rank rules by relative cost within
    /// one run, don't diff raw `nanos` across separate runs.
    pub rule_timings: Option<Vec<RuleTiming>>,
}

/// `AnalyzeOutput::cache`'s payload — see that field's doc for what counts as a hit vs a miss.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CacheStats {
    pub hits: usize,
    pub misses: usize,
}

/// Runs the fused engine over every file under `root`: per-file parse + DSL rules (`pipeline`), then
/// whole-graph assembly (`analyze`), including the optional git-history-dependent analyses. Two calls
/// over an unchanged tree (and unchanged git history) produce byte-for-byte identical output.
pub fn analyze_tree(root: &Path, config: &EngineConfig) -> AnalyzeOutput {
    // Input-scope self-report (`input-scope-error` in the disclosure registry): a mistyped root used
    // to be absorbed as an empty tree whose only trace was `files: 0` in the census — which reads as a
    // clean/empty repo. A root that does not exist (or is not a directory) is a structural fact about
    // the REQUEST, disclosed up front; a root that exists but yields zero artifacts self-reports after
    // the walk. A too-narrow root that still matches SOME files stays undetected (registry: partial).
    let mut scope_warnings = Vec::new();
    if !root.is_dir() {
        scope_warnings.push(format!(
            "root '{}' does not exist or is not a directory — analyzed as an empty tree (0 files). Check the path for a typo; every count and finding for this tree is a statement about nothing.",
            root.display()
        ));
    }

    let mut cache_warnings = Vec::new();
    let analysis_cache = cache::open_cache(config, &mut cache_warnings);
    let counters = analysis_cache
        .as_ref()
        .map(|_| cache::CacheCounters::default());

    let mut artifacts =
        pipeline::run_file_pass(root, config, analysis_cache.as_ref(), counters.as_ref());
    if scope_warnings.is_empty() && artifacts.is_empty() {
        scope_warnings.push(format!(
            "0 source files found under root '{}' — this tree contributes nothing to any analysis. If that is unexpected, the root points at the wrong directory or every file was filtered before parsing.",
            root.display()
        ));
    }
    let mut overlay_warnings = Vec::new();
    if !config.adapter_overlays.is_empty() {
        envelope::apply_adapter_overlays(
            &mut artifacts,
            &config.adapter_overlays,
            &mut overlay_warnings,
        );
    }
    let mut output = analyze::assemble(root, artifacts, config);

    // Scope warnings lead: they qualify every other line ("about nothing"), so a reader hits them first.
    if !scope_warnings.is_empty() {
        scope_warnings.append(&mut output.warnings);
        output.warnings = scope_warnings;
    }
    output.warnings.extend(cache_warnings);
    output.warnings.extend(overlay_warnings);
    output.cache = counters.map(cache::CacheCounters::into_stats);
    output
}

/// One `analyze_tree` call's output, per tree, plus the cross-layer join over every tree's IoFacts.
pub struct MultiAnalyzeOutput {
    /// `(root, config.source_id, output)` for each input tree, in the same order as `trees`.
    pub trees: Vec<(PathBuf, String, AnalyzeOutput)>,
    pub cross_layer: zzop_core::CrossLayerResult,
    /// The 22 `cross-layer/*` native rules run over `cross_layer` — see `compute_cross_layer_findings`'s
    /// doc for the gating/derivation/sort contract. Always populated: even a single-tree `analyze_trees`
    /// call runs these (most find nothing, since e.g. `shared-db-table`/`duplicate-route` need 2+
    /// distinct source trees to ever fire).
    pub cross_layer_findings: Vec<Finding>,
}

/// Cross-layer multi-tree API: runs `analyze_tree` once per `(root, config)` pair, then joins every
/// tree's `CommonIr.ir.io` via `zzop_core::link_cross_layer_io` (an exact `(kind, key)` join). Each tree
/// keeps its own `EngineConfig::source_id` as the join's per-tree tag, so a consume in tree A and a
/// provide in tree B join into a `cross_source: true` edge when their normalized keys match. A tree with
/// `ir.io = None` contributes an empty `IoFacts` to the join — never a panic, never a skipped tree.
/// One non-relative (package) import specifier's per-tree summary — see `AnalyzeOutput::package_imports`.
#[derive(Debug, Clone)]
pub struct PackageImportSummary {
    pub specifier: String,
    pub file_count: usize,
    pub example_file: String,
}

pub fn analyze_trees(trees: &[(PathBuf, EngineConfig)]) -> MultiAnalyzeOutput {
    let mut outputs = Vec::with_capacity(trees.len());
    let mut source_ios = Vec::with_capacity(trees.len());
    for (root, config) in trees {
        let output = analyze_tree(root, config);
        source_ios.push(SourceIo {
            source: config.source_id.clone(),
            io: output.ir.ir.io.clone().unwrap_or_default(),
        });
        outputs.push((root.clone(), config.source_id.clone(), output));
    }
    let link_opts = zzop_core::LinkOptions {
        // Default generic-path vocabulary (health/ping/metrics/...) is analysis-domain, not join
        // mechanism, so it lives in `zzop-metrics` rather than `zzop-core`.
        low_confidence_key_patterns: zzop_metrics::default_generic_interface_key_patterns(),
    };
    let cross_layer = zzop_core::link_cross_layer_io(&source_ios, &link_opts);
    let package_imports: Vec<zzop_rules_cross_layer::PackageImportSite> = outputs
        .iter()
        .flat_map(|(_, source, output)| {
            output
                .package_imports
                .iter()
                .map(move |p| zzop_rules_cross_layer::PackageImportSite {
                    source: source.clone(),
                    specifier: p.specifier.clone(),
                    file_count: p.file_count,
                    example_file: p.example_file.clone(),
                })
        })
        .collect();
    // Per-tree, not run-global: a source tree "participates" in a `trpc`-kind edge when it appears on
    // EITHER side (`from.source` or `to.source` — a tree can be the router-defining provider, the caller,
    // or occasionally both for a same-tree edge). `trpc_edge_counts_by_source` counts each edge once per
    // distinct participating source (a same-tree edge, `from.source == to.source`, counts once for that
    // source, not twice). A run-global count here would let tree A's trpc edges suppress/misattribute a
    // literal `/trpc/`-segment route that tree B provides on its own, unrelated deployment — see
    // `zzop_rules_cross_layer::is_trpc_mount_route_key`'s doc.
    let mut trpc_edge_counts_by_source: BTreeMap<String, usize> = BTreeMap::new();
    for e in cross_layer.edges.iter().filter(|e| e.kind == "trpc") {
        let mut participants: Vec<&str> = vec![e.from.source.as_str()];
        if e.to.source != e.from.source {
            participants.push(e.to.source.as_str());
        }
        for source in participants {
            *trpc_edge_counts_by_source
                .entry(source.to_string())
                .or_insert(0) += 1;
        }
    }
    let trpc_participating_sources: BTreeSet<String> =
        trpc_edge_counts_by_source.keys().cloned().collect();
    let cross_layer_findings = compute_cross_layer_findings(
        &source_ios,
        &cross_layer,
        trees,
        &package_imports,
        &trpc_participating_sources,
    );

    // tRPC mount-route suppression disclosure — `unconsumed-endpoint`/`unconsumed-mutation-endpoint`
    // (inside `compute_cross_layer_findings` above) silently excluded any http provide identified as a
    // tRPC mount route whose OWN source tree is in `trpc_participating_sources`; per `output-philosophy.md`
    // §0/§1 (no silent suppression), that exclusion must surface somewhere — pushed onto the OWNING source
    // tree's own `AnalyzeOutput::warnings`, the same per-tree engine self-report channel every other
    // silent-failure disclosure in this crate uses. See
    // `zzop_rules_cross_layer::trpc_mount_route_suppression_notes`'s doc for the message shape and dogfood
    // motivation (round 9). Gated on the SAME rule-enable union the suppression itself runs under: with
    // both unconsumed rules disabled, no finding was suppressed, so a note would disclose a suppression
    // that never happened (a phantom disclosure).
    let disclosure_gate = RuleConfig {
        disabled_rules: trees
            .iter()
            .flat_map(|(_, c)| c.rule_config.disabled_rules.iter().cloned())
            .collect(),
        ..RuleConfig::default()
    };
    if zzop_core::is_enabled(&disclosure_gate, "cross-layer/unconsumed-endpoint")
        || zzop_core::is_enabled(&disclosure_gate, "cross-layer/unconsumed-mutation-endpoint")
    {
        for (source, note) in zzop_rules_cross_layer::trpc_mount_route_suppression_notes(
            &cross_layer.unconsumed_provides,
            &trpc_edge_counts_by_source,
        ) {
            if let Some((_, _, output)) = outputs.iter_mut().find(|(_, s, _)| *s == source) {
                output.warnings.push(note);
            }
        }
    }

    MultiAnalyzeOutput {
        trees: outputs,
        cross_layer,
        cross_layer_findings,
    }
}

/// Runs the 22 `cross-layer/*` native rules (`zzop_rules_cross_layer::cross_layer`) over `cross_layer` and
/// returns their merged, sorted findings.
///
/// ## disabledRules gating: union, exclude-only
/// A cross-layer rule is disabled if its id appears in ANY tree's `EngineConfig::rule_config.disabled_rules`
/// — the union, not the intersection: this is a joint-analysis output no single tree fully owns, so any
/// one tree opting out is treated as the whole cross-layer run opting that rule out.
///
/// ## The provide-key universe
/// `method_mismatch`/`version_skew`/`path_near_miss` need every `http` provide across every tree, not
/// just what `CrossLayerResult` exposes — derived here (`http_provides`) from the same `source_ios` the
/// join itself was built from.
///
/// ## Sort
/// `zzop_core::merge_findings`, the same (severity, file, line, ruleId) order `AnalyzeOutput::findings`
/// uses, called with a default `RuleConfig` purely for that shared sort/merge primitive (disabling is
/// the only lever for cross-layer findings, handled above via `is_enabled`).
fn compute_cross_layer_findings(
    source_ios: &[SourceIo],
    cross_layer: &zzop_core::CrossLayerResult,
    trees: &[(PathBuf, EngineConfig)],
    package_imports: &[zzop_rules_cross_layer::PackageImportSite],
    trpc_participating_sources: &BTreeSet<String>,
) -> Vec<Finding> {
    let mut disabled_union: Vec<String> = Vec::new();
    for (_, config) in trees {
        disabled_union.extend(config.rule_config.disabled_rules.iter().cloned());
    }
    let gate = RuleConfig {
        disabled_rules: disabled_union,
        ..RuleConfig::default()
    };

    let http_provides: Vec<zzop_rules_cross_layer::HttpProvideSite> = source_ios
        .iter()
        .flat_map(|s| {
            s.io.provides
                .iter()
                .filter(|p| p.kind == "http")
                .map(move |p| zzop_rules_cross_layer::HttpProvideSite {
                    source: s.source.clone(),
                    key: p.key.clone(),
                    file: p.file.clone(),
                    line: p.line,
                })
        })
        .collect();

    let http_consume_totals: Vec<(String, usize)> = source_ios
        .iter()
        .filter_map(|s| {
            let n = s.io.consumes.iter().filter(|c| c.kind == "http").count();
            (n > 0).then(|| (s.source.clone(), n))
        })
        .collect();

    let mut sources: Vec<Vec<Finding>> = Vec::with_capacity(22);

    // `route_near_miss_results` is called ONCE here (ahead of its own position in `sources` order below) so
    // both `unconsumed-endpoint` and `unconsumed-mutation-endpoint` can annotate a provide that is also a
    // near-miss target — see `zzop_rules_cross_layer::route_near_miss`'s module doc. Disabled ->
    // `near_miss_targets` stays empty (there is no near-miss finding to point at, so no annotation), and the
    // findings themselves are still only pushed into `sources` at their original position, under the same
    // `is_enabled` gate as before.
    let route_near_miss_result = if zzop_core::is_enabled(&gate, "cross-layer/route-near-miss") {
        Some(
            zzop_rules_cross_layer::cross_layer::route_near_miss::route_near_miss_results(
                &cross_layer.unprovided_consumes,
                &http_provides,
            ),
        )
    } else {
        None
    };
    let near_miss_targets = route_near_miss_result
        .as_ref()
        .map(|r| r.targets.clone())
        .unwrap_or_default();

    if zzop_core::is_enabled(&gate, "cross-layer/unconsumed-endpoint") {
        sources.push(zzop_rules_cross_layer::unconsumed_endpoint_findings(
            &cross_layer.unconsumed_provides,
            &cross_layer.unresolved_consumes,
            &near_miss_targets,
            trpc_participating_sources,
        ));
    }
    if zzop_core::is_enabled(&gate, "cross-layer/method-mismatch") {
        sources.push(zzop_rules_cross_layer::method_mismatch_findings(
            &cross_layer.unprovided_consumes,
            &http_provides,
        ));
    }
    if zzop_core::is_enabled(&gate, "cross-layer/version-skew") {
        sources.push(zzop_rules_cross_layer::version_skew_findings(
            &cross_layer.unprovided_consumes,
            &http_provides,
        ));
    }
    if zzop_core::is_enabled(&gate, "cross-layer/path-near-miss") {
        sources.push(zzop_rules_cross_layer::path_near_miss_findings(
            &cross_layer.unprovided_consumes,
            &http_provides,
        ));
    }
    if let Some(result) = route_near_miss_result {
        // `cross-layer/prefix-drift` aggregates route-near-miss's `prefix_records`: when 3+ consumes share
        // one missing/extra base prefix (`/api`, ...) against the same target tree, one aggregate finding
        // replaces those per-route near-misses — subsumed via `retain_non_subsumed`. This is a replacement,
        // not silent suppression: the aggregate enumerates every folded route (`output-philosophy.md` §0/§1).
        // Structurally derived from route-near-miss's records, so it can only run inside this branch (route-
        // near-miss enabled); disabling prefix-drift alone leaves the per-route near-misses intact.
        if zzop_core::is_enabled(&gate, "cross-layer/prefix-drift") {
            let prefix_drift =
                zzop_rules_cross_layer::prefix_drift_findings(&result.prefix_records);
            sources.push(zzop_rules_cross_layer::retain_non_subsumed(
                result.findings,
                &prefix_drift.subsumed,
            ));
            if !prefix_drift.findings.is_empty() {
                sources.push(prefix_drift.findings);
            }
        } else {
            sources.push(result.findings);
        }
    }
    if zzop_core::is_enabled(&gate, "cross-layer/shared-db-table") {
        sources.push(zzop_rules_cross_layer::shared_db_table_findings(
            cross_layer,
        ));
    }
    if zzop_core::is_enabled(&gate, "cross-layer/duplicate-route") {
        sources.push(zzop_rules_cross_layer::cross_layer_duplicate_route_findings(cross_layer));
    }
    if zzop_core::is_enabled(&gate, "cross-layer/external-shadow-internal") {
        sources.push(zzop_rules_cross_layer::external_shadow_internal_findings(
            &cross_layer.external_consumes,
            &http_provides,
        ));
    }
    if zzop_core::is_enabled(&gate, "cross-layer/external-secret-in-url") {
        sources.push(zzop_rules_cross_layer::external_secret_in_url_findings(
            &cross_layer.external_consumes,
        ));
    }
    if zzop_core::is_enabled(&gate, "cross-layer/external-duplicated-integration") {
        sources.push(
            zzop_rules_cross_layer::external_duplicated_integration_findings(
                &cross_layer.external_consumes,
            ),
        );
    }
    if zzop_core::is_enabled(&gate, "cross-layer/external-host-fanout") {
        sources.push(zzop_rules_cross_layer::external_host_fanout_findings(
            &cross_layer.external_consumes,
        ));
    }
    if zzop_core::is_enabled(&gate, "cross-layer/external-base-url-drift") {
        sources.push(zzop_rules_cross_layer::external_base_url_drift_findings(
            &cross_layer.external_consumes,
        ));
    }
    if zzop_core::is_enabled(&gate, "cross-layer/external-version-inconsistent") {
        sources.push(
            zzop_rules_cross_layer::external_version_inconsistent_findings(
                &cross_layer.external_consumes,
            ),
        );
    }
    if zzop_core::is_enabled(&gate, "cross-layer/external-ip-literal") {
        sources.push(zzop_rules_cross_layer::external_ip_literal_findings(
            &cross_layer.external_consumes,
        ));
    }
    if zzop_core::is_enabled(&gate, "cross-layer/ambiguous-consume") {
        sources.push(zzop_rules_cross_layer::ambiguous_consume_findings(
            &cross_layer.ambiguous_consumes,
        ));
    }
    if zzop_core::is_enabled(&gate, "cross-layer/unconsumed-mutation-endpoint") {
        sources.push(
            zzop_rules_cross_layer::unconsumed_mutation_endpoint_findings(
                &cross_layer.unconsumed_provides,
                &near_miss_targets,
                trpc_participating_sources,
            ),
        );
    }
    if zzop_core::is_enabled(&gate, "cross-layer/unprovided-mutation-call") {
        sources.push(zzop_rules_cross_layer::unprovided_mutation_call_findings(
            &cross_layer.unprovided_consumes,
        ));
    }
    if zzop_core::is_enabled(&gate, "cross-layer/route-shadowing") {
        sources.push(zzop_rules_cross_layer::cross_tree_route_shadowing_findings(
            &http_provides,
        ));
    }
    if zzop_core::is_enabled(&gate, "cross-layer/unresolved-consume-ratio") {
        sources.push(zzop_rules_cross_layer::unresolved_consume_ratio_findings(
            &cross_layer.unresolved_consumes,
            &http_consume_totals,
        ));
    }
    if zzop_core::is_enabled(&gate, "cross-layer/sdk-import-no-visible-consume") {
        sources.push(
            zzop_rules_cross_layer::sdk_import_no_visible_consume_findings(
                package_imports,
                &http_consume_totals,
            ),
        );
    }
    if zzop_core::is_enabled(&gate, "cross-layer/unconsumed-procedure") {
        sources.push(zzop_rules_cross_layer::unconsumed_procedure_findings(
            &cross_layer.unconsumed_provides,
        ));
    }

    zzop_core::merge_findings(sources, &RuleConfig::default())
}

#[cfg(test)]
mod tests {
    //! End-to-end fixture-tree tests — a hand-rolled `TempDir` (same pattern as
    //! `packages/core/src/pack_loader.rs` / `parser/parser-prisma/src/lib.rs`'s test modules; no `tempfile`
    //! dependency in this workspace).
    use super::*;
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

    /// Loads the real `rules/dsl/java-security/java-security.json` from the repo, resolved from
    /// `CARGO_MANIFEST_DIR` (`packages/engine` -> up two -> repo root -> `rules/dsl/...`) so the test
    /// exercises the shipped pack.
    fn java_security_pack() -> RulePackDef {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../rules/dsl/java-security/java-security.json");
        let text = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
        serde_json::from_str(&text).expect("parse java-security.json")
    }

    /// Builds the shared fixture tree:
    /// - `a.ts` <-> `b.ts`: a circular import pair.
    /// - `c.ts`: imports a module that does not exist (dangling import — must not panic, must not resolve
    ///   to an edge).
    /// - `db/schema.prisma`: a `User` model.
    /// - `legacy/C.java`: a SQL-taint pattern the `java-security` DSL pack's line-scan rule matches.
    /// - `generated/big.ts`: exceeds `size_cap` -> oversized lexical fallback.
    /// - `broken.ts`: unbalanced braces -> structurally-broken lexical fallback.
    fn fixture_tree() -> TempDir {
        let dir = TempDir::new("zzop-engine-fixture");
        dir.write(
            "a.ts",
            "import { b } from './b';\nexport function a() { return b(); }\n",
        );
        dir.write(
            "b.ts",
            "import { a } from './a';\nexport function b() { return a(); }\n",
        );
        dir.write(
            "c.ts",
            "import { missing } from './does-not-exist';\nexport const c = missing;\n",
        );
        dir.write(
            "db/schema.prisma",
            "model User {\n  id String @id\n  email String @unique\n}\n",
        );
        dir.write(
            "legacy/C.java",
            "public class C {\n  void run(String login) {\n    Query q = em.createQuery(\"SELECT u FROM User u WHERE u.login = '\" + login + \"'\");\n  }\n}\n",
        );
        dir.write(
            "generated/big.ts",
            &"const filler = 'generated content line';\n".repeat(40),
        );
        dir.write("broken.ts", "function broken( {\n  return 1;\n");
        dir
    }

    fn config(size_cap: usize) -> EngineConfig {
        EngineConfig {
            source_id: "fixture".to_string(),
            size_cap,
            packs: vec![java_security_pack()],
            ..EngineConfig::default()
        }
    }

    #[test]
    fn circular_ts_import_pair_produces_a_circular_finding() {
        let dir = fixture_tree();
        let out = analyze_tree(dir.path(), &config(DEFAULT_SIZE_CAP));
        let cycle = out.findings.iter().find(|f| f.rule_id == "circular");
        assert!(
            cycle.is_some(),
            "expected a circular finding, got: {:?}",
            out.findings
        );
        let cycle = cycle.unwrap();
        assert!(cycle.file == "a.ts" || cycle.file == "b.ts");
    }

    #[test]
    fn java_security_line_scan_pack_fires_on_the_java_file() {
        let dir = fixture_tree();
        let out = analyze_tree(dir.path(), &config(DEFAULT_SIZE_CAP));
        let hit = out
            .findings
            .iter()
            .find(|f| f.rule_id == "java-security/sql-taint");
        assert!(
            hit.is_some(),
            "expected a java-security/sql-taint finding, got: {:?}",
            out.findings
        );
        assert_eq!(hit.unwrap().file, "legacy/C.java");
    }

    #[test]
    fn oversized_file_degrades_but_loc_is_still_counted() {
        let dir = fixture_tree();
        // Small cap so `generated/big.ts` (~1.5KB) is oversized, but every other fixture file is not.
        let out = analyze_tree(dir.path(), &config(500));
        assert!(out.degraded.contains(&"generated/big.ts".to_string()));
        let loc = out.ir.ir.loc.get("generated/big.ts").copied().unwrap_or(0);
        assert!(
            loc > 0,
            "oversized file's loc should still be lexically counted"
        );
        // A file under the cap must NOT be marked degraded.
        assert!(!out.degraded.contains(&"a.ts".to_string()));
    }

    #[test]
    fn syntactically_broken_ts_file_degrades_without_panicking() {
        let dir = fixture_tree();
        let out = analyze_tree(dir.path(), &config(DEFAULT_SIZE_CAP));
        assert!(out.degraded.contains(&"broken.ts".to_string()));
        let loc = out.ir.ir.loc.get("broken.ts").copied().unwrap_or(0);
        assert!(loc > 0);
    }

    #[test]
    fn dangling_import_resolves_to_no_edge_without_panicking() {
        let dir = fixture_tree();
        let out = analyze_tree(dir.path(), &config(DEFAULT_SIZE_CAP));
        let edges = out.ir.ir.dep.get("c.ts").cloned().unwrap_or_default();
        assert!(edges.is_empty());
    }

    #[test]
    fn prisma_model_symbols_are_present_in_the_ir() {
        let dir = fixture_tree();
        let out = analyze_tree(dir.path(), &config(DEFAULT_SIZE_CAP));
        let user = out
            .ir
            .ir
            .symbols
            .iter()
            .find(|s| s.name == "User" && s.file == "db/schema.prisma");
        assert!(
            user.is_some(),
            "expected a User model symbol, got: {:?}",
            out.ir.ir.symbols
        );
        assert!(user.unwrap().exported);
    }

    #[test]
    fn file_count_covers_every_fixture_file() {
        let dir = fixture_tree();
        let out = analyze_tree(dir.path(), &config(DEFAULT_SIZE_CAP));
        assert_eq!(out.file_count, 7); // a.ts, b.ts, c.ts, schema.prisma, C.java, big.ts, broken.ts
    }

    #[test]
    fn skip_dirs_are_never_walked() {
        let dir = fixture_tree();
        dir.write("node_modules/vendor/index.ts", "export const x = 1;\n");
        let out = analyze_tree(dir.path(), &config(DEFAULT_SIZE_CAP));
        assert_eq!(out.file_count, 7); // vendor file under node_modules/ must not be counted
        assert!(!out.ir.ir.loc.contains_key("node_modules/vendor/index.ts"));
    }

    #[test]
    fn yarn_dir_is_never_walked() {
        // `.yarn` (vendored Yarn Berry bundles) must be skipped the same way `node_modules` is.
        let dir = fixture_tree();
        dir.write(
            ".yarn/releases/yarn-4.0.0.cjs",
            "process.env.SOME_TOKEN; const x = 1;\n",
        );
        let out = analyze_tree(dir.path(), &config(DEFAULT_SIZE_CAP));
        assert_eq!(out.file_count, 7); // vendored file under .yarn/ must not be counted
        assert!(!out.ir.ir.loc.contains_key(".yarn/releases/yarn-4.0.0.cjs"));
    }

    #[test]
    fn disabling_a_pack_removes_its_findings() {
        let dir = fixture_tree();
        let mut cfg = config(DEFAULT_SIZE_CAP);
        cfg.rule_config
            .disabled_rules
            .push("java-security".to_string());
        let out = analyze_tree(dir.path(), &cfg);
        assert!(!out
            .findings
            .iter()
            .any(|f| f.rule_id.starts_with("java-security/")));
    }

    #[test]
    fn disabling_circular_removes_the_circular_finding() {
        let dir = fixture_tree();
        let mut cfg = config(DEFAULT_SIZE_CAP);
        cfg.rule_config.disabled_rules.push("circular".to_string());
        let out = analyze_tree(dir.path(), &cfg);
        assert!(!out.findings.iter().any(|f| f.rule_id == "circular"));
    }

    #[test]
    fn two_runs_over_the_same_tree_are_byte_for_byte_identical() {
        let dir = fixture_tree();
        let cfg = config(500); // exercise the oversized path too
        let out1 = analyze_tree(dir.path(), &cfg);
        let out2 = analyze_tree(dir.path(), &cfg);
        assert_eq!(
            serde_json::to_value(&out1.ir).unwrap(),
            serde_json::to_value(&out2.ir).unwrap()
        );
        assert_eq!(
            serde_json::to_value(&out1.findings).unwrap(),
            serde_json::to_value(&out2.findings).unwrap()
        );
        assert_eq!(out1.degraded, out2.degraded);
        assert_eq!(out1.file_count, out2.file_count);
    }

    // --- late consume resolution: cross-file constant indirection (crate::io's module doc / analyze::
    // late_resolve_cross_file_consumes) ---

    #[test]
    fn cross_file_constant_indirection_resolves_via_late_consume_resolution() {
        let dir = TempDir::new("zzop-engine-late-resolve");
        dir.write(
            "ControlKey.ts",
            "export const ControlKey = { AUTHEN: { getUserInfo: '/api/auth/user' } };\n",
        );
        dir.write(
            "Ctx.tsx",
            "import { ControlKey } from './ControlKey';\naxios.get(ControlKey.AUTHEN.getUserInfo);\n",
        );
        let out = analyze_tree(
            dir.path(),
            &EngineConfig {
                source_id: "fixture".to_string(),
                ..EngineConfig::default()
            },
        );
        let io = out.ir.ir.io.expect("expected io facts");
        let consume = io
            .consumes
            .iter()
            .find(|c| c.file == "Ctx.tsx")
            .expect("expected a consume from Ctx.tsx");
        assert_eq!(
            consume.key.as_deref(),
            Some("GET /api/auth/user"),
            "cross-file constant indirection should now resolve at assembly time: {consume:?}"
        );
        // Provenance is kept, not cleared, on a late-resolved consume.
        assert_eq!(
            consume.raw.as_deref(),
            Some("ControlKey.AUTHEN.getUserInfo")
        );
    }

    #[test]
    fn duplicate_const_key_across_two_files_resolves_to_the_lexicographically_first_file() {
        let dir = TempDir::new("zzop-engine-late-resolve-dup");
        // Both files declare the SAME dotted constant key with different values — "a-consts.ts" sorts
        // before "z-consts.ts", so its value must win regardless of file-walk/rayon scheduling order.
        dir.write("a-consts.ts", "export const K = { path: '/from/a' };\n");
        dir.write("z-consts.ts", "export const K = { path: '/from/z' };\n");
        dir.write("Ctx.tsx", "axios.get(K.path);\n");
        let out = analyze_tree(
            dir.path(),
            &EngineConfig {
                source_id: "fixture".to_string(),
                ..EngineConfig::default()
            },
        );
        let io = out.ir.ir.io.expect("expected io facts");
        let consume = io
            .consumes
            .iter()
            .find(|c| c.file == "Ctx.tsx")
            .expect("expected a consume from Ctx.tsx");
        assert_eq!(consume.key.as_deref(), Some("GET /from/a"));
    }

    // --- tRPC: assembly-time PROVIDE composition (analyze::compose_trpc_provides) joined to a client CONSUME
    // (crate::io's TS branch / trpc_consume) ---

    #[test]
    fn trpc_router_composes_across_files_and_joins_to_a_client_consume() {
        let dir = TempDir::new("zzop-engine-trpc");
        // `viewer.ts`: the leaf procedure's own router fragment.
        dir.write(
            "viewer.ts",
            "export const viewerRouter = router({ me: publicProcedure.query(() => 1) });\n",
        );
        // `trpc.ts`: mounts `viewerRouter` (imported from another file) under the `viewer` key — the
        // cross-file `Ref` `compose_trpc_provides` must resolve via the same import-resolution machinery
        // the TS dep graph itself uses.
        dir.write(
            "trpc.ts",
            "import { viewerRouter } from './viewer';\nexport const appRouter = router({ viewer: viewerRouter });\n",
        );
        // `page.tsx`: a client bound from a `"trpc"`-named specifier (the import-specifier client-detection
        // route `trpc_consume` documents), calling the composed procedure.
        dir.write(
            "page.tsx",
            "import { client } from './trpc-client';\nclient.viewer.me.useQuery();\n",
        );
        let out = analyze_tree(
            dir.path(),
            &EngineConfig {
                source_id: "fixture".to_string(),
                ..EngineConfig::default()
            },
        );
        let io = out.ir.ir.io.expect("expected io facts");
        let provide = io
            .provides
            .iter()
            .find(|p| p.kind == "trpc" && p.key == "QUERY viewer.me")
            .unwrap_or_else(|| panic!("expected a trpc provide, got: {:?}", io.provides));
        assert_eq!(
            provide.file, "viewer.ts",
            "the composed provide must anchor on the leaf's own originating file, not the `Ref`'s"
        );
        let consume = io
            .consumes
            .iter()
            .find(|c| c.kind == "trpc" && c.key.as_deref() == Some("QUERY viewer.me"))
            .unwrap_or_else(|| panic!("expected a trpc consume, got: {:?}", io.consumes));
        assert_eq!(consume.file, "page.tsx");
    }
}
