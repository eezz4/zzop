//! The recommendation rules — each maps `FileNode`s / coupling / cycles to `RawItem`s under a rule
//! id and severity, gated by `RecommendationGates` and capped per rule.

use std::cmp::Ordering;
use std::collections::HashSet;

use regex::Regex;

use crate::coupling::CouplingMap;
use crate::roi::RecId;
use zzop_core::{DepGraph, FileNode, Lifecycle, Severity};

use super::types::{RawItem, RecommendationGates};

// --- constants ---

const MAX_BUG_PRONE: usize = 20;
const MAX_CIRCULAR: usize = 15;
const MAX_HOT_CHURN: usize = 15;
const MAX_FAT_FANOUT: usize = 15;
const MAX_HIDDEN_COUPLING: usize = 15;
const MAX_KNOWLEDGE_SILO: usize = 10;
const MAX_VERSIONING_CANDIDATE: usize = 10;

pub(super) fn tag_count(n: &FileNode, tag: &str) -> u32 {
    n.tag_counts.get(tag).copied().unwrap_or(0)
}

pub(super) fn rule_bug_prone(
    nodes: &[FileNode],
    g: &RecommendationGates,
) -> Vec<(RecId, Severity, Vec<RawItem>)> {
    let mut filtered: Vec<&FileNode> = nodes
        .iter()
        .filter(|n| tag_count(n, "FIX") >= g.bug_prone_fix)
        .collect();
    filtered.sort_by_key(|n| std::cmp::Reverse(tag_count(n, "FIX")));
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
        vec![(RecId::BugProne, Severity::Critical, items)]
    }
}

pub(super) fn rule_circular(circular: &[Vec<String>]) -> Vec<(RecId, Severity, Vec<RawItem>)> {
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
    vec![(RecId::Circular, Severity::Critical, items)]
}

pub(super) fn rule_high_churn_per_loc(
    nodes: &[FileNode],
    g: &RecommendationGates,
) -> Vec<(RecId, Severity, Vec<RawItem>)> {
    let ratio = |n: &FileNode| n.churn as f64 / n.loc as f64;
    let mut filtered: Vec<&FileNode> = nodes
        .iter()
        .filter(|n| n.loc > g.hot_churn_min_loc && ratio(n) > g.hot_churn_ratio)
        .collect();
    filtered.sort_by(|a, b| ratio(b).partial_cmp(&ratio(a)).unwrap_or(Ordering::Equal));
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
        vec![(RecId::HotChurn, Severity::Warning, items)]
    }
}

pub(super) fn rule_fat_fan_out(
    nodes: &[FileNode],
    g: &RecommendationGates,
) -> Vec<(RecId, Severity, Vec<RawItem>)> {
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
        vec![(RecId::FatFanout, Severity::Warning, items)]
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
) -> Vec<(RecId, Severity, Vec<RawItem>)> {
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
    items.truncate(MAX_HIDDEN_COUPLING);
    vec![(RecId::HiddenCoupling, Severity::Warning, items)]
}

pub(super) fn rule_knowledge_silo(
    nodes: &[FileNode],
    g: &RecommendationGates,
) -> Vec<(RecId, Severity, Vec<RawItem>)> {
    let mut filtered: Vec<&FileNode> = nodes
        .iter()
        .filter(|n| n.author_count >= g.knowledge_silo_authors)
        .collect();
    filtered.sort_by_key(|n| std::cmp::Reverse(n.author_count));
    filtered.truncate(MAX_KNOWLEDGE_SILO);
    let items: Vec<RawItem> = filtered
        .into_iter()
        .map(|n| RawItem {
            path: n.path.clone(),
            note: Some(format!("authors {}", n.author_count)),
        })
        .collect();
    if items.is_empty() {
        vec![]
    } else {
        vec![(RecId::KnowledgeSilo, Severity::Info, items)]
    }
}

/// volatile + many callers + repeated FIX -> in-place refactor hits legacy users; suggest parallel V2.
pub(super) fn rule_versioning_candidate(
    nodes: &[FileNode],
    g: &RecommendationGates,
) -> Vec<(RecId, Severity, Vec<RawItem>)> {
    let mut filtered: Vec<&FileNode> = nodes
        .iter()
        .filter(|n| {
            n.lifecycle == Some(Lifecycle::Volatile)
                && n.fan_in >= g.versioning_fan_in
                && tag_count(n, "FIX") >= g.versioning_fix
        })
        .collect();
    filtered.sort_by_key(|n| std::cmp::Reverse(tag_count(n, "FIX")));
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
        vec![(RecId::VersioningCandidate, Severity::Warning, items)]
    }
}
