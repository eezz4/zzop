//! The recommendation rules — each maps `FileNode`s / coupling / cycles to `RawItem`s under a rule
//! id and severity, gated by `RecommendationGates` and capped per rule.
//!
//! # Every cap here says what it dropped
//! The six caps below were bare `Vec::truncate` / `Iterator::take` calls until 2026-09-04 — which is
//! the defect [`crate::scores::detail_cap`] names in as many words (*"That is a SILENT cap"*) and had
//! already repaired for the ten `scores/*` modules. That module's doc then ENUMERATES the lanes that do
//! disclose — `findings`, `suggestionsTruncated`/`edgesTruncated`/`degradedTruncated`, the graph `%%`
//! census, `scores/*` — and this lane was not among them. So one reply shipped two honesty contracts at
//! once: `godFile.files` came back with `filesTruncated` beside it while `recommendations[].items` came
//! back with nothing, and "these are the 15 cycles" and "these are 15 of four hundred" were the same
//! bytes. Found by a census of user-approved decisions against this repo's own disclosure philosophy.
//!
//! Each rule now returns [`RuleOutput`], carrying the count to `Recommendation::items_truncated`.
//!
//! # What the number means, and what it deliberately does not
//! It is taken AT THE CAP, which is both what makes it honest and what bounds it: it counts *rows this
//! rule's cap dropped*, never *rows missing from the reply*. Two later stages narrow the list further
//! and are not folded in — `is_filtered` drops config-excluded paths (so an excluded path can still
//! consume a cap slot, a pre-existing ordering this change does not touch), and
//! `escalate_critical_bug_evidence` MOVES items between groups rather than dropping them. One scalar
//! cannot mean all three, and the cap's own count is the one a reader cannot reconstruct from the
//! reply: the other two leave their evidence in `config.exclude` and in the escalated item's
//! `escalatedFrom`.

use std::cmp::Ordering;
use std::collections::HashSet;

use regex::Regex;

use crate::coupling::CouplingMap;
use crate::roi::RecId;
use zzop_core::{DepGraph, FileNode, Lifecycle, Severity};

use super::types::{RawItem, RecommendationGates, RuleOutput};

// --- constants ---

const MAX_BUG_PRONE: usize = 20;
const MAX_CIRCULAR: usize = 15;
const MAX_HOT_CHURN: usize = 15;
const MAX_FAT_FANOUT: usize = 15;
const MAX_HIDDEN_COUPLING: usize = 15;
const MAX_VERSIONING_CANDIDATE: usize = 10;

/// Rows a cap is about to drop, counted before the truncation that makes them uncountable.
///
/// A free function rather than [`crate::scores::detail_cap::cap_and_count_dropped`] because the six
/// call sites do not share ONE shape: four truncate a `Vec` in place, one caps an iterator with `take`
/// (nothing is ever in a `Vec` to truncate), and one truncates a `Vec` it built itself. The scores lane
/// helper owns the truncation AND the count together, which is right there — every one of its call
/// sites is `cap_and_count_dropped(&mut v, N)`. Forcing the `take` site into that shape would mean
/// materializing the full list only to throw it away, so this lane shares the CONVENTION (a count
/// beside the list, always serialized) rather than the function.
fn drop_count(before_cap: usize, cap: usize) -> u32 {
    // Saturating, like the scores lane's: an absurd input must not report a SMALL remainder, which
    // would be the same lie in a new costume.
    u32::try_from(before_cap.saturating_sub(cap)).unwrap_or(u32::MAX)
}

pub(super) fn tag_count(n: &FileNode, tag: &str) -> u32 {
    n.tag_counts.get(tag).copied().unwrap_or(0)
}

pub(super) fn rule_bug_prone(nodes: &[FileNode], g: &RecommendationGates) -> Vec<RuleOutput> {
    let mut filtered: Vec<&FileNode> = nodes
        .iter()
        .filter(|n| tag_count(n, "FIX") >= g.bug_prone_fix)
        .collect();
    filtered.sort_by_key(|n| std::cmp::Reverse(tag_count(n, "FIX")));
    let truncated = drop_count(filtered.len(), MAX_BUG_PRONE);
    filtered.truncate(MAX_BUG_PRONE);
    let items: Vec<RawItem> = filtered
        .into_iter()
        .map(|n| RawItem {
            path: n.path.clone(),
            note: Some(format!(
                "FIX {} · risk {:.0}",
                tag_count(n, "FIX"),
                n.risk_score
            )),
        })
        .collect();
    if items.is_empty() {
        vec![]
    } else {
        vec![RuleOutput::new(
            RecId::BugProne,
            Severity::Critical,
            items,
            truncated,
        )]
    }
}

pub(super) fn rule_circular(circular: &[Vec<String>]) -> Vec<RuleOutput> {
    if circular.is_empty() {
        return vec![];
    }
    let items: Vec<RawItem> = circular
        .iter()
        .take(MAX_CIRCULAR)
        .filter_map(|cycle| {
            let head = cycle.first()?;
            let note = format!("{} → {}", cycle.join(" → "), head);
            Some(RawItem {
                path: head.clone(),
                note: Some(note),
            })
        })
        .collect();
    // Counted off the INPUT, not off `items`: the `take` runs before `filter_map`, so a cycle the
    // cap dropped never reaches the closure that could have counted it.
    vec![RuleOutput::new(
        RecId::Circular,
        Severity::Critical,
        items,
        drop_count(circular.len(), MAX_CIRCULAR),
    )]
}

