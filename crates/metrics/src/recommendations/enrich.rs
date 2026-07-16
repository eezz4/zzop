//! Per-item ROI enrichment and action-hint derivation, plus the post-filters (`scope_excludes` /
//! `permanent_ignores`) and the minimal glob matcher they use.

use regex::Regex;

use crate::roi::{compute_roi, RecId};
use zzop_core::{FileNode, Severity};

use super::types::{ActionHintKey, BuildRecInput, RawItem, RecItem};

/// LOC boundary between `fat-fanout-small` and `-large`.
const FAT_FANOUT_LOC: u32 = 100;
/// fanIn at/above which a bug-prone file is "shared".
const BUG_PRONE_SHARED_FANIN: u32 = 3;
/// fanIn at/above which a hot-churn file is "core".
const HOT_CHURN_CORE_FANIN: u32 = 5;

pub(super) fn is_filtered(rule_id: RecId, path: &str, input: &BuildRecInput) -> bool {
    for (rid, p) in input.permanent_ignores {
        if *rid == rule_id && p == path {
            return true;
        }
    }
    for (rid, glob) in input.scope_excludes {
        if *rid == rule_id && matches_glob(path, glob) {
            return true;
        }
    }
    false
}

#[allow(clippy::too_many_arguments)]
pub(super) fn enrich(
    rule_id: RecId,
    severity: Severity,
    item: RawItem,
    node: Option<&FileNode>,
    untested: bool,
    amplification: f64,
    bug_evidence: Vec<String>,
) -> RecItem {
    let base_risk = node.map_or(0.0, |n| n.risk_score);
    let loc = node.map_or(0, |n| n.loc);
    let fan_in = node.map_or(0, |n| n.fan_in);
    let r = compute_roi(
        rule_id,
        severity,
        base_risk,
        loc,
        fan_in,
        untested,
        amplification,
    );
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

/// Minimal glob: "**" matches any characters (including "/"), "*" matches non-slash characters.
pub(super) fn matches_glob(path: &str, glob: &str) -> bool {
    let mut escaped = String::with_capacity(glob.len());
    for c in glob.chars() {
        if matches!(
            c,
            '.' | '+' | '^' | '$' | '{' | '}' | '(' | ')' | '|' | '[' | ']' | '\\'
        ) {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    // Placeholder for "**" while single "*" is rewritten — must not collide with `escaped`'s output,
    // which can only contain the glob's original characters plus backslash escapes.
    const DOUBLE_STAR_PLACEHOLDER: &str = "\u{0}";
    let rewritten = escaped
        .replace("**", DOUBLE_STAR_PLACEHOLDER)
        .replace('*', "[^/]*")
        .replace(DOUBLE_STAR_PLACEHOLDER, ".*");
    let anchored = format!("^{rewritten}$");
    Regex::new(&anchored)
        .map(|re| re.is_match(path))
        .unwrap_or(false)
}
