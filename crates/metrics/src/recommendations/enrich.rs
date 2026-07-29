//! Per-item ROI enrichment and action-hint derivation, plus the config-exclude post-filter.

use crate::report_excludes::path_excluded;
use crate::roi::{compute_roi, RecId};
use zzop_core::{FileNode, Severity};

use super::types::{ActionHintKey, BuildRecInput, RawItem, RecItem};

/// LOC boundary between `fat-fanout-small` and `-large`.
const FAT_FANOUT_LOC: u32 = 100;
/// fanIn at/above which a bug-prone file is "shared".
const BUG_PRONE_SHARED_FANIN: u32 = 3;
/// fanIn at/above which a hot-churn file is "core".
const HOT_CHURN_CORE_FANIN: u32 = 5;

/// True when the config's top-level `exclude` covers `path` — rule-agnostic, exactly as it is for
/// findings (`zzop_core::is_suppressed`'s `global_excludes` arm), and evaluated with the SAME matcher.
/// Delegates to [`crate::report_excludes::path_excluded`], which every score channel shares, so the
/// recommendations filter can never become a second dialect of the other two.
pub(super) fn is_filtered(path: &str, input: &BuildRecInput) -> bool {
    path_excluded(input.excludes, path)
}

pub(super) fn enrich(
    rule_id: RecId,
    severity: Severity,
    item: RawItem,
    node: Option<&FileNode>,
    bug_evidence: Vec<String>,
) -> RecItem {
    let base_risk = node.map_or(0.0, |n| n.risk_score);
    let loc = node.map_or(0, |n| n.loc);
    let fan_in = node.map_or(0, |n| n.fan_in);
    let r = compute_roi(rule_id, severity, base_risk, loc, fan_in);
    RecItem {
        path: item.path,
        note: item.note,
        estimated_reduction: r.estimated_reduction,
        estimated_cost: r.estimated_cost,
        roi: r.roi,
        action_hint_key: derive_action_hint_key(rule_id, node),
        fan_in,
        bug_evidence,
        escalated_from: None,
    }
}

/// Maps rule + file metrics to an `ActionHintKey`. Three rules (fat-fanout, bug-prone, hot-churn)
/// produce metric-based sub-keys; all others map to their rule id.
fn derive_action_hint_key(rule_id: RecId, node: Option<&FileNode>) -> ActionHintKey {
    match rule_id {
        RecId::FatFanout => {
            if node.map_or(0, |n| n.loc) < FAT_FANOUT_LOC {
                ActionHintKey::FatFanoutSmall
            } else {
                ActionHintKey::FatFanoutLarge
            }
        }
        RecId::BugProne => {
            if node.map_or(0, |n| n.fan_in) >= BUG_PRONE_SHARED_FANIN {
                ActionHintKey::BugProneShared
            } else {
                ActionHintKey::BugProneIsolated
            }
        }
        RecId::HotChurn => {
            if node.map_or(0, |n| n.fan_in) >= HOT_CHURN_CORE_FANIN {
                ActionHintKey::HotChurnCore
            } else {
                ActionHintKey::HotChurnLeaf
            }
        }
        RecId::Circular => ActionHintKey::Circular,
        RecId::HiddenCoupling => ActionHintKey::HiddenCoupling,
        RecId::KnowledgeSilo => ActionHintKey::KnowledgeSilo,
        RecId::VersioningCandidate => ActionHintKey::VersioningCandidate,
        RecId::UrgentBugRisk => {
            unreachable!("UrgentBugRisk is a post-escalation synthetic group id — derive_action_hint_key is only ever called with an item's original rule id, before escalation (see RecId's doc)")
        }
    }
}
