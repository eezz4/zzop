//! Per-metric violation and summary element structs — the per-file/per-edge rows carried inside each score report.

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