pub(super) fn rule_high_churn_per_loc(
    nodes: &[FileNode],
    g: &RecommendationGates,
) -> Vec<RuleOutput> {
    let ratio = |n: &FileNode| n.churn as f64 / n.loc as f64;
    let mut filtered: Vec<&FileNode> = nodes
        .iter()
        .filter(|n| n.loc > g.hot_churn_min_loc && ratio(n) > g.hot_churn_ratio)
        .collect();
    filtered.sort_by(|a, b| ratio(b).partial_cmp(&ratio(a)).unwrap_or(Ordering::Equal));
    let truncated = drop_count(filtered.len(), MAX_HOT_CHURN);
    filtered.truncate(MAX_HOT_CHURN);
    let items: Vec<RawItem> = filtered
        .into_iter()
        .map(|n| RawItem {
            path: n.path.clone(),
            note: Some(format!("churn/LOC {:.1} (loc {})", ratio(n), n.loc)),
        })
        .collect();
    if items.is_empty() {
        vec![]
    } else {
        vec![RuleOutput::new(
            RecId::HotChurn,
            Severity::Warning,
            items,
            truncated,
        )]
    }
}

pub(super) fn rule_fat_fan_out(nodes: &[FileNode], g: &RecommendationGates) -> Vec<RuleOutput> {
    // Barrel/Page/App.tsx assembly points naturally have high fanOut — exclude from warnings.
    // Public API violations are covered separately by the publicApi score.
    let barrel_re = Regex::new(r"(?:^|/)index\.(?:ts|tsx|js|jsx|mjs|cjs)$").unwrap();
    let orchestrator_re =
        Regex::new(r"(?:^|/)App\.tsx$|(?:^|/)pages/.*Page\.tsx$|(?:^|/)apiRoutes\.ts$").unwrap();
    let mut filtered: Vec<&FileNode> = nodes
        .iter()
        .filter(|n| {
            n.fan_out >= g.fat_fan_out
                && !barrel_re.is_match(&n.path)
                && !is_reexport_barrel(n, g)
                && !orchestrator_re.is_match(&n.path)
        })
        .collect();
    filtered.sort_by_key(|n| std::cmp::Reverse(n.fan_out));
    let truncated = drop_count(filtered.len(), MAX_FAT_FANOUT);
    filtered.truncate(MAX_FAT_FANOUT);
    let items: Vec<RawItem> = filtered
        .into_iter()
        .map(|n| RawItem {
            path: n.path.clone(),
            note: Some(format!("fan_out {}", n.fan_out)),
        })
        .collect();
    if items.is_empty() {
        vec![]
    } else {
        vec![RuleOutput::new(
            RecId::FatFanout,
            Severity::Warning,
            items,
            truncated,
        )]
    }
}

/// Structural (filename-agnostic) barrel: most of the file's lines are imports, so fanOut ~= LOC.
/// Catches CommonJS aggregators (e.g. a module entry that is only `Pkg.X = require('...')` lines)
/// that no filename pattern would match.
fn is_reexport_barrel(n: &FileNode, g: &RecommendationGates) -> bool {
    n.loc > 0 && (n.fan_out as f64 / n.loc as f64) >= g.barrel_fan_out_ratio
}

pub(super) fn rule_hidden_coupling(
    coupling: &CouplingMap,
    dep: &DepGraph,
    g: &RecommendationGates,
) -> Vec<RuleOutput> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut items: Vec<RawItem> = Vec::new();
    for (a, partners) in coupling {
        for entry in partners {
            let b = &entry.path;
            if a.as_str() >= b.as_str() || entry.count < g.hidden_coupling_min {
                continue;
            }
            let key = format!("{a}|{b}");
            if !seen.insert(key) {
                continue;
            }
            let a_imports_b = dep.get(a).is_some_and(|v| v.iter().any(|x| x == b));
            let b_imports_a = dep.get(b).is_some_and(|v| v.iter().any(|x| x == a));
            if a_imports_b || b_imports_a {
                continue;
            }
            items.push(RawItem {
                path: a.clone(),
                note: Some(format!("{}x ↔ {}", entry.count, b)),
            });
        }
    }
    if items.is_empty() {
        return vec![];
    }
    let truncated = drop_count(items.len(), MAX_HIDDEN_COUPLING);
    items.truncate(MAX_HIDDEN_COUPLING);
    vec![RuleOutput::new(
        RecId::HiddenCoupling,
        Severity::Warning,
        items,
        truncated,
    )]
}

/// volatile + many callers + repeated FIX -> in-place refactor hits legacy users; suggest parallel V2.
pub(super) fn rule_versioning_candidate(
    nodes: &[FileNode],
    g: &RecommendationGates,
) -> Vec<RuleOutput> {
    let mut filtered: Vec<&FileNode> = nodes
        .iter()
        .filter(|n| {
            n.lifecycle == Some(Lifecycle::Volatile)
                && n.fan_in >= g.versioning_fan_in
                && tag_count(n, "FIX") >= g.versioning_fix
        })
        .collect();
    filtered.sort_by_key(|n| std::cmp::Reverse(tag_count(n, "FIX")));
    let truncated = drop_count(filtered.len(), MAX_VERSIONING_CANDIDATE);
    filtered.truncate(MAX_VERSIONING_CANDIDATE);
    let items: Vec<RawItem> = filtered
        .into_iter()
        .map(|n| RawItem {
            path: n.path.clone(),
            note: Some(format!(
                "volatile · fan_in {} · FIX {}",
                n.fan_in,
                tag_count(n, "FIX")
            )),
        })
        .collect();
    if items.is_empty() {
        vec![]
    } else {
        vec![RuleOutput::new(
            RecId::VersioningCandidate,
            Severity::Warning,
            items,
            truncated,
        )]
    }
}
