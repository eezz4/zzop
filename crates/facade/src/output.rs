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
    CoChangeEdge, CriticalFile, HealthIndex, Recommendation, Scores, SeamCandidate,
};

mod mirrors;
mod native_analyses_legend;
mod packs_legend;

/// The two LANE-INVARIANT sentences of `nativeAnalysesMeaning`, re-exported so the cross-layer join
/// lane (`zzop_summary::cross`) can COMPOSE its own legend from them instead of restating what
/// `registered` counts and what `disabled` means. Two copies of one build-fact sentence is the drift
/// class this repo keeps paying for: the join reply and the per-tree reply must agree about their own
/// denominator, and one owner is the only thing that guarantees it. See [`native_analyses_legend`]
/// for which sentences deliberately do NOT travel, and why a key can keep its name across two lanes
/// while the action it licenses inverts.
pub use native_analyses_legend::{
    NATIVE_ANALYSES_DISABLED_MEANING, NATIVE_ANALYSES_REGISTERED_MEANING,
    NATIVE_ANALYSES_SHIPPED_OFF_MEANING,
};

/// The two legends' RUN-FREE views, for the one consumer that needs them with no analysis in hand:
/// `zzop_summary`'s reply-legends contract document, which serves the full text these two keys used to
/// ship on every call. Each function's own doc says why a document rendered from a run would be the
/// wrong document.
pub use native_analyses_legend::native_analyses_legend;
pub use packs_legend::packs_loaded_legend;

