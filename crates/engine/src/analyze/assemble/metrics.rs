//! Phase 6: git-history-dependent metrics (`scores`/`health`/`recommendations`/`critical`/`seams`/
//! `layer_co_churn`) — all `None`/empty when `git_active` is `false` (see `super::dep_graph`'s doc for
//! why `nodes`/`dep` still build unconditionally either way).

use std::collections::HashMap;
use std::time::Instant;

use zzop_core::{ir::DepGraph, FileNode, Finding};
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

    let (scores, health, recommendations, critical, seams) = if git_active {
        let coupling = build_coupling(commits, COUPLING_TOP_PER_FILE);

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

        let t0 = profile.then(Instant::now);
        let health = compute_health_index(&scores);
        record_native_timing(rule_time, t0, "health", 0);

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

        let t0 = profile.then(Instant::now);
        let critical = compute_criticality(
            nodes,
            dep,
            CRITICALITY_MIN_BLAST_RADIUS,
            CRITICALITY_SILENT_CHANGE_MAX,
            CRITICALITY_LIMIT,
        );
        record_native_timing(rule_time, t0, "criticality", critical.len());

        let t0 = profile.then(Instant::now);
        let seams = compute_seams(dep, &coupling, SEAMS_MIN_FILES, SEAMS_LIMIT);
        record_native_timing(rule_time, t0, "seams", seams.len());

        (Some(scores), Some(health), recommendations, critical, seams)
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
