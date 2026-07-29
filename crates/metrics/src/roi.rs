//! ROI scoring. Pure scalar functions: the caller extracts risk/loc/fanIn from a FileNode (or passes 0
//! when absent). (`hotspot_score` — the other half of the original roi/hotspot module — moved to
//! `zzop_core::file_nodes` in the R3 crate-boundary batch: `build_file_nodes` is a core mechanism and
//! must stay reachable from core without an upward dependency on this crate — per the crate-boundary
//! split.)

use serde::{Deserialize, Serialize};

use zzop_core::Severity;

const MIN_COST: f64 = 10.0;
const FANIN_COST_WEIGHT: f64 = 3.0;

/// Recommendation rule id.
///
/// `UrgentBugRisk` is not a rule — it is the synthetic escalation group id
/// (`recommendations::escalate_critical_bug_evidence`) that critical-finding-confirmed items are moved
/// into so they sort to the top without inflating their ROI. `compute_roi`/`derive_action_hint_key` are
/// only ever called with an item's ORIGINAL rule id, before escalation moves it — so `reduction_ratio`
/// below never actually receives `UrgentBugRisk` at runtime; its match arm exists only to keep the
/// match exhaustive and panics loudly if that invariant is ever broken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RecId {
    BugProne,
    Circular,
    HotChurn,
    FatFanout,
    HiddenCoupling,
    KnowledgeSilo,
    VersioningCandidate,
    /// Synthetic escalation-only group id — see the enum doc above.
    UrgentBugRisk,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoiResult {
    pub roi: f64,
    pub estimated_reduction: f64,
    pub estimated_cost: f64,
}

/// ROI scalar (D1 formula):
/// reduction = base_risk * reductionRatio(rule) * severityMultiplier(sev)
/// cost      = max(10, loc + fanIn*3)
/// roi       = reduction / cost
///
/// The formula once carried two more cost multipliers — an untested-path doubling and a
/// change-amplification factor — driven by `BuildRecInput` fields that NO production caller ever
/// populated (the engine passed an empty set and an empty map at the sole call site, so both multipliers
/// were constant 1.0 in every real run). They were deleted rather than wired because nothing in the
/// config surface could feed them: neither "which paths have tests" nor "co-change amplification per
/// path" is a quantity this pipeline computes. Reinstating either means first building its input.
pub fn compute_roi(
    rule_id: RecId,
    severity: Severity,
    base_risk: f64,
    loc: u32,
    fan_in: u32,
) -> RoiResult {
    let reduction = base_risk * reduction_ratio(rule_id) * severity_multiplier(severity);
    let cost = (loc as f64 + fan_in as f64 * FANIN_COST_WEIGHT).max(MIN_COST);
    RoiResult {
        roi: reduction / cost,
        estimated_reduction: reduction,
        estimated_cost: cost,
    }
}

fn reduction_ratio(rule_id: RecId) -> f64 {
    match rule_id {
        RecId::BugProne | RecId::Circular | RecId::FatFanout => 0.7,
        RecId::HotChurn | RecId::HiddenCoupling | RecId::VersioningCandidate => 0.5,
        RecId::KnowledgeSilo => 0.3,
        RecId::UrgentBugRisk => {
            unreachable!("UrgentBugRisk is a post-escalation synthetic group id — compute_roi is only ever called with an item's original rule id, before escalation")
        }
    }
}

fn severity_multiplier(severity: Severity) -> f64 {
    match severity {
        Severity::Critical => 3.0,
        Severity::Warning => 2.0,
        Severity::Info => 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roi_basic_formula() {
        // risk 100, loc 50, fanIn 10, circular(0.7) x critical(3); cost = max(10, 50+30)=80.
        let r = compute_roi(RecId::Circular, Severity::Critical, 100.0, 50, 10);
        assert!((r.estimated_reduction - 210.0).abs() < 1e-9);
        assert!((r.estimated_cost - 80.0).abs() < 1e-9);
        assert!((r.roi - 2.625).abs() < 1e-9);
    }

    #[test]
    fn roi_cost_floor_applies_to_tiny_files() {
        // loc 1, fanIn 0 -> raw cost 1, floored to MIN_COST.
        let r = compute_roi(RecId::Circular, Severity::Critical, 100.0, 1, 0);
        assert!((r.estimated_cost - 10.0).abs() < 1e-9);
    }

    #[test]
    fn roi_zero_risk_is_zero() {
        let r = compute_roi(RecId::KnowledgeSilo, Severity::Info, 0.0, 0, 0);
        assert_eq!(r.estimated_reduction, 0.0);
        assert_eq!(r.roi, 0.0);
    }
}
