//! Output assembly: JSON-serializable views over engine outputs (single-tree, multi-tree, disclosure).

use serde::Serialize;

use zzop_core::{CommonIr, FileNode, Finding};
use zzop_engine::{AnalyzeOutput, CacheStats};
use zzop_metrics::{
    CriticalFile, CrossLayerCoChurn, FolderAggregates, HealthIndex, Recommendation, Scores,
    SeamCandidate,
};

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

/// JSON view over one `zzop_engine::PackLoaded` — the positive pack-load confirmation entry (pack id,
/// rule count as loaded, provenance `"dir"` | `"inline"`). Borrowed strings, same zero-copy-view
/// convention as every other field; camelCase like every other output-facing type at this boundary
/// (a no-op today — all three field names are one word).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PackLoadedView<'a> {
    id: &'a str,
    rules: usize,
    source: &'a str,
}

impl<'a> From<&'a zzop_engine::PackLoaded> for PackLoadedView<'a> {
    fn from(p: &'a zzop_engine::PackLoaded) -> Self {
        PackLoadedView {
            id: &p.id,
            rules: p.rules,
            source: &p.source,
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
pub(crate) struct BlindnessClassView {
    id: &'static str,
    group: &'static str,
    summary: &'static str,
    status: &'static str,
}

/// The full registry as a serializable list — attached at the top level of every entry point's output
/// (a run-global honesty channel, never per-tree, so it is emitted once regardless of tree count).
pub(crate) fn disclosure_views() -> Vec<BlindnessClassView> {
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
pub(crate) struct SingleTreeOutputView<'a> {
    #[serde(flatten)]
    output: AnalyzeOutputView<'a>,
    disclosure: Vec<BlindnessClassView>,
}

impl<'a> SingleTreeOutputView<'a> {
    pub(crate) fn of(output: &'a AnalyzeOutput) -> Self {
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
pub(crate) struct AnalyzeOutputView<'a> {
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
    /// Positive pack-load confirmation, sorted by pack id — ALWAYS serialized (no skip-if-empty): an
    /// empty array is the honest "zero DSL packs loaded" signal, not a field to hide.
    packs_loaded: Vec<PackLoadedView<'a>>,
    warnings: &'a [String],
    cache: Option<CacheStatsView>,
    rule_timings: &'a Option<Vec<zzop_core::dsl::RuleTiming>>,
    /// Structural coverage census — always present (post-aggregate, never git-gated).
    coverage: CoverageCensusView,
}

impl<'a> AnalyzeOutputView<'a> {
    pub(crate) fn of(output: &'a AnalyzeOutput) -> Self {
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
            packs_loaded: output
                .packs_loaded
                .iter()
                .map(PackLoadedView::from)
                .collect(),
            warnings: &output.warnings,
            cache: output.cache.map(CacheStatsView::from),
            rule_timings: &output.rule_timings,
            coverage: CoverageCensusView::from(&output.coverage),
        }
    }
}

/// One `analyzeTrees` output entry: a tree's `root`/`sourceId` echo plus its `AnalyzeOutputView`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TreeEntryView<'a> {
    pub(crate) root: String,
    pub(crate) source_id: &'a str,
    pub(crate) output: AnalyzeOutputView<'a>,
}

/// `analyzeTrees`'s output root: every tree's entry plus the cross-layer join, its findings, and the
/// run-global disclosure registry.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MultiAnalyzeOutputView<'a> {
    pub(crate) trees: Vec<TreeEntryView<'a>>,
    pub(crate) cross_layer: &'a zzop_core::CrossLayerResult,
    /// The 23 `cross-layer/*` native rules run over `cross_layer` (`zzop_engine::analyze_trees`'s own
    /// `MultiAnalyzeOutput::cross_layer_findings` field — a plain `&'a [Finding]` borrow, same
    /// zero-copy-view convention as every other field on this struct, since `Finding` already derives
    /// `Serialize` in `zzop-core`).
    pub(crate) cross_layer_findings: &'a [Finding],
    /// Run-global silent-failure-class registry — emitted once (not per tree), same content as the
    /// single-tree output's `disclosure`.
    pub(crate) disclosure: Vec<BlindnessClassView>,
}
