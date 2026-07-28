//! Phase 6: git-history-dependent metrics (`scores`/`health`/`recommendations`/`critical`/`seams`/
//! `layer_co_churn`) — all `None`/empty when `git_active` is `false` (see `super::dep_graph`'s doc for
//! why `nodes`/`dep` still build unconditionally either way).
//!
//! ## `disabledRules` gating (the five `zzop_metrics::register_native_analyses` ids)
//! `seams`/`criticality`/`scores`/`health`/`recommendations` are registered native analysis ids
//! (`crates/metrics/src/lib.rs`), so they share the `RuleConfig` id space every DSL rule and native
//! rule rides — and this file is their SOLE call site. Each therefore carries its own
//! [`is_enabled`] gate here, exactly as `super::rules` does for `circular`/`unimported-export`/...
//! Without them the ids were accepted by `disabledRules` and then ignored: no `configWarnings` entry
//! (they ARE known ids) AND a positive `ruleOverridesApplied.disabled` confirmation that the disable
//! had been applied, while the analysis kept running and kept emitting. A disable that reports
//! success and does nothing is worse than a silent one, so the gate is not optional here.
//!
//! A disabled id yields the SAME shape the git-inactive branch already yields (`None` for
//! `scores`/`health`, `[]` for the three lists) — no new output state to consume, and no timing entry
//! under that id in the `--profile-rules` map, since nothing ran.
//!
//! Two internal dependencies mean "gate" is not always "skip the work" — both resolved in favour of
//! each gate meaning exactly what it says about ITS OWN output, never a hidden cascade onto another
//! id's:
//! - `health` is computed FROM `scores` (`compute_health_index(&scores)`), so `scores` is computed
//!   whenever either id is enabled. Disabling `scores` alone still suppresses the `scores` field; it
//!   just cannot also skip the computation while `health` still needs it (and the run then still
//!   records the work honestly under the `scores` timing key — it did run).
//! - `coupling` feeds `recommendations` and `seams` only, so it is built only when at least one of
//!   those two is enabled.
//!
//! `layer_co_churn` is deliberately NOT gated: it is not a registered id, so `disabledRules` never
//! claims to control it (the only honest state — see `zzop_metrics::register_native_analyses`).

use std::collections::HashMap;
use std::time::Instant;

use zzop_core::{ir::DepGraph, is_enabled, FileNode, Finding};
use zzop_metrics::{
    build_coupling, build_cross_layer_co_churn, build_recommendations, compute_criticality,
    compute_health_index, compute_scores, compute_seams, layer_of, scores::types::FileKinds,
    BuildRecInput, CriticalFile, CrossLayerCoChurn, CrossLayerCoChurnOptions, HealthIndex,
    Recommendation, RecommendationGates, Scores, ScoresInput, SeamCandidate, COUPLING_TOP_PER_FILE,
    CRITICALITY_LIMIT, CRITICALITY_MIN_BLAST_RADIUS, CRITICALITY_SILENT_CHANGE_MAX, SEAMS_LIMIT,
    SEAMS_MIN_FILES,
};

use crate::EngineConfig;

use crate::analyze::record_native_timing;

