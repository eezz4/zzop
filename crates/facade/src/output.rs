//! Output assembly: JSON-serializable views over engine outputs (single-tree, multi-tree, disclosure).
//!
//! The small, single-type mirror views (`CacheStatsView`, `PackLoadedView`, `RuleOverridesAppliedView`,
//! `GitWindowView`, `CoverageCensusView`, `BlindnessClassView`/`disclosure_views`) live in
//! [`mirrors`]. The composed views below
//! (which reach into more than one engine type, and carry this boundary's casing-contract doc) stay
//! here.

use serde::Serialize;

use zzop_core::{CommonIr, FileNode, Finding};
use zzop_engine::AnalyzeOutput;
use zzop_metrics::{
    CriticalFile, CrossLayerCoChurn, FolderAggregates, HealthIndex, Recommendation, Scores,
    SeamCandidate,
};

mod mirrors;

pub(crate) use mirrors::{disclosure_views, BlindnessClassView};
use mirrors::{
    CacheStatsView, CoverageCensusView, GitWindowView, PackLoadedView, RuleOverridesAppliedView,
};

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
    /// Config-channel diagnostics — currently the unknown-`disabledRules`/`severityOverrides`-id
    /// self-reports (`zzop_engine::AnalyzeOutput::config_warnings`'s own doc has the full rationale for
    /// why these ride a separate channel from `warnings`). ALWAYS serialized (no skip-if-empty), same
    /// convention as `warnings` — an empty array is the honest "no config problem" signal. A host that
    /// also runs `crates/config`'s mapper (parse-time config problems: unknown config keys, a malformed
    /// overlay) attaches THOSE warnings to the same `configWarnings` key on its own reply; this field is
    /// the analysis-time half of that one channel, never a rename of `warnings`.
    config_warnings: &'a [String],
    cache: Option<CacheStatsView>,
    rule_timings: &'a Option<Vec<zzop_core::dsl::RuleTiming>>,
    /// Structural coverage census — always present (post-aggregate, never git-gated).
    coverage: CoverageCensusView,
    /// D13③: `ruleOverridesApplied` — omitted entirely (never an empty `{}`) when the caller's
    /// `disabled_rules`/`severity_overrides` were both empty (nothing requested). This is the "quieter"
    /// of the two documented conventions (`zzop_engine::RuleOverridesApplied`'s own doc) — a caller who
    /// never touched either knob sees no new field, rather than an always-present empty object.
    #[serde(skip_serializing_if = "Option::is_none")]
    rule_overrides_applied: Option<RuleOverridesAppliedView<'a>>,
    /// `gitWindow` — the operative `recentDays`/`since` git-window knobs. ALWAYS serialized (no
    /// skip-if-none), same convention as `scores`/`health`/`cache`: `null` on the wire is itself the
    /// honest "git didn't run" signal, not a field to hide (contrast `ruleOverridesApplied`'s
    /// deliberately quieter "omit entirely" convention for a knob nobody touched — `gitWindow` is gated
    /// on whether an ANALYSIS PHASE ran, exactly like `scores`/`health`, not on whether a request field
    /// was set).
    git_window: Option<GitWindowView<'a>>,
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
            config_warnings: &output.config_warnings,
            cache: output.cache.map(CacheStatsView::from),
            rule_timings: &output.rule_timings,
            coverage: CoverageCensusView::from(&output.coverage),
            rule_overrides_applied: output
                .rule_overrides_applied
                .as_ref()
                .map(RuleOverridesAppliedView::from),
            git_window: output.git_window.as_ref().map(GitWindowView::from),
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
    /// Run-level self-reports that belong to the JOIN itself, not any one tree (currently only the
    /// parallel-implementation tripwire — `zzop_engine::MultiAnalyzeOutput::warnings`'s own doc).
    /// ALWAYS serialized (no skip-if-empty), same "empty is the honest signal" convention every other
    /// warnings channel at this boundary uses.
    pub(crate) warnings: &'a [String],
    /// Run-global silent-failure-class registry — emitted once (not per tree), same content as the
    /// single-tree output's `disclosure`.
    pub(crate) disclosure: Vec<BlindnessClassView>,
}
