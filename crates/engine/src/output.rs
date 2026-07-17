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

/// `AnalyzeOutput::rule_overrides_applied`'s payload. Both lists are sorted + deduped and bounded by the
/// size of the corresponding `RuleConfig` list the caller supplied (never larger than what was
/// requested): only entries that matched a KNOWN id appear — the same known-id union `analyze::
/// diagnostics::coverage_report::known_rule_ids` already computes for the unknown-id diagnostics, never a
/// second definition of "known".
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
}

/// `AnalyzeOutput::cache`'s payload — see that field's doc for what counts as a hit vs a miss.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CacheStats {
    pub hits: usize,
    pub misses: usize,
}

/// One `AnalyzeOutput::packs_loaded` entry — a loaded DSL rule pack's id, its rule count as loaded
/// (before `disabled_rules` gating), its provenance (`PackSource::as_str`: `"dir"` | `"inline"`), and
/// how many of this tree's analyzed files fall in scope of >=1 of its rules' `file_pattern`s.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackLoaded {
    pub id: String,
    pub rules: usize,
    pub source: String,
    /// The per-pack applicability signal (D16 follow-up): analyzed files matching at least one of this
    /// pack's rule `file_pattern`s (`analyze::diagnostics::compute_dsl_scope`'s census — exact per-file
    /// counts, shared with the tree-wide zero-applicability warning). `0` on a loaded pack is the
    /// per-pack "never applicable here" disclosure: `typescript: 12 rules` on a pure-Go tree reads
    /// `filesInScope: 0`, so zero findings from that pack means "out of scope", not "clean".
    pub files_in_scope: usize,
}

impl PackLoaded {
    /// Builds `AnalyzeOutput::packs_loaded` from `config.packs` + `config.pack_sources`, sorted by pack
    /// id (deterministic regardless of load order). A pack id with no `pack_sources` entry reports
    /// `"inline"` — see `EngineConfig::pack_sources`. `files_in_scope` is
    /// `analyze::diagnostics::DslScope::files_in_scope_by_pack` — parallel to `config.packs` ORDER (the
    /// pairing happens before the id sort), one count per pack; a missing entry (never happens from the
    /// two real call sites, which compute the census over the same `config.packs`) degrades to `0`.
    /// Shared by `analyze::assemble` and `envelope::analyze_envelope`, so both entry points confirm the
    /// identical pack set.
    pub(crate) fn from_config(config: &EngineConfig, files_in_scope: &[usize]) -> Vec<PackLoaded> {
        let mut loaded: Vec<PackLoaded> = config
            .packs
            .iter()
            .enumerate()
            .map(|(i, pack)| PackLoaded {
                id: pack.id.clone(),
                rules: pack.rules.len(),
                source: config
                    .pack_sources
                    .get(&pack.id)
                    .copied()
                    .unwrap_or(PackSource::Inline)
                    .as_str()
                    .to_string(),
                files_in_scope: files_in_scope.get(i).copied().unwrap_or(0),
            })
            .collect();
        loaded.sort_by(|a, b| a.id.cmp(&b.id));
        loaded
    }
}
