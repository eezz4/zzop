//! `analyze_tree`'s result types — `AnalyzeOutput` and its `CacheStats` payload.

use zzop_core::{dsl::RuleTiming, CommonIr, FileNode, Finding};
use zzop_metrics::{
    CriticalFile, CrossLayerCoChurn, FolderAggregates, HealthIndex, Recommendation, Scores,
    SeamCandidate,
};

use crate::{CoverageCensus, EngineConfig, PackSource, PackageImportSummary};

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
    /// The positive pack-load confirmation: one entry per DSL rule pack in `EngineConfig::packs`,
    /// sorted by pack id — so an embedder/agent can verify "did my custom pack actually load" without
    /// inferring it from findings deltas. Always populated; an EMPTY vec is the honest "zero DSL packs
    /// loaded" signal (the positive complement of `zero_packs_warning`'s `warnings` entry — both gate on
    /// the same `config.packs`). Reflects LOADED packs, before `disabled_rules` gating: disabling a pack
    /// is the caller's own explicit config, not a load failure, so it must not look like one.
    pub packs_loaded: Vec<PackLoaded>,
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

/// One `AnalyzeOutput::packs_loaded` entry — a loaded DSL rule pack's id, its rule count as loaded
/// (before `disabled_rules` gating), and its provenance (`PackSource::as_str`: `"dir"` | `"inline"`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackLoaded {
    pub id: String,
    pub rules: usize,
    pub source: String,
}

impl PackLoaded {
    /// Builds `AnalyzeOutput::packs_loaded` from `config.packs` + `config.pack_sources`, sorted by pack
    /// id (deterministic regardless of load order). A pack id with no `pack_sources` entry reports
    /// `"inline"` — see `EngineConfig::pack_sources`. Shared by `analyze::assemble` and
    /// `envelope::analyze_envelope`, so both entry points confirm the identical pack set.
    pub(crate) fn from_config(config: &EngineConfig) -> Vec<PackLoaded> {
        let mut loaded: Vec<PackLoaded> = config
            .packs
            .iter()
            .map(|pack| PackLoaded {
                id: pack.id.clone(),
                rules: pack.rules.len(),
                source: config
                    .pack_sources
                    .get(&pack.id)
                    .copied()
                    .unwrap_or(PackSource::Inline)
                    .as_str()
                    .to_string(),
            })
            .collect();
        loaded.sort_by(|a, b| a.id.cmp(&b.id));
        loaded
    }
}
