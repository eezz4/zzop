//! `analyze_tree`'s result types — `AnalyzeOutput` and its `CacheStats` payload. The pack-load
//! confirmation entry lives in the [`packs_loaded`] child module (see its own doc for the seam).

use zzop_core::{dsl::RuleTiming, CommonIr, FileNode, Finding};
use zzop_metrics::{
    CoChangeEdge, CriticalFile, CrossLayerCoChurn, FolderAggregates, HealthIndex, Recommendation,
    Scores, SeamCandidate,
};

use crate::{CoverageCensus, PackageImportSummary};

mod native_analyses;
mod packs_loaded;

pub use native_analyses::NativeAnalyses;
pub use packs_loaded::{PackLoaded, PackNotRun};

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
    /// Files this tree's OWN `package.json` manifests name from a `scripts` command — the tree declaring,
    /// in its own manifest, "this file is how I am BUILT, not what I ship" (`pipeline::package_json_entries`'s
    /// `PackageJsonScan::script_paths`; disjoint from the shipped `entry_paths` by construction). Sorted,
    /// deduped, and resolved against the walked file set, so every entry names a file this run analyzed.
    ///
    /// It is a plain fact, not a judgment: the consumer that acts on it is the summary layer's
    /// first-screen ordering, which demotes findings on these files BELOW production findings of the same
    /// severity (the build-surface tier beside the older test-path one). Nothing is dropped and no count
    /// moves — see `zzop_summary`'s `shape_findings`.
    ///
    /// ALWAYS populated (never `Option`): the manifest walk is unconditional, so an EMPTY vec is the
    /// honest "no `package.json` in this tree declared a resolvable script path" — the state every tree
    /// outside the npm ecosystem is in by construction. Mode A (envelope) has no filesystem to walk and
    /// is empty for a different reason, stated at its own construction site.
    pub build_script_paths: Vec<String>,
    pub file_count: usize,
    /// Structural coverage census — see `CoverageCensus`. Always present (post-aggregate, never
    /// git-gated).
    pub coverage: CoverageCensus,
    /// Per non-relative import specifier: how many files import it + the first importing file. Plumbing
    /// for `cross-layer/untraced-client-import-no-visible-consume` (the tree IR drops package imports during dep
    /// resolution) — not part of the serialized output surface.
    pub package_imports: Vec<PackageImportSummary>,
    /// Route registrations this tree's source shows LEXICALLY, when that was measured — plumbing for the
    /// run-wide provide-blind severity gate (`framework_silence::provide_blind_sources`), not part of the
    /// serialized output surface.
    ///
    /// `None` means NOT MEASURED, and the distinction is load-bearing: the scan runs only under the S2
    /// tripwire's precondition (a server-framework import with fewer than `MIN_PROVIDES_FLOOR` extracted
    /// routes), so an ordinary tree pays nothing and arrives here unmeasured. A reader that collapses
    /// `None` into `Some(0)` turns "we did not look" into "there is nothing there", which is the exact
    /// confusion that gate exists to avoid.
    pub visible_route_registrations: Option<usize>,
    /// This tree's assembled entity-attribute store (native producer judgments + Mode B overlay
    /// injections — see `zzop_core::AttributeStore`). Plumbing for the cross-layer stage
    /// (`cross-layer/retrying-write-no-idempotency`'s provider-side `idempotency-guarded` veto reads
    /// the PROVIDER tree's store by source id) — like `package_imports`, not part of the serialized
    /// output surface.
    pub attributes: zzop_core::AttributeStore,
    pub nodes: Vec<FileNode>,
    pub scores: Option<Scores>,
    pub health: Option<HealthIndex>,
    pub recommendations: Vec<Recommendation>,
    pub critical: Vec<CriticalFile>,
    /// How many rows `zzop_metrics::CRITICALITY_LIMIT` dropped from `critical`. `0` = complete list.
    ///
    /// ALWAYS SERIALIZED, `0` included — the convention `zzop_metrics::scores::detail_cap` set for this
    /// repo's capped lists, and for its reason: a field that vanishes when nothing was dropped makes
    /// "complete list" and "this build has no disclosure" the same bytes.
    ///
    /// Added 2026-09-04. `architecture.criticalTop`'s legend publishes the THREE paths it lifts, so a
    /// reader who drilled into `critical` itself met a 20-row wall that no sentence in the reply named —
    /// and 20 rows read as the whole set. Zero when the analysis did not run (git inactive, or the id
    /// disabled): nothing was capped, which the empty list already says.
    pub critical_truncated: u32,
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
    /// Undirected file-pair co-change edges — the second relation over the same nodes the dep graph
    /// draws, and until 2026-08-06 the only one of the two that never left this crate: the coupling map
    /// was consumed by `recommendations`/`seams` and dropped, so no surface could answer "what changes
    /// together". `layer_co_churn` above is NOT that answer — it keeps only pairs that CROSS an
    /// architectural layer, and only representative ones per layer pair, so every tie inside a module is
    /// absent from it by construction.
    ///
    /// Git-gated the same way, and `Option` carries the distinction that matters: `None` means git was
    /// inactive or collection failed, so nothing was MEASURED; `Some(vec![])` means it was measured and
    /// nothing co-changed. A consumer that flattens those two into "no coupling" states a fact the run
    /// never established. Ungated by `disabledRules` for the reason `layer_co_churn` is — it is not a
    /// registered analysis id, so claiming that switch controls it would be a lie.
    ///
    /// ⚠ A doubly filtered sample, never a repository total — see `zzop_metrics::co_change_edges`.
    pub co_change: Option<Vec<CoChangeEdge>>,
    /// The positive pack-load confirmation: one entry per DSL rule pack in `EngineConfig::packs`,
    /// sorted by pack id — so an embedder/agent can verify "did my custom pack actually load" without
    /// inferring it from findings deltas. Always populated; an EMPTY vec is the honest "zero DSL packs
    /// loaded" signal (the positive complement of `zero_packs_warning`'s `warnings` entry — both gate on
    /// the same `config.packs`). Reflects LOADED packs, before `disabled_rules` gating: disabling a pack
    /// is the caller's own explicit config, not a load failure, so it must not look like one.
    pub packs_loaded: Vec<PackLoaded>,
    /// The NATIVE-analysis counterpart of `packs_loaded`: how many native analyses this build
    /// registers, and which of them could not have keyed this output's `findings` — split by cause
    /// (config switched them off, versus they report into the cross-tree join's own
    /// `crossLayerFindings` channel instead). See [`NativeAnalyses`] for why the causes are kept apart
    /// and for the corpus measurement that forced the field.
    ///
    /// Always populated: unlike `rule_overrides_applied` this is not a report on what the caller
    /// requested but on what this build contains, so there is no "nothing was asked" state in which
    /// silence would be honest.
    pub native_analyses: NativeAnalyses,
    /// Non-fatal diagnostics — e.g. git collection failing, or the cache directory failing to open.
    /// Analysis still completes normally in either case.
    pub warnings: Vec<String>,
    /// Config-channel diagnostics computed at analysis time — currently just the
    /// `unknown_disabled_rule_ids`/`unknown_severity_override_ids` self-reports (a typo'd
    /// `disabled_rules`/`severity_overrides` entry matched no known rule id, so it did nothing). These
    /// are config problems, not degenerate-output signals, so they are kept OUT of `warnings` and land
    /// here instead — the same honesty channel a config front-end's own parse-time warnings (unknown
    /// config key, a malformed overlay) ride, so a consumer checking "did my config have a problem"
    /// only has to look in one place. Computed here rather than by the config mapper (`crates/config`)
    /// because only analysis time has the known-rule-id set (native analysis ids + loaded DSL pack
    /// ids) a config parser never sees. Empty when neither knob had a matching-nothing entry.
    pub config_warnings: Vec<String>,
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
    /// D13③: the positive counterpart of `unknown_disabled_rule_ids`/`unknown_severity_override_ids` (the
    /// coverage-gap diagnostics' "this config entry matched no known id, so it did nothing" self-report).
    /// Those two catch a TYPO; this confirms the opposite case — a CORRECT `disabled_rules`/
    /// `severity_overrides` entry that silently succeeded is otherwise unverifiable without a before/
    /// after findings diff. `None` when neither `RuleConfig::disabled_rules` nor `RuleConfig::
    /// severity_overrides` had any entries at all (nothing was requested) — the quieter of the two
    /// documented conventions for an additive field with nothing to say; see
    /// `analyze::diagnostics::coverage_report::rule_overrides_applied`'s doc for why an all-typo request
    /// still yields `Some` with empty lists rather than `None` (something WAS requested, it just matched
    /// nothing — that is still worth confirming, not hiding).
    pub rule_overrides_applied: Option<RuleOverridesApplied>,
    /// The operative git-window knobs (`EngineConfig::git`'s `recent_days`/`since`) for this run — a
    /// consumer diffing two runs' `scores`/`health`/`critical`/`seams` numbers has no other way to tell
    /// which window produced which output, since neither knob was echoed anywhere before this field
    /// existed (a blind field test's deep-history round hit exactly this: `recentDays`/`since` both
    /// change rankings, silently). `Some` exactly when git collection ran (mirrors `scores`/`health`'s
    /// own git-gating — `git_active` in `analyze::assemble::dep_graph::DepGraphResult`); `None` when
    /// `EngineConfig::git` was `None` OR collection failed (see `warnings` for the latter case), same as
    /// `scores`/`health` staying empty in both. `recent_days` is always the RESOLVED value (the default
    /// 30 when the caller never set one — `GitOptions` has no "unset" representation of its own by the
    /// time it reaches `EngineConfig`, so there is nothing to further resolve here).
    pub git_window: Option<GitWindow>,
}