pub(super) struct MetricsResult {
    pub(super) scores: Option<Scores>,
    pub(super) health: Option<HealthIndex>,
    pub(super) recommendations: Vec<Recommendation>,
    pub(super) critical: Vec<CriticalFile>,
    pub(super) seams: Vec<SeamCandidate>,
    pub(super) layer_co_churn: Option<Vec<CrossLayerCoChurn>>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn compute(
    config: &EngineConfig,
    nodes: &[FileNode],
    dep: &DepGraph,
    cycles: &[Vec<String>],
    commits: &[zzop_core::CommitFileSet],
    git_active: bool,
    findings: &[Finding],
    rule_time: &mut HashMap<String, (u128, usize)>,
) -> MetricsResult {
    let profile = config.profile_rules;
    // `is_source`: same dispatch-classification closure `super::dep_graph::build` used to build
    // `nodes` — recreated here (a pure fn of `config.dispatch`, zero-cost) rather than threaded
    // through, since a closure capturing `config` can't cross a function boundary as a struct field.
    let is_source = |id: &str| crate::dispatch::dispatch(id, &config.dispatch).is_some();

    // One `is_enabled` read per registered id, up front — see this module's doc for why each of the
    // five needs its own gate and why `scores`/`coupling` are still computed for a dependent id.
    let scores_on = is_enabled(&config.rule_config, "scores");
    let health_on = is_enabled(&config.rule_config, "health");
    let recommendations_on = is_enabled(&config.rule_config, "recommendations");
    let criticality_on = is_enabled(&config.rule_config, "criticality");
    let seams_on = is_enabled(&config.rule_config, "seams");

    let (scores, health, recommendations, critical, seams) = if git_active {
        let coupling = if recommendations_on || seams_on {
            build_coupling(commits, COUPLING_TOP_PER_FILE)
        } else {
            Default::default()
        };

        let computed_scores = (scores_on || health_on).then(|| {
            let t0 = profile.then(Instant::now);
            let scores = compute_scores(
                &ScoresInput {
                    nodes,
                    dep,
                    circular: cycles,
                    target: None,
                    file_kinds: &FileKinds::new(),
                    type_safety_counts: &HashMap::new(),
                    lod_by_file: &HashMap::new(),
                    is_source: &is_source,
                },
                &config.scores_config,
            );
            // `scores`/`health` produce one struct, not a `Vec` — `findings: 0` is the convention for a
            // native analysis id with nothing list-shaped to count.
            record_native_timing(rule_time, t0, "scores", 0);
            scores
        });

        let health = computed_scores
            .as_ref()
            .filter(|_| health_on)
            .map(|scores| {
                let t0 = profile.then(Instant::now);
                let health = compute_health_index(scores);
                record_native_timing(rule_time, t0, "health", 0);
                health
            });

        let recommendations = if recommendations_on {
            let t0 = profile.then(Instant::now);
            let recommendations = build_recommendations(
                &BuildRecInput {
                    nodes,
                    dep,
                    coupling: &coupling,
                    circular: cycles,
                    scope_excludes: &[],
                    permanent_ignores: &[],
                    untested_paths: &std::collections::HashSet::new(),
                    amplification_by_path: &HashMap::new(),
                    findings,
                },
                &RecommendationGates::default(),
            );
            record_native_timing(rule_time, t0, "recommendations", recommendations.len());
            recommendations
        } else {
            Vec::new()
        };

        let critical = if criticality_on {
            let t0 = profile.then(Instant::now);
            let critical = compute_criticality(
                nodes,
                dep,
                CRITICALITY_MIN_BLAST_RADIUS,
                CRITICALITY_SILENT_CHANGE_MAX,
                CRITICALITY_LIMIT,
            );
            record_native_timing(rule_time, t0, "criticality", critical.len());
            critical
        } else {
            Vec::new()
        };

        let seams = if seams_on {
            let t0 = profile.then(Instant::now);
            let seams = compute_seams(dep, &coupling, SEAMS_MIN_FILES, SEAMS_LIMIT);
            record_native_timing(rule_time, t0, "seams", seams.len());
            seams
        } else {
            Vec::new()
        };

        // `scores` is dropped here (not above) when only `health` asked for it — the field is
        // suppressed, the work it fed was still real.
        (
            computed_scores.filter(|_| scores_on),
            health,
            recommendations,
            critical,
            seams,
        )
    } else {
        (None, None, Vec::new(), Vec::new(), Vec::new())
    };

    // `AnalyzeOutput::layer_co_churn` — git-gated like `scores`/`health` above: `None` when git is
    // inactive, `Some` (possibly an empty `Vec`) when it succeeded. `layer_of` folds
    // `hierarchy_shared_dirs` into a shared, non-layer sentinel.
    let layer_co_churn = git_active.then(|| {
        build_cross_layer_co_churn(
            commits,
            |p| layer_of(p, &config.scores_config.hierarchy_shared_dirs),
            &CrossLayerCoChurnOptions::default(),
        )
    });

    MetricsResult {
        scores,
        health,
        recommendations,
        critical,
        seams,
        layer_co_churn,
    }
}
