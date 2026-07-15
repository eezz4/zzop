//! Score types — per-metric violation and summary structs, plus the aggregate `Scores` collection.
//! Rust field names stay snake_case per Rust/crate convention (see e.g. node.rs FileNode); every struct
//! here carries `#[serde(rename_all = "camelCase")]` so the WIRE (JSON) shape matches every other
//! napi-boundary output type instead — see `crates/facade/src/lib.rs`'s `AnalyzeOutputView` doc.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Analysis-target role classification: which files count as "abstract" (interfaces/types) vs "concrete"
/// (implementations) for the main-sequence (Robert Martin A/I/D) metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileKind {
    Abstract,
    Concrete,
}

/// Per-file abstract/concrete classification, keyed by path.
pub type FileKinds = BTreeMap<String, FileKind>;

/// FSD layer-order kind: a lower layer reaching into a higher one ("layer-reverse"), or one L2 slice reaching
/// directly into another L2 slice's internals ("cross-slice").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FsdViolationKind {
    LayerReverse,
    CrossSlice,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FsdViolation {
    pub from: String,
    pub to: String,
    pub kind: FsdViolationKind,
    pub from_layer: u8,
    pub to_layer: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_slice: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_slice: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SliceCohesion {
    pub slice: String,
    pub file_count: usize,
    pub internal_edges: u32,
    pub outgoing_edges: u32,
    pub incoming_edges: u32,
    /// internal / (internal + outgoing) — internal cohesion ratio 0-1.
    pub cohesion: f64,
    /// outgoing / (incoming + outgoing) — instability 0-1 (Martin's I).
    pub instability: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SdpViolation {
    pub from_slice: String,
    pub to_slice: String,
    pub from_i: f64,
    pub to_i: f64,
    pub edge_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HierarchyViolation {
    pub from: String,
    pub to: String,
    pub module: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeepImport {
    pub from: String,
    pub to: String,
    pub to_module: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleMainSeq {
    pub module: String,
    pub file_count: usize,
    pub abstractness: f64,
    pub instability: f64,
    pub distance: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SfcViolation {
    pub path: String,
    pub loc: u32,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GodFile {
    pub path: String,
    pub loc: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SiblingCross {
    pub from: String,
    pub to: String,
    pub module: String,
    pub from_subdir: String,
    pub to_subdir: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiamondPair {
    pub root: String,
    pub leaf: String,
    pub through: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenamedFile {
    pub path: String,
    pub rename_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BusFactorFile {
    pub path: String,
    pub change_count: u32,
    pub authors: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeSafetyViolation {
    pub path: String,
    pub as_cast: u32,
    pub any_type: u32,
    pub loc: u32,
    /// (as_cast + any_type) / max(loc, 1).
    pub density: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LodFileSummary {
    pub path: String,
    pub count: u32,
    pub max_depth: u32,
    pub loc: u32,
    /// count / max(loc, 1).
    pub density: f64,
}

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
