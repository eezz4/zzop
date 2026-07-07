//! zzop-metrics — the score channel: roi/health/criticality/seams/recommendations and related
//! whole-tree aggregate computations that produce SCORES (not findings) from `zzop-core`'s IR types
//! (`FileNode`/`DepGraph`/`CommitFileSet`/`Severity`). Depends only on `zzop-core` by design: metrics
//! computes scores from core's IR, never the other way around.
//! `zzop-engine` is the sole caller, assembling this crate's outputs into `AnalyzeOutput`'s score
//! fields; rule crates never depend on this one (metrics scores are not findings).

pub mod aggregates;
pub mod commit_tags;
pub mod coupling;
pub mod criticality;
pub mod cross_layer_co_churn;
pub mod diagnostics;
pub mod generic_interface_keys;
pub mod health;
pub mod recommendations;
pub mod roi;
pub mod scores;
pub mod seams;
pub mod trend;

use zzop_core::{register_native_analysis_stub, RuleRegistry, Severity};

/// Registers every native analysis id whose implementation lives in THIS crate — the metrics half of the
/// extensibility contract's per-crate registration (see `rules/README.md`'s "Adding a rule" section and
/// `zzop_engine::register_all_native`, which composes this with `zzop_rules_graph`'s, `zzop_rules_http`'s,
/// `zzop_rules_cross_layer`'s, and `zzop_rules_schema`'s own `register_native_analyses`). These 5 ids are
/// not findings-producing rules — they gate SCORE
/// computations (`compute_seams`/`compute_criticality`/`compute_scores`/`compute_health_index`/
/// `build_recommendations`) that only ride the same enabled/severity/suppression toggle surface as native
/// rules do (see this crate's module doc). Ids/severities moved verbatim from the old
/// `zzop_core::register_native_analyses` table.
pub fn register_native_analyses(registry: &mut RuleRegistry) {
    let analyses: &[(&str, Severity)] = &[
        ("seams", Severity::Info),
        ("criticality", Severity::Warning),
        ("scores", Severity::Info),
        ("health", Severity::Info),
        ("recommendations", Severity::Info),
    ];
    for &(id, default_severity) in analyses {
        register_native_analysis_stub(registry, id, default_severity);
    }
}

pub use aggregates::{
    aggregate_action_deps, aggregate_by_folder, aggregate_dep_by_folder, build_folder_aggregates,
    ActionDepSummary, ActionUse, FolderAggregates, FolderEdge, FolderSummary, DEFAULT_FOLDER_DEPTH,
};

pub use commit_tags::default_commit_type_patterns;

pub use coupling::{
    build_coupling, CouplingEntry, CouplingMap, COUPLING_TOP_PER_FILE, MAX_FILES_PER_COMMIT,
    MIN_FILES_PER_COMMIT,
};

pub use criticality::{
    compute_criticality, CriticalFile, CRITICALITY_LIMIT, CRITICALITY_MIN_BLAST_RADIUS,
    CRITICALITY_SILENT_CHANGE_MAX,
};

pub use cross_layer_co_churn::{
    build_cross_layer_co_churn, layer_of, CrossLayerCoChurn, CrossLayerCoChurnOptions,
    CrossLayerExample,
};

pub use diagnostics::{
    build_diagnostics, AnalysisDiagnostics, DiagnosticsInput, GitDiagnosticsInput,
};

pub use generic_interface_keys::default_generic_interface_key_patterns;

pub use health::{compute_health_index, HealthContributor, HealthIndex, HEALTH_METRIC_WEIGHTS};

pub use recommendations::{
    build_recommendations, ActionHintKey, BuildRecInput, RecItem, Recommendation,
    RecommendationGates,
};

pub use roi::{compute_roi, RecId, RoiResult};

pub use scores::compute::{compute_scores, ScoresInput};
pub use scores::config::ScoresConfig;
pub use scores::types::Scores;

pub use seams::{compute_seams, SeamCandidate, SEAMS_LIMIT, SEAMS_MIN_FILES};

pub use trend::{build_trend_series, TrendPoint, TrendSeries, TrendSnapshot, TOP_N};
