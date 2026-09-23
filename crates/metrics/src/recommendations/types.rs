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
    /// Ordering weight, NOT an estimate in any unit — `base_risk × reductionRatio × severityMultiplier`,
    /// where the two multipliers are hardcoded per rule id and per severity (`roi.rs`) with no
    /// calibration behind them, and `base_risk` is `calc_risk_score`, an UNNORMALISED sum of a commit
    /// count, a line count and an edge count. Lines dominate that sum by orders of magnitude on any real
    /// file, so read this as "churn-weighted, severity-tilted" rather than as risk that would go away.
    /// It answers "which of these should I look at first", never "how much safer will I be".
    pub estimated_reduction: f64,
    /// Ordering weight for effort, same caveat: `max(10, loc + fanIn × 3)`. It is a size proxy, not a
    /// claim about human effort — nothing here has ever been measured against how long a fix took.
    pub estimated_cost: f64,
    /// `reduction × severityMultiplier / cost`. Dimensionless and comparable only WITHIN one reply's
    /// item list; across runs or repos it means nothing, because both inputs are uncalibrated weights
    /// rather than measured quantities. Since `cost` is size-dominated, ranking by `roi` ranks small
    /// files up — that is the intended "cheap wins first" tilt, and it is a preference, not a finding.
    /// The channel these fields ride is a RANKING (`docs/rules/catalog.md`'s recommendation table says
    /// so outright); `findings` is where defects live.
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
    /// What [`Self::id`] MEANS, shipped beside the value it explains.
    ///
    /// # Why it rides here and not in a lookup table (2026-09-14, review ledger V214)
    ///
    /// The analyze reply used to call `id` "one of a CLOSED SET", which invited a consumer to
    /// hard-code the spellings — a promise this project does not make, because `VERSIONING.md` puts
    /// field VALUES outside the freeze and the set can gain a member in a MINOR release. The consumer
    /// who accepted the invitation breaks on a change that is not a compatibility break.
    ///
    /// The repair is to remove the need for the promise rather than enlarge it, and this workspace
    /// already ships that design one lane over: `zzop_facade`'s io `verdict` carries `verdictMeaning`
    /// FOR THE TOKEN IT RETURNED. A value that explains itself needs no memorized list, and an id
    /// added later arrives explaining itself.
    ///
    /// SERIALIZED FROM THE ENUM, never stored: [`RecId::meaning`] is the one owner, so a renamed id
    /// cannot keep an explanation about the old one. It rides on the WIRE rather than being resolved
    /// by each reader because the shaping crate is forbidden a shipped dependency on this one — that
    /// layering is why forwarding the sentence is the only honest way for it to reach the reply.
    ///
    /// PRIVATE, and that is the seal rather than a style choice (2026-09-14, review ledger V234). The
    /// paragraph above says a renamed id "cannot keep an explanation about the old one" — which was a
    /// discipline, not a mechanism: both construction sites spelled `id` and `id_meaning` as two
    /// independent expressions, held together by a comment asking the next author to look. Only
    /// [`Recommendation::new`] can set this now, from the id it is handed, so the drift the sentence
    /// above forbids is no longer expressible.
    #[serde(rename = "idMeaning")]
    id_meaning: &'static str,
    pub severity: Severity,
    /// Sorted in descending ROI order.
    pub items: Vec<RecItem>,
    /// How many rows this rule's own cap dropped. `0` means the list is complete.
    ///
    /// ALWAYS SERIALIZED, including as `0` — the same call `crate::scores::detail_cap` made and for the
    /// same reason: a field that vanishes when nothing was dropped makes "complete list" and "this build
    /// has no disclosure" identical bytes again, which is the silence being repaired.
    ///
    /// SCOPE: the cap, and only the cap. `items.len() + items_truncated` is what the rule PRODUCED, not
    /// what the tree holds — config excludes and critical-escalation both move rows after this number is
    /// taken. `super::rules`' module doc owns why one scalar cannot carry all three.
    pub items_truncated: u32,
}

impl Recommendation {
    /// The ONLY way to build one, so [`Self::id_meaning`] cannot disagree with [`Self::id`].
    ///
    /// Every other field stays public: they are independent facts about a run and there is nothing to
    /// couple. The id and its sentence are one fact wearing two field names, and this is where that is
    /// enforced instead of asked for.
    pub(crate) fn new(
        id: RecId,
        severity: Severity,
        items: Vec<RecItem>,
        items_truncated: u32,
    ) -> Self {
        Self {
            id,
            id_meaning: id.meaning(),
            severity,
            items,
            items_truncated,
        }
    }
}

/// One rule's output before enrichment: its id, the severity every item of it carries, the rows that
/// survived its cap, and how many the cap dropped.
///
/// A struct rather than the 4-tuple it replaced. The 3-tuple was already at the edge of readable at the
/// six construction sites, and the field this change adds is a bare `u32` sitting beside a `Vec` — in a
/// tuple those are told apart only by position, which is exactly how a disclosure count ends up
/// measuring the wrong list.
pub(super) struct RuleOutput {
    pub(super) id: RecId,
    pub(super) severity: Severity,
    pub(super) items: Vec<RawItem>,
    pub(super) items_truncated: u32,
}

impl RuleOutput {
    pub(super) fn new(
        id: RecId,
        severity: Severity,
        items: Vec<RawItem>,
        items_truncated: u32,
    ) -> Self {
        RuleOutput {
            id,
            severity,
            items,
            items_truncated,
        }
    }
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
