//! Bug-evidence assembly and critical-finding escalation — see `crate::recommendations`'s module
//! doc for why evidence never feeds ROI and why only critical-`Finding` evidence escalates.

use std::cmp::Ordering;
use std::collections::HashMap;

use crate::roi::RecId;
use zzop_core::{FileNode, Finding, Severity};

use super::rules::tag_count;
use super::types::{RecItem, Recommendation};

/// `changeCount` floor below which a FIX-tag ratio is too small a sample to be meaningful evidence.
const FIX_RATIO_MIN_CHANGE_COUNT: u32 = 5;
/// FIX / changeCount ratio at/above which "N of M changes are bug-fix commits" is surfaced as evidence.
const FIX_RATIO_THRESHOLD: f64 = 0.5;
/// fanIn at/above which a hotspot file's blast radius makes "frequently changed and imported by N files"
/// worth surfacing as evidence.
const HOTSPOT_BLAST_FAN_IN: u32 = 5;

/// Sort rank for the `info` severity (critical = 0, warning = 1; lower = more severe).
const SEVERITY_RANK_INFO: u8 = 2;

/// path -> critical `Finding`s on that path — the sole substrate for both critical-finding evidence
/// text and the escalation decision (see this module's doc). Built once per `build_recommendations`
/// call rather than per-item, since the same path can be looked up by multiple rules' items.
pub(super) fn critical_findings_by_path(findings: &[Finding]) -> HashMap<&str, Vec<&Finding>> {
    let mut by_path: HashMap<&str, Vec<&Finding>> = HashMap::new();
    for f in findings {
        if f.severity == Severity::Critical {
            by_path.entry(f.file.as_str()).or_default().push(f);
        }
    }
    by_path
}

/// Moves every item whose path has >= 1 critical `Finding` (`critical_by_path`, the same map
/// `bug_evidence_for` already consulted to build the item's evidence text) out of its home group and
/// into a new synthetic `RecId::UrgentBugRisk` / `Severity::Critical` group — a MOVE (the item is
/// removed from `items`, not copied), so escalation never increases a path's total multiplicity. A
/// path that legitimately sat in two home groups (e.g. bug-prone AND fat-fanout) moves twice and so
/// appears twice in the urgent group, each entry keeping its own `escalated_from`/action hint —
/// honest (two distinct improvement angles for one file), not a double-count. `escalated_from` is set to the
/// home group's id before the move so a consumer can still recover which rule originally flagged the
/// file. Home groups left with no items are dropped, preserving `build_recommendations`' existing "every
/// returned group has >= 1 item" invariant. FIX-ratio/hotspot-only evidence never triggers this — see
/// this module's doc. Checking `critical_by_path` directly here (rather than pattern-matching the
/// already-built `bug_evidence` strings) keeps the escalation decision independent of the evidence text's
/// exact wording.
pub(super) fn escalate_critical_bug_evidence(
    mut recs: Vec<Recommendation>,
    critical_by_path: &HashMap<&str, Vec<&Finding>>,
) -> Vec<Recommendation> {
    let mut urgent_items: Vec<RecItem> = Vec::new();
    for rec in &mut recs {
        let home = rec.id;
        let mut kept = Vec::with_capacity(rec.items.len());
        for mut item in rec.items.drain(..) {
            if critical_by_path.contains_key(item.path.as_str()) {
                item.escalated_from = Some(home);
                urgent_items.push(item);
            } else {
                kept.push(item);
            }
        }
        rec.items = kept;
    }
    recs.retain(|r| !r.items.is_empty());

    if !urgent_items.is_empty() {
        urgent_items.sort_by(|a, b| {
            b.roi
                .partial_cmp(&a.roi)
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.path.cmp(&b.path))
        });
        recs.push(Recommendation {
            id: RecId::UrgentBugRisk,
            severity: Severity::Critical,
            items: urgent_items,
        });
    }

    recs.sort_by_key(|r| (severity_rank(r.severity), urgency_rank(r.id)));
    recs
}

fn severity_rank(s: Severity) -> u8 {
    match s {
        Severity::Critical => 0,
        Severity::Warning => 1,
        Severity::Info => SEVERITY_RANK_INFO,
    }
}

/// Secondary sort key within a severity band: `RecId::UrgentBugRisk` (0) sorts before every other group
/// (1) — the mechanism that actually lands the urgent group "on top" among same-severity groups (e.g.
/// `bug-prone`, also `Severity::Critical`), since severity rank alone ties between them.
fn urgency_rank(id: RecId) -> u8 {
    if id == RecId::UrgentBugRisk {
        0
    } else {
        1
    }
}

/// Builds an item's `bug_evidence` in the module's fixed order (critical-findings, fix-ratio, hotspot) —
/// see this module's doc for why only the first of the three ever triggers escalation. `node` is `None`
/// for the rare item whose path has no matching `FileNode` (e.g. a circular-dep cycle head that fell out
/// of `nodes`), in which case only critical-finding evidence can apply.
pub(super) fn bug_evidence_for(
    path: &str,
    node: Option<&FileNode>,
    critical_by_path: &HashMap<&str, Vec<&Finding>>,
) -> Vec<String> {
    let mut evidence = Vec::new();

    if let Some(findings) = critical_by_path.get(path) {
        let mut rule_ids: Vec<&str> = findings.iter().map(|f| f.rule_id.as_str()).collect();
        rule_ids.sort_unstable();
        rule_ids.dedup();
        evidence.push(format!(
            "{} critical finding(s) in this file: {}",
            findings.len(),
            rule_ids.join(", ")
        ));
    }

    if let Some(n) = node {
        let fix = tag_count(n, "FIX");
        if n.change_count >= FIX_RATIO_MIN_CHANGE_COUNT
            && fix as f64 / n.change_count as f64 >= FIX_RATIO_THRESHOLD
        {
            evidence.push(format!(
                "{fix} of {} changes are bug-fix commits",
                n.change_count
            ));
        }

        let hotspot = n.hotspot_score.unwrap_or(0.0);
        if hotspot > 0.0 && n.fan_in >= HOTSPOT_BLAST_FAN_IN {
            evidence.push(format!(
                "frequently changed and imported by {} files",
                n.fan_in
            ));
        }
    }

    evidence
}
