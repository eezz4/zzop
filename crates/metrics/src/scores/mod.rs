//! Structural health scores (0-100, higher is better).
pub mod bus_factor;
pub mod cohesion;
pub mod compute;
pub mod config;
pub mod coupling;
pub mod diamond;
pub mod fix_ratio;
pub mod fsd;
pub mod god_file;
pub mod hierarchy;
pub mod lod;
pub mod main_sequence;
pub mod modularity;
pub mod public_api;
pub mod rename;
pub mod sdp;
pub mod sfc;
pub mod shared;
pub mod sibling_cross;
pub mod type_safety;
pub mod types;

pub use compute::{compute_scores, ScoresInput};
pub use config::ScoresConfig;
pub use lod::LodChain;
pub use type_safety::TypeSafetyCounts;
pub use types::{
    BusFactorScore, CohesionScore, CouplingScore, DiamondScore, FixRatioScore, FsdScore,
    GodFileScore, HierarchyScore, LodScore, MainSequenceScore, ModularityScore, PublicApiScore,
    RenameScore, Scores, SdpScore, SfcScore, SiblingCrossScore, TypeSafetyScore,
};
