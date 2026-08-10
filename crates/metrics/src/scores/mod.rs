//! Structural health scores (0-100, higher is better).
pub mod bus_factor;
pub mod cohesion;
pub mod compute;
pub mod config;
pub mod coupling;
mod detail_cap;
pub mod diamond;
pub mod feature_sliced_design;
pub mod file_size_compliance;
pub mod fix_ratio;
pub mod god_file;
pub mod hierarchy;
pub mod main_sequence;
pub mod meanings;
pub mod modularity;
pub mod public_api;
pub mod rename;
pub mod sdp;
pub mod shared;
pub mod sibling_cross;
pub mod types;

pub use compute::{compute_scores, ScoresInput};
pub use config::ScoresConfig;
pub use meanings::{score_meaning, SCORE_MEANINGS};
pub use types::{
    BusFactorScore, CohesionScore, CouplingScore, DiamondScore, FeatureSlicedDesignScore,
    FileSizeComplianceScore, FixRatioScore, GodFileScore, HierarchyScore, MainSequenceScore,
    ModularityScore, PublicApiScore, RenameScore, Scores, SdpScore, SiblingCrossScore,
};