pub(crate) use mirrors::{disclosure_views, BlindnessClassView};
use mirrors::{
    CacheStatsView, CoverageCensusView, GitWindowView, NativeAnalysesView, PackLoadedView,
    RuleOverridesAppliedView,
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
/// `zzop_core::SourceSymbol` doubles as the deserialize target for `docs/NORMALIZED_AST.md`'s
/// external-parser envelope input contract (`FileProjection.symbols`): output is camelCase like
/// everything else, while per-field `#[serde(alias = ...)]` attributes keep accepting the frozen
/// contract's snake_case names on the way in (zzop only ever receives an envelope, never emits one).
///
/// `Finding.data` is the one exception, by design: it is opaque `serde_json::Value` authored ad hoc per
/// rule, never a `#[derive(Serialize)]` struct with a uniform convention to enforce — see
/// `docs/modules/facade.md`'s "Output data shapes" section for the per-rule shapes.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AnalyzeOutputView<'a> {
    ir: &'a CommonIr,
    findings: &'a [Finding],
    degraded: &'a [String],
    /// `buildScriptPaths` — the files this tree's own `package.json` `scripts` commands name, sorted
    /// (`zzop_engine::AnalyzeOutput::build_script_paths`). ALWAYS serialized (no skip-if-empty), the same
    /// "an empty array is the honest signal" convention `packsLoaded`/`warnings`/`configWarnings` use: an
    /// empty array means the manifest walk ran and no manifest declared a resolvable script path — the
    /// state every tree outside the npm ecosystem is in by construction — and hiding it would make "no
    /// build surface" indistinguishable from "an engine build that does not report one".
    ///
    /// Consumed by `zzop-summary`'s finding ordering (the build-surface tier), which is why it rides the
    /// wire at all rather than staying an engine internal — the shaping layer receives this JSON view and
    /// nothing else, so a fact it must judge on has to be ON the view.
    build_script_paths: &'a [String],
    file_count: usize,
    /// Per-file graph/git metrics. Its `fanIn`/`fanOut`/`totalConnections` are graph-theoretic terms and
    /// are correct ABOUT the graph they describe, so they are deliberately NOT renamed the way the
    /// census's `resolvedImportEdges` was (2026-07-31) — what needed stating is WHICH graph, and that
    /// sentence has exactly one owner: [`zzop_core::DEP_GRAPH_RESOLVED_ONLY`]. Read every degree here
    /// under it.
    nodes: &'a [FileNode],
    scores: &'a Option<Scores>,
    health: &'a Option<HealthIndex>,
    recommendations: &'a [Recommendation],
    /// Files ranked by `blastRadius` (transitive dependents). Same treatment as `nodes` above: the term
    /// is correct about the graph it is measured over, and that graph's membership rule is disclosed
    /// once, from [`zzop_core::DEP_GRAPH_RESOLVED_ONLY`] — a blast radius counts in-tree dependents.
    critical: &'a [CriticalFile],
    /// Rows the `critical` cap dropped — `0` on a complete list, and ALWAYS serialized so that an
    /// absent field can never stand in for a complete one. See
    /// [`zzop_engine::AnalyzeOutput::critical_truncated`]; the sibling `criticalTop` legend publishes
    /// only the three paths it lifts, so without this the 20-row wall behind it was unnamed.
    critical_truncated: u32,
    seams: &'a [SeamCandidate],
    /// Undirected file-pair co-change edges — the git-history relation over the same nodes `nodes`/the
    /// dep graph describe, and the substrate `graph --domain cochange` draws. `null` and `[]` say
    /// DIFFERENT things and must not be folded together: `null` = git inactive or collection failed, so
    /// nothing was measured; `[]` = measured, nothing co-changed. See
    /// `zzop_engine::AnalyzeOutput::co_change` for why it is not gated by `disabledRules`, and
    /// `zzop_metrics::co_change_edges` for the two filters that make it a sample rather than a total.
    co_change: &'a Option<Vec<CoChangeEdge>>,
    /// Positive pack-load confirmation, sorted by pack id — ALWAYS serialized (no skip-if-empty): an
    /// empty array is the honest "zero DSL packs loaded" signal, not a field to hide.
    packs_loaded: Vec<PackLoadedView<'a>>,
    /// The legend for the array above — what `filesInScope` counts, what a rule's presence in
    /// `zeroAdmissionRules` does and does not claim, and (only when a pack really was gated off) what
    /// `didNotRun` means. Omitted when no pack loaded. See [`packs_legend`] for why this is a sibling
    /// key rather than a `meaning` inside the object, and for the two measured misreadings it closes.
    #[serde(skip_serializing_if = "Option::is_none")]
    packs_loaded_meaning: Option<std::collections::BTreeMap<&'static str, &'static str>>,
    /// The NATIVE half of the same question `packsLoaded` answers for DSL packs: which of this build's
    /// native analyses could not have keyed `findings`, and why. ALWAYS serialized — it is a statement
    /// about the build, not about a request, so there is no state in which silence is honest. See
    /// `zzop_engine::NativeAnalyses` for the corpus measurement that forced it (975 `cross-layer/*`
    /// findings, reachable from the very same config, sitting behind nine blank replies).
    native_analyses: NativeAnalysesView<'a>,
    /// The legend for the object above — what `registered` counts, and what each of the two
    /// not-evaluated lists licenses the reader to do next. Same sibling-key shape and same reason as
    /// [`Self::packs_loaded_meaning`]; unconditional, because its subject is.
    native_analyses_meaning: std::collections::BTreeMap<&'static str, &'static str>,
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
    coverage: CoverageCensusView<'a>,
    /// D13③: `ruleOverridesApplied` — omitted entirely (never an empty `{}`) when the caller's
    /// `disabled_rules`/`severity_overrides`/`only_packs` were ALL empty (nothing requested). This is the
    /// "quieter" of the two documented conventions (`zzop_engine::RuleOverridesApplied`'s own doc) — a
    /// caller who never touched any of the three sees no new field, rather than an always-present empty
    /// object. (`only_packs` became the third gate in v0.29.0, when a release audit found the pack
    /// allowlist suppressing findings with no positive acknowledgement anywhere on the wire.)
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
            build_script_paths: &output.build_script_paths,
            file_count: output.file_count,
            nodes: &output.nodes,
            scores: &output.scores,
            health: &output.health,
            recommendations: &output.recommendations,
            critical: &output.critical,
            critical_truncated: output.critical_truncated,
            seams: &output.seams,
            co_change: &output.co_change,
            packs_loaded: output
                .packs_loaded
                .iter()
                .map(PackLoadedView::from)
                .collect(),
            packs_loaded_meaning: packs_legend::packs_loaded_meaning(&output.packs_loaded),
            native_analyses: NativeAnalysesView::from(&output.native_analyses),
            native_analyses_meaning: native_analyses_legend::native_analyses_meaning(),
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
    /// The `cross-layer/*` native rules run over `cross_layer` — how many there are is owned by
    /// `zzop_rules_cross_layer::register_native_analyses` (`zzop_engine::analyze_trees`'s own
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
