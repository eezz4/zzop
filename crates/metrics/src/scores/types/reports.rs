//! Per-metric score report structs plus the aggregate `Scores` collection.

use serde::{Deserialize, Serialize};

use super::violations::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FsdScore {
    pub score: f64,
    pub total_imports: u32,
    pub violations: Vec<FsdViolation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CohesionScore {
    pub score: f64,
    pub slices: Vec<SliceCohesion>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CouplingScore {
    pub score: f64,
    pub avg_fan_out: f64,
    pub max_fan_out: f64,
    pub circular_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SdpScore {
    pub score: f64,
    pub total_cross_slice_edges: u32,
    pub violations: Vec<SdpViolation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HierarchyScore {
    pub score: f64,
    pub total_intra_module_edges: u32,
    pub violations: Vec<HierarchyViolation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicApiScore {
    pub score: f64,
    pub total_cross_module_imports: u32,
    pub deep_imports: Vec<DeepImport>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SfcScore {
    pub score: f64,
    pub limit: u32,
    pub compliant: u32,
    pub total: u32,
    pub violations: Vec<SfcViolation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MainSequenceScore {
    pub score: f64,
    pub avg_distance: f64,
    pub modules: Vec<ModuleMainSeq>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModularityScore {
    pub score: f64,
    pub q: f64,
    pub edge_count: u32,
    pub slice_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GodFileScore {
    pub score: f64,
    pub limit: u32,
    pub files: Vec<GodFile>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SiblingCrossScore {
    pub score: f64,
    pub total_intra_module_edges: u32,
    pub violations: Vec<SiblingCross>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiamondScore {
    pub score: f64,
    pub pairs: Vec<DiamondPair>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameScore {
    pub score: f64,
    pub renamed: u32,
    pub total: u32,
    pub files: Vec<RenamedFile>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BusFactorScore {
    pub score: f64,
    pub risky: u32,
    pub files: Vec<BusFactorFile>,
}

/// FIX commit ratio — lower means fewer reactive fixes. Score 0 at 30%, 100 at 0%.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixRatioScore {
    pub score: f64,
    pub fix: u32,
    pub total: u32,
    pub ratio: f64,
}

/// as-cast / any-type density — lower means higher TypeScript type confidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeSafetyScore {
    pub score: f64,
    pub total_as_cast: u32,
    pub total_any_type: u32,
    pub violations: Vec<TypeSafetyViolation>,
}

/// Law of Demeter — a.b.c+ chain density. Lower means less indirect coupling.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LodScore {
    pub score: f64,
    pub total_violations: u32,
    pub violations: Vec<LodFileSummary>,
}

/// The aggregate score report — one field per structural-health metric.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scores {
    pub fsd: FsdScore,
    pub cohesion: CohesionScore,
    pub coupling: CouplingScore,
    pub sdp: SdpScore,
    pub hierarchy: HierarchyScore,
    pub public_api: PublicApiScore,
    pub sfc: SfcScore,
    pub main_sequence: MainSequenceScore,
    pub modularity: ModularityScore,
    pub god_file: GodFileScore,
    pub sibling_cross: SiblingCrossScore,
    pub diamond: DiamondScore,
    pub rename_instability: RenameScore,
    pub bus_factor: BusFactorScore,
    pub fix_ratio: FixRatioScore,
    pub type_safety: TypeSafetyScore,
    pub lod: LodScore,
}
