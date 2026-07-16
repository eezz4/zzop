//! Generates improvement recommendations from `FileNode`s, coupling, and circular deps.
//!
//! Every rule is evaluated; each item gets ROI, cost, and an `ActionHintKey`. `scope_excludes`
//! (rule + glob) and `permanent_ignores` (rule + path) are applied as post-filters. Results are
//! sorted by severity (critical -> warning -> info), then ROI desc within the same severity.
//! `deriveActionHintKey` is folded in below since it is tiny and has no other callers.
//!
//! Rule-gate thresholds are passed explicitly via `RecommendationGates` rather than through an
//! ambient config singleton (see `crate::scores::config` for the precedent).
//!
//! ## Bug evidence + severity escalation (not ROI inflation)
//! Every item can carry `bug_evidence`: deterministic strings naming WHY the underlying file is
//! bug-risky, built from data already in scope (`enrich`'s `critical_by_path` lookup, `FileNode::tag_counts`,
//! `FileNode::hotspot_score`/`fan_in`). The ROI number's meaning stays pure — reduction / cost — so
//! evidence NEVER feeds into `compute_roi`; that would make the score a misleading composite, which is a
//! product-defect class in this codebase (see the task's design note). Instead, an item whose evidence
//! includes a critical-`Finding` hit is physically MOVED (never copied) into a new synthetic
//! `RecId::UrgentBugRisk` group with `Severity::Critical`, which the existing severity-first sort then
//! carries to the top.
//!
//! **Why only critical-`Finding` evidence escalates, and FIX-ratio/hotspot evidence never does**: a
//! `Finding` is rule-confirmed — a specific rule fired on that exact file with `Severity::Critical`, the
//! same trust level this module's own `bug-prone`/`circular` rules already carry. FIX-ratio and hotspot
//! evidence are both *inferred* correlations (churn/authorship signals), not confirmations — promoting a
//! file to the top group on inference alone would make the escalation channel less trustworthy than the
//! findings channel that feeds it, which defeats the point of a "these are worth escalating" signal. So
//! FIX-ratio/hotspot evidence rides along on the item wherever it already sorted — advisory only.

use std::cmp::Ordering;
use std::collections::HashMap;

use crate::roi::RecId;
use zzop_core::{FileNode, Severity};

mod enrich;
mod evidence;
mod rules;
mod types;

#[cfg(test)]
mod tests;

pub use self::types::{ActionHintKey, BuildRecInput, RecItem, Recommendation, RecommendationGates};

use self::enrich::{enrich, is_filtered};
use self::evidence::{bug_evidence_for, critical_findings_by_path, escalate_critical_bug_evidence};
use self::rules::{
    rule_bug_prone, rule_circular, rule_fat_fan_out, rule_hidden_coupling, rule_high_churn_per_loc,
    rule_knowledge_silo, rule_versioning_candidate,
};
use self::types::RawItem;

pub fn build_recommendations(
    input: &BuildRecInput,
    gates: &RecommendationGates,
) -> Vec<Recommendation> {
    let nodes_by_path: HashMap<&str, &FileNode> =
        input.nodes.iter().map(|n| (n.path.as_str(), n)).collect();
    let critical_by_path = critical_findings_by_path(input.findings);

    let mut raw: Vec<(RecId, Severity, Vec<RawItem>)> = Vec::new();
    raw.extend(rule_bug_prone(input.nodes, gates));
    raw.extend(rule_circular(input.circular));
    raw.extend(rule_high_churn_per_loc(input.nodes, gates));
    raw.extend(rule_fat_fan_out(input.nodes, gates));
    raw.extend(rule_hidden_coupling(input.coupling, input.dep, gates));
    raw.extend(rule_knowledge_silo(input.nodes, gates));
    raw.extend(rule_versioning_candidate(input.nodes, gates));

    let mut recs: Vec<Recommendation> = Vec::new();
    for (rule_id, severity, items) in raw {
        let filtered: Vec<RawItem> = items
            .into_iter()
            .filter(|it| !is_filtered(rule_id, &it.path, input))
            .collect();
        if filtered.is_empty() {
            continue;
        }
        let mut enriched: Vec<RecItem> = filtered
            .into_iter()
            .map(|it| {
                let node = nodes_by_path.get(it.path.as_str()).copied();
                let untested = input.untested_paths.contains(&it.path);
                let amplification = input
                    .amplification_by_path
                    .get(&it.path)
                    .copied()
                    .unwrap_or(0.0);
                let bug_evidence = bug_evidence_for(&it.path, node, &critical_by_path);
                enrich(
                    rule_id,
                    severity,
                    it,
                    node,
                    untested,
                    amplification,
                    bug_evidence,
                )
            })
            .collect();
        enriched.sort_by(|a, b| b.roi.partial_cmp(&a.roi).unwrap_or(Ordering::Equal));
        recs.push(Recommendation {
            id: rule_id,
            severity,
            items: enriched,
        });
    }
    escalate_critical_bug_evidence(recs, &critical_by_path)
}