/// `AnalyzeOutput::git_window`'s payload — see that field's doc for the `Some`/`None` gating.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitWindow {
    /// `GitOptions::recent_days`, resolved (never a caller-facing "unset" state at this layer).
    pub recent_days: u32,
    /// `GitOptions::since`, verbatim (`None` = full history).
    pub since: Option<String>,
}

/// `AnalyzeOutput::rule_overrides_applied`'s payload. Every list is sorted + deduped and bounded by the
/// size of the corresponding `RuleConfig` list the caller supplied (never larger than what was
/// requested): only entries that matched a KNOWN name appear.
///
/// "Known" is one definition per ID SPACE, never a second definition within one: `disabled`/
/// `severity_remapped` reuse the union `analyze::diagnostics::coverage_report::known_rule_ids` already
/// computes for the unknown-id diagnostics, while `only` reads the LOADED PACK IDS — a different space
/// on purpose, because `registry::is_pack_enabled` compares an allowlist entry against `pack.id` alone,
/// so a `"<pack>/<rule>"` or native id there gates nothing and must not be reported as applied.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuleOverridesApplied {
    /// `RuleConfig::disabled_rules` entries that matched a known native-analysis id, DSL `"<pack>/<rule>"`
    /// id, or bare DSL pack id (bare pack ids count here — `registry::is_enabled`/`gate_pack_rules` both
    /// honor one, dropping the whole pack, mirroring `unknown_disabled_rule_ids`'s own known-id union).
    pub disabled: Vec<String>,
    /// `RuleConfig::severity_overrides` keys that matched a known id — bare pack ids excluded, mirroring
    /// `unknown_severity_override_ids`'s narrower known-id union (`registry::apply_severity_override`
    /// matches a finding's `rule_id` exactly, and a bare pack id can never equal one).
    pub severity_remapped: Vec<String>,
    /// `RuleConfig::only_packs` entries that matched a loaded pack id — the pack ALLOWLIST that actually
    /// took effect this run (config dialect `packs.only`, embedder field `packsOnly`).
    ///
    /// This lives beside `disabled` because it is the same question asked from the other side, and a
    /// v0.29.0 release audit found the asymmetry the hard way: `packs.disabled` was positively confirmed
    /// here while `packs.only` — which suppresses *strictly more* — was confirmed nowhere in any reply
    /// field. A consumer could watch `sql/*` and `typescript/*` findings vanish with no wire evidence
    /// that a knob had been set, and `packsLoaded` cannot supply it (that field is a path-match census by
    /// its own doc, and it stays identical under BOTH knobs — it was never an enablement report).
    ///
    /// ⚠ That last clause stopped being true on 2026-08-26. [`crate::PackLoaded::did_not_run`] now
    /// marks each gated pack IN ITS OWN ROW (`"disabled"` / `"notAllowlisted"`), because the
    /// census-only reading was measured doing real damage in the other direction: a disabled
    /// `security` pack sitting in `packsLoaded` with 51 rules and a positive scanned-file count reads
    /// as "ran, found nothing". The two fields are still not redundant and neither replaces the other
    /// — this one answers *which entries of the caller's own request took effect* (a typo'd id never
    /// appears here), while `did_not_run` answers *what happened to each loaded pack* (every pack
    /// outside an allowlist is marked, which is exactly what the all-typo `only_packs` case needed and
    /// this field structurally cannot say).
    ///
    /// Only entries naming a pack in `config.packs` appear, mirroring `disabled`'s known-id filter: an
    /// allowlist entry that names no loaded pack contributed nothing to this run's gate. An all-typo
    /// `only_packs` is the more dangerous typo, because `is_pack_enabled` then admits NO pack and every
    /// DSL finding disappears at once.
    ///
    /// ⚠ This field is NOT the signal for that, and this doc claimed it was until 2026-08-11. The claim
    /// was *"`only` present but EMPTY means the caller set an allowlist and none of it named a loaded
    /// pack"* — false, because `only: []` is ALSO what a run produces when `only_packs` was never set
    /// and `disabled_rules` or `severity_overrides` was (either one is enough to make this whole struct
    /// `Some`). Measured on identical source, with `rules: {"dead-candidates": "off"}` also present:
    /// `packs.only: ["typo"]` and no allowlist at all produced envelopes differing in NO field but
    /// `findings` — `packsLoaded`, `filesInScope`, `warnings` and `configWarnings` were all identical.
    /// A wire field that only discriminates when no other knob is set is not a signal; it is a
    /// coincidence that holds in the bare case.
    ///
    /// The real signal is a `configWarning`, which fires regardless of what else is set:
    /// `DiagnosticsInput::unknown_only_pack_ids` (+ `only_packs_matched_nothing` for the wording),
    /// computed by `analyze::diagnostics::coverage_report`. This field keeps its original job —
    /// positively confirming the allowlist that DID take effect.
    pub only: Vec<String>,
}

/// `AnalyzeOutput::cache`'s payload — see that field's doc for what counts as a hit vs a miss.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CacheStats {
    pub hits: usize,
    pub misses: usize,
}
