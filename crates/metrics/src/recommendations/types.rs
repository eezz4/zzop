//! Serialized recommendation output shapes (`Recommendation`/`RecItem`/`ActionHintKey`), the
//! `RecommendationGates` thresholds, `BuildRecInput`, and the pre-enrichment `RawItem`.

use serde::{Deserialize, Serialize};

use crate::coupling::CouplingMap;
use crate::roi::RecId;
use zzop_core::{DepGraph, FileNode, Finding, GlobalExclude, Severity};

/// actionHint i18n key — resolved via FE `labels.action[<key>]`; branched on rule + metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActionHintKey {
    FatFanoutSmall,
    FatFanoutLarge,
    BugProneShared,
    BugProneIsolated,
    HotChurnCore,
    HotChurnLeaf,
    Circular,
    HiddenCoupling,
    KnowledgeSilo,
    VersioningCandidate,
}

/// A single improvement target — file-level.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecItem {
    pub path: String,
    /// Human-readable one-line context (e.g. "FIX 8 · risk 120").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Estimated risk reduction (>= 0).
    pub estimated_reduction: f64,
    /// Estimated fix cost (>= 10, floor guaranteed).
    pub estimated_cost: f64,
    /// ROI = reduction x severityMultiplier / cost.
    pub roi: f64,
    /// i18n key for the FE Labels `action[<key>]` lookup.
    pub action_hint_key: ActionHintKey,
    /// For leaf-first sorting; 0 when node is absent. Lower fanIn = more leaf-like.
    pub fan_in: u32,
    /// Deterministic strings evidencing WHY this file is bug-risky — never fed into `roi` (see this
    /// module's doc). Fixed order: critical-findings, fix-ratio, hotspot.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bug_evidence: Vec<String>,
    /// Set only on an item that was escalated into the `RecId::UrgentBugRisk` group — names the rule
    /// group it was moved OUT of, so a consumer can still tell which rule originally flagged it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub escalated_from: Option<RecId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recommendation {
    pub id: RecId,
    pub severity: Severity,
    /// Sorted in descending ROI order.
    pub items: Vec<RecItem>,
}

/// Rule-gate thresholds for `build_recommendations`. `Default` provides the baseline thresholds
/// used when no project-specific config overrides them.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RecommendationGates {
    pub bug_prone_fix: u32,
    pub hot_churn_min_loc: u32,
    pub hot_churn_ratio: f64,
    pub fat_fan_out: u32,
    pub barrel_fan_out_ratio: f64,
    pub hidden_coupling_min: u32,
    pub knowledge_silo_authors: u32,
    pub versioning_fan_in: u32,
    pub versioning_fix: u32,
}

impl Default for RecommendationGates {
    fn default() -> Self {
        RecommendationGates {
            bug_prone_fix: 5,
            hot_churn_min_loc: 30,
            hot_churn_ratio: 10.0,
            fat_fan_out: 8,
            barrel_fan_out_ratio: 0.5,
            hidden_coupling_min: 10,
            knowledge_silo_authors: 6,
            versioning_fan_in: 3,
            versioning_fix: 3,
        }
    }
}

/// Inputs to `build_recommendations`.
pub struct BuildRecInput<'a> {
    pub nodes: &'a [FileNode],
    pub dep: &'a DepGraph,
    pub coupling: &'a CouplingMap,
    pub circular: &'a [Vec<String>],
    /// The config's rule-agnostic path filter — `RuleConfig::global_excludes`, the top-level `"exclude"`
    /// config key. Matching paths are dropped from every recommendation group, using
    /// `zzop_core::global_exclude_matches_path` so this channel and the findings channel share ONE filter
    /// dialect rather than two (see this module's parent doc for the false positive that wired it).
    /// Callers with nothing to exclude pass an empty slice.
    pub excludes: &'a [GlobalExclude],
    /// Whole-tree findings — sole source of an item's critical-finding bug evidence (and the sole
    /// escalation trigger; see this module's doc). Not filtered to any particular rule id: any
    /// `Severity::Critical` finding on the item's path counts.
    pub findings: &'a [Finding],
}

/// A rule hit before ROI enrichment.
pub(super) struct RawItem {
    pub(super) path: String,
    pub(super) note: Option<String>,
}
