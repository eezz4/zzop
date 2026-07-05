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
use std::collections::{HashMap, HashSet};

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::coupling::CouplingMap;
use crate::roi::{compute_roi, RecId};
use zpz_core::{DepGraph, FileNode, Finding, Lifecycle, Severity};

/// `changeCount` floor below which a FIX-tag ratio is too small a sample to be meaningful evidence.
const FIX_RATIO_MIN_CHANGE_COUNT: u32 = 5;
/// FIX / changeCount ratio at/above which "N of M changes are bug-fix commits" is surfaced as evidence.
const FIX_RATIO_THRESHOLD: f64 = 0.5;
/// fanIn at/above which a hotspot file's blast radius makes "frequently changed and imported by N files"
/// worth surfacing as evidence.
const HOTSPOT_BLAST_FAN_IN: u32 = 5;

// --- constants ---

/// Sort rank for the `info` severity (critical = 0, warning = 1; lower = more severe).
const SEVERITY_RANK_INFO: u8 = 2;

const MAX_BUG_PRONE: usize = 20;
const MAX_CIRCULAR: usize = 15;
const MAX_HOT_CHURN: usize = 15;
const MAX_FAT_FANOUT: usize = 15;
const MAX_HIDDEN_COUPLING: usize = 15;
const MAX_KNOWLEDGE_SILO: usize = 10;
const MAX_VERSIONING_CANDIDATE: usize = 10;

/// LOC boundary between `fat-fanout-small` and `-large`.
const FAT_FANOUT_LOC: u32 = 100;
/// fanIn at/above which a bug-prone file is "shared".
const BUG_PRONE_SHARED_FANIN: u32 = 3;
/// fanIn at/above which a hot-churn file is "core".
const HOT_CHURN_CORE_FANIN: u32 = 5;

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

/// Inputs to `build_recommendations`. `scope_excludes`, `permanent_ignores`, `untested_paths`, and
/// `amplification_by_path` are all optional in practice; callers that have nothing to pass pass
/// empty collections rather than `Option`.
pub struct BuildRecInput<'a> {
    pub nodes: &'a [FileNode],
    pub dep: &'a DepGraph,
    pub coupling: &'a CouplingMap,
    pub circular: &'a [Vec<String>],
    /// rule + glob scope exclusions (e.g. hidden-coupling x core/i18n/**).
    pub scope_excludes: &'a [(RecId, String)],
    /// permanently ignored (ruleId, path) pairs.
    pub permanent_ignores: &'a [(RecId, String)],
    /// Paths with no test — their ROI cost is multiplied (safely changing untested code costs more).
    pub untested_paths: &'a HashSet<String>,
    /// path -> change-amplification (effective co-changing file count); raises ROI cost for ripple epicenters.
    pub amplification_by_path: &'a HashMap<String, f64>,
    /// Whole-tree findings — sole source of an item's critical-finding bug evidence (and the sole
    /// escalation trigger; see this module's doc). Not filtered to any particular rule id: any
    /// `Severity::Critical` finding on the item's path counts.
    pub findings: &'a [Finding],
}

/// A rule hit before ROI enrichment.
struct RawItem {
    path: String,
    note: Option<String>,
}

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

/// path -> critical `Finding`s on that path — the sole substrate for both critical-finding evidence
/// text and the escalation decision (see this module's doc). Built once per `build_recommendations`
/// call rather than per-item, since the same path can be looked up by multiple rules' items.
fn critical_findings_by_path(findings: &[Finding]) -> HashMap<&str, Vec<&Finding>> {
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
fn escalate_critical_bug_evidence(
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

fn is_filtered(rule_id: RecId, path: &str, input: &BuildRecInput) -> bool {
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
fn enrich(
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

/// Builds an item's `bug_evidence` in the module's fixed order (critical-findings, fix-ratio, hotspot) —
/// see this module's doc for why only the first of the three ever triggers escalation. `node` is `None`
/// for the rare item whose path has no matching `FileNode` (e.g. a circular-dep cycle head that fell out
/// of `nodes`), in which case only critical-finding evidence can apply.
fn bug_evidence_for(
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
fn matches_glob(path: &str, glob: &str) -> bool {
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

fn tag_count(n: &FileNode, tag: &str) -> u32 {
    n.tag_counts.get(tag).copied().unwrap_or(0)
}

fn rule_bug_prone(
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

fn rule_circular(circular: &[Vec<String>]) -> Vec<(RecId, Severity, Vec<RawItem>)> {
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

fn rule_high_churn_per_loc(
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

fn rule_fat_fan_out(
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

fn rule_hidden_coupling(
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

fn rule_knowledge_silo(
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
fn rule_versioning_candidate(
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

#[cfg(test)]
mod tests {
    //! Covers rule selection and ordering (every applicable rule included, sorted by severity then
    //! ROI desc), the per-rule filters (fat-fanout barrel/re-export/orchestrator exclusions, scope
    //! excludes, permanent ignores), the glob matcher, hidden-coupling pair dedup, and cost/ROI
    //! adjustments from untested paths and amplification. `deriveActionHintKey`'s branches are
    //! exercised indirectly here via `action_hint_key` assertions since it has no separate call site
    //! in this crate.
    use super::*;

    fn node(path: &str) -> FileNode {
        FileNode {
            id: path.to_string(),
            path: path.to_string(),
            change_count: 0,
            churn: 0,
            last_modified: None,
            author_count: 1,
            loc: 50,
            tag_counts: HashMap::new(),
            fan_in: 0,
            fan_out: 0,
            total_connections: 0,
            risk_score: 50.0,
            ..Default::default()
        }
    }

    fn tags(fix: u32) -> HashMap<String, u32> {
        let mut m = HashMap::new();
        m.insert("FIX".to_string(), fix);
        m
    }

    fn empty_input<'a>(
        nodes: &'a [FileNode],
        dep: &'a DepGraph,
        coupling: &'a CouplingMap,
    ) -> BuildRecInput<'a> {
        BuildRecInput {
            nodes,
            dep,
            coupling,
            circular: &[],
            scope_excludes: &[],
            permanent_ignores: &[],
            untested_paths: empty_set(),
            amplification_by_path: empty_map(),
            findings: &[],
        }
    }

    fn critical_finding(path: &str, rule_id: &str) -> Finding {
        Finding {
            rule_id: rule_id.to_string(),
            severity: Severity::Critical,
            file: path.to_string(),
            line: 1,
            message: "test fixture critical finding".to_string(),
            data: None,
        }
    }

    // Static empty collections shared by tests that don't exercise untested/amplification inputs.
    // std has no `const fn` HashSet::new usable in a `static`, so build lazily via `OnceLock`.
    static EMPTY_SET: std::sync::OnceLock<HashSet<String>> = std::sync::OnceLock::new();
    static EMPTY_MAP: std::sync::OnceLock<HashMap<String, f64>> = std::sync::OnceLock::new();

    fn empty_set() -> &'static HashSet<String> {
        EMPTY_SET.get_or_init(HashSet::new)
    }
    fn empty_map() -> &'static HashMap<String, f64> {
        EMPTY_MAP.get_or_init(HashMap::new)
    }

    #[test]
    fn every_applicable_rule_is_included_no_persona_filtering() {
        let nodes = [
            FileNode {
                tag_counts: tags(6),
                risk_score: 100.0,
                ..node("bug.ts")
            },
            FileNode {
                fan_out: 10,
                risk_score: 80.0,
                ..node("fat.ts")
            },
        ];
        let dep = DepGraph::new();
        let coupling = CouplingMap::new();
        let input = BuildRecInput {
            nodes: &nodes,
            dep: &dep,
            coupling: &coupling,
            circular: &[],
            scope_excludes: &[],
            permanent_ignores: &[],
            untested_paths: empty_set(),
            amplification_by_path: empty_map(),
            findings: &[],
        };
        let recs = build_recommendations(&input, &RecommendationGates::default());
        assert!(recs.iter().any(|r| r.id == RecId::BugProne));
        assert!(recs.iter().any(|r| r.id == RecId::FatFanout));
    }

    #[test]
    fn sorted_descending_by_roi_within_the_same_rule() {
        let nodes = [
            FileNode {
                tag_counts: tags(10),
                risk_score: 200.0,
                loc: 40,
                fan_in: 1,
                ..node("hi.ts")
            },
            FileNode {
                tag_counts: tags(6),
                risk_score: 40.0,
                loc: 200,
                fan_in: 10,
                ..node("lo.ts")
            },
        ];
        let dep = DepGraph::new();
        let coupling = CouplingMap::new();
        let input = BuildRecInput {
            nodes: &nodes,
            dep: &dep,
            coupling: &coupling,
            circular: &[],
            scope_excludes: &[],
            permanent_ignores: &[],
            untested_paths: empty_set(),
            amplification_by_path: empty_map(),
            findings: &[],
        };
        let recs = build_recommendations(&input, &RecommendationGates::default());
        let bug = &recs[0];
        assert_eq!(bug.items[0].path, "hi.ts");
        assert!(bug.items[0].roi > bug.items[1].roi);
    }

    #[test]
    fn each_item_carries_roi_estimated_reduction_estimated_cost_action_hint_key_fan_in() {
        let nodes = [FileNode {
            fan_out: 10,
            loc: 50,
            risk_score: 60.0,
            fan_in: 4,
            ..node("fat.ts")
        }];
        let dep = DepGraph::new();
        let coupling = CouplingMap::new();
        let input = empty_input(&nodes, &dep, &coupling);
        let recs = build_recommendations(&input, &RecommendationGates::default());
        let fat = &recs[0];
        assert_eq!(fat.items[0].path, "fat.ts");
        assert!(fat.items[0].roi.is_finite());
        assert!(fat.items[0].estimated_reduction.is_finite());
        assert!(fat.items[0].estimated_cost.is_finite());
        assert_eq!(fat.items[0].action_hint_key, ActionHintKey::FatFanoutSmall);
        assert_eq!(fat.items[0].fan_in, 4);
    }

    #[test]
    fn fat_fanout_auto_excludes_barrel_files() {
        let nodes = [
            FileNode {
                fan_out: 20,
                ..node("src/index.ts")
            },
            FileNode {
                fan_out: 20,
                ..node("features/some/index.tsx")
            },
            FileNode {
                fan_out: 20,
                ..node("RealFat.ts")
            },
        ];
        let dep = DepGraph::new();
        let coupling = CouplingMap::new();
        let input = empty_input(&nodes, &dep, &coupling);
        let recs = build_recommendations(&input, &RecommendationGates::default());
        let fat = recs.iter().find(|r| r.id == RecId::FatFanout).unwrap();
        let paths: Vec<&str> = fat.items.iter().map(|i| i.path.as_str()).collect();
        assert_eq!(paths, vec!["RealFat.ts"]);
    }

    #[test]
    fn fat_fanout_excludes_structural_reexport_barrels() {
        let nodes = [
            // 0.83 — barrel of `Pkg.X = require(...)` lines
            FileNode {
                fan_out: 30,
                loc: 36,
                ..node("module/main.js")
            },
            // 0.016 — real dispatcher with logic
            FileNode {
                fan_out: 9,
                loc: 574,
                ..node("core/Engine.js")
            },
        ];
        let dep = DepGraph::new();
        let coupling = CouplingMap::new();
        let input = empty_input(&nodes, &dep, &coupling);
        let recs = build_recommendations(&input, &RecommendationGates::default());
        let fat = recs.iter().find(|r| r.id == RecId::FatFanout).unwrap();
        let paths: Vec<&str> = fat.items.iter().map(|i| i.path.as_str()).collect();
        assert_eq!(paths, vec!["core/Engine.js"]);
    }

    #[test]
    fn fat_fanout_excludes_orchestrators() {
        let nodes = [
            FileNode {
                fan_out: 15,
                ..node("App.tsx")
            },
            FileNode {
                fan_out: 15,
                ..node("pages/recommendation/RecommendationPage.tsx")
            },
            FileNode {
                fan_out: 15,
                ..node("api/apiRoutes.ts")
            },
            FileNode {
                fan_out: 15,
                ..node("features/evidence/Real.tsx")
            },
        ];
        let dep = DepGraph::new();
        let coupling = CouplingMap::new();
        let input = empty_input(&nodes, &dep, &coupling);
        let recs = build_recommendations(&input, &RecommendationGates::default());
        let fat = recs.iter().find(|r| r.id == RecId::FatFanout).unwrap();
        let paths: Vec<&str> = fat.items.iter().map(|i| i.path.as_str()).collect();
        assert_eq!(paths, vec!["features/evidence/Real.tsx"]);
    }

    #[test]
    fn fat_fanout_loc_branch_small_vs_large() {
        let nodes = [
            FileNode {
                fan_out: 10,
                loc: 50,
                ..node("small.ts")
            },
            FileNode {
                fan_out: 10,
                loc: 200,
                ..node("large.ts")
            },
        ];
        let dep = DepGraph::new();
        let coupling = CouplingMap::new();
        let input = empty_input(&nodes, &dep, &coupling);
        let recs = build_recommendations(&input, &RecommendationGates::default());
        let fat = recs.iter().find(|r| r.id == RecId::FatFanout).unwrap();
        let small = fat.items.iter().find(|i| i.path == "small.ts").unwrap();
        let large = fat.items.iter().find(|i| i.path == "large.ts").unwrap();
        assert_eq!(small.action_hint_key, ActionHintKey::FatFanoutSmall);
        assert_eq!(large.action_hint_key, ActionHintKey::FatFanoutLarge);
    }

    #[test]
    fn scope_excludes_filters_by_rule_id_and_glob() {
        let nodes = [
            FileNode {
                fan_out: 10,
                ..node("core/i18n/en.ts")
            },
            FileNode {
                fan_out: 10,
                ..node("src/HotFile.ts")
            },
        ];
        let dep = DepGraph::new();
        let coupling = CouplingMap::new();
        let scope_excludes = [(RecId::FatFanout, "core/i18n/**".to_string())];
        let input = BuildRecInput {
            nodes: &nodes,
            dep: &dep,
            coupling: &coupling,
            circular: &[],
            scope_excludes: &scope_excludes,
            permanent_ignores: &[],
            untested_paths: empty_set(),
            amplification_by_path: empty_map(),
            findings: &[],
        };
        let recs = build_recommendations(&input, &RecommendationGates::default());
        let fat = recs.iter().find(|r| r.id == RecId::FatFanout).unwrap();
        let paths: Vec<&str> = fat.items.iter().map(|i| i.path.as_str()).collect();
        assert_eq!(paths, vec!["src/HotFile.ts"]);
    }

    #[test]
    fn permanent_ignores_removes_rule_id_path_pairs() {
        let nodes = [
            FileNode {
                fan_out: 10,
                ..node("A.ts")
            },
            FileNode {
                fan_out: 10,
                ..node("B.ts")
            },
        ];
        let dep = DepGraph::new();
        let coupling = CouplingMap::new();
        let permanent_ignores = [(RecId::FatFanout, "A.ts".to_string())];
        let input = BuildRecInput {
            nodes: &nodes,
            dep: &dep,
            coupling: &coupling,
            circular: &[],
            scope_excludes: &[],
            permanent_ignores: &permanent_ignores,
            untested_paths: empty_set(),
            amplification_by_path: empty_map(),
            findings: &[],
        };
        let recs = build_recommendations(&input, &RecommendationGates::default());
        let fat = recs.iter().find(|r| r.id == RecId::FatFanout).unwrap();
        let paths: Vec<&str> = fat.items.iter().map(|i| i.path.as_str()).collect();
        assert_eq!(paths, vec!["B.ts"]);
    }

    #[test]
    fn severity_order_critical_then_warning_then_info() {
        let nodes = [
            FileNode {
                tag_counts: tags(6),
                risk_score: 100.0,
                ..node("bug.ts")
            },
            FileNode {
                fan_out: 10,
                ..node("fat.ts")
            },
            FileNode {
                author_count: 7,
                ..node("silo.ts")
            },
        ];
        let dep = DepGraph::new();
        let coupling = CouplingMap::new();
        let input = empty_input(&nodes, &dep, &coupling);
        let recs = build_recommendations(&input, &RecommendationGates::default());
        let sevs: Vec<Severity> = recs.iter().map(|r| r.severity).collect();
        let idx_of = |s: Severity| sevs.iter().position(|&x| x == s).unwrap();
        assert!(idx_of(Severity::Critical) < idx_of(Severity::Warning));
        assert!(idx_of(Severity::Warning) < idx_of(Severity::Info));
    }

    // --- glob matcher ---

    #[test]
    fn glob_double_star_matches_any_depth() {
        assert!(matches_glob("core/i18n/nested/en.ts", "core/i18n/**"));
        assert!(matches_glob("core/i18n/en.ts", "core/i18n/**"));
        assert!(!matches_glob("core/other/en.ts", "core/i18n/**"));
    }

    #[test]
    fn glob_single_star_does_not_cross_slash() {
        assert!(matches_glob("src/Foo.ts", "src/*.ts"));
        assert!(!matches_glob("src/nested/Foo.ts", "src/*.ts"));
    }

    #[test]
    fn glob_escapes_regex_special_characters() {
        assert!(matches_glob("a.b.ts", "a.b.ts"));
        assert!(!matches_glob("aXb.ts", "a.b.ts")); // literal '.', not "any char"
    }

    // --- hidden coupling dedup / rule gates ---

    #[test]
    fn hidden_coupling_dedups_symmetric_pairs_and_skips_importers() {
        let nodes: [FileNode; 0] = [];
        let dep = DepGraph::new();
        let mut coupling = CouplingMap::new();
        coupling.insert(
            "a.ts".to_string(),
            vec![crate::coupling::CouplingEntry {
                path: "b.ts".to_string(),
                count: 12,
            }],
        );
        coupling.insert(
            "b.ts".to_string(),
            vec![crate::coupling::CouplingEntry {
                path: "a.ts".to_string(),
                count: 12,
            }],
        );
        let input = empty_input(&nodes, &dep, &coupling);
        let recs = build_recommendations(&input, &RecommendationGates::default());
        let hidden = recs.iter().find(|r| r.id == RecId::HiddenCoupling).unwrap();
        assert_eq!(hidden.items.len(), 1); // a|b and b|a collapse to a single item
        assert_eq!(hidden.items[0].path, "a.ts");
        assert_eq!(hidden.items[0].note.as_deref(), Some("12x ↔ b.ts"));
    }

    #[test]
    fn hidden_coupling_skips_pairs_with_a_static_import_edge() {
        let nodes: [FileNode; 0] = [];
        let mut dep = DepGraph::new();
        dep.insert("a.ts".to_string(), vec!["b.ts".to_string()]);
        let mut coupling = CouplingMap::new();
        coupling.insert(
            "a.ts".to_string(),
            vec![crate::coupling::CouplingEntry {
                path: "b.ts".to_string(),
                count: 12,
            }],
        );
        let input = empty_input(&nodes, &dep, &coupling);
        let recs = build_recommendations(&input, &RecommendationGates::default());
        assert!(recs.iter().all(|r| r.id != RecId::HiddenCoupling));
    }

    #[test]
    fn versioning_candidate_requires_volatile_fan_in_and_fix() {
        let nodes = [FileNode {
            lifecycle: Some(Lifecycle::Volatile),
            fan_in: 5,
            tag_counts: tags(4),
            ..node("legacy.ts")
        }];
        let dep = DepGraph::new();
        let coupling = CouplingMap::new();
        let input = empty_input(&nodes, &dep, &coupling);
        let recs = build_recommendations(&input, &RecommendationGates::default());
        let vc = recs
            .iter()
            .find(|r| r.id == RecId::VersioningCandidate)
            .unwrap();
        assert_eq!(vc.items[0].path, "legacy.ts");
        assert_eq!(
            vc.items[0].note.as_deref(),
            Some("volatile · fan_in 5 · FIX 4")
        );
        assert_eq!(
            vc.items[0].action_hint_key,
            ActionHintKey::VersioningCandidate
        );
    }

    #[test]
    fn bug_prone_action_hint_key_branches_on_fan_in() {
        let nodes = [
            FileNode {
                tag_counts: tags(6),
                fan_in: 3,
                ..node("shared.ts")
            },
            FileNode {
                tag_counts: tags(6),
                fan_in: 0,
                ..node("isolated.ts")
            },
        ];
        let dep = DepGraph::new();
        let coupling = CouplingMap::new();
        let input = empty_input(&nodes, &dep, &coupling);
        let recs = build_recommendations(&input, &RecommendationGates::default());
        let bug = recs.iter().find(|r| r.id == RecId::BugProne).unwrap();
        let shared = bug.items.iter().find(|i| i.path == "shared.ts").unwrap();
        let isolated = bug.items.iter().find(|i| i.path == "isolated.ts").unwrap();
        assert_eq!(shared.action_hint_key, ActionHintKey::BugProneShared);
        assert_eq!(isolated.action_hint_key, ActionHintKey::BugProneIsolated);
    }

    #[test]
    fn hot_churn_action_hint_key_branches_on_fan_in() {
        let nodes = [
            FileNode {
                loc: 40,
                churn: 500,
                fan_in: 5,
                ..node("core.ts")
            },
            FileNode {
                loc: 40,
                churn: 500,
                fan_in: 0,
                ..node("leaf.ts")
            },
        ];
        let dep = DepGraph::new();
        let coupling = CouplingMap::new();
        let input = empty_input(&nodes, &dep, &coupling);
        let recs = build_recommendations(&input, &RecommendationGates::default());
        let hot = recs.iter().find(|r| r.id == RecId::HotChurn).unwrap();
        let core = hot.items.iter().find(|i| i.path == "core.ts").unwrap();
        let leaf = hot.items.iter().find(|i| i.path == "leaf.ts").unwrap();
        assert_eq!(core.action_hint_key, ActionHintKey::HotChurnCore);
        assert_eq!(leaf.action_hint_key, ActionHintKey::HotChurnLeaf);
    }

    #[test]
    fn circular_note_joins_cycle_with_arrow_back_to_start() {
        let nodes: [FileNode; 0] = [];
        let dep = DepGraph::new();
        let coupling = CouplingMap::new();
        let circular = vec![vec![
            "a.ts".to_string(),
            "b.ts".to_string(),
            "c.ts".to_string(),
        ]];
        let input = BuildRecInput {
            nodes: &nodes,
            dep: &dep,
            coupling: &coupling,
            circular: &circular,
            scope_excludes: &[],
            permanent_ignores: &[],
            untested_paths: empty_set(),
            amplification_by_path: empty_map(),
            findings: &[],
        };
        let recs = build_recommendations(&input, &RecommendationGates::default());
        let circ = recs.iter().find(|r| r.id == RecId::Circular).unwrap();
        assert_eq!(circ.items[0].path, "a.ts");
        assert_eq!(
            circ.items[0].note.as_deref(),
            Some("a.ts → b.ts → c.ts → a.ts")
        );
        assert_eq!(circ.items[0].action_hint_key, ActionHintKey::Circular);
    }

    #[test]
    fn untested_and_amplification_raise_cost_and_lower_roi() {
        let nodes = [FileNode {
            fan_out: 10,
            ..node("fat.ts")
        }];
        let dep = DepGraph::new();
        let coupling = CouplingMap::new();
        let baseline_input = empty_input(&nodes, &dep, &coupling);
        let baseline = build_recommendations(&baseline_input, &RecommendationGates::default());
        let baseline_roi = baseline[0].items[0].roi;

        let mut untested_paths = HashSet::new();
        untested_paths.insert("fat.ts".to_string());
        let input = BuildRecInput {
            nodes: &nodes,
            dep: &dep,
            coupling: &coupling,
            circular: &[],
            scope_excludes: &[],
            permanent_ignores: &[],
            untested_paths: &untested_paths,
            amplification_by_path: empty_map(),
            findings: &[],
        };
        let recs = build_recommendations(&input, &RecommendationGates::default());
        assert!(recs[0].items[0].roi < baseline_roi);
    }

    // --- bug evidence + escalation ---

    #[test]
    fn escalates_item_with_critical_finding_into_urgent_group_and_removes_from_home() {
        let nodes = [FileNode {
            tag_counts: tags(6),
            risk_score: 100.0,
            ..node("bug.ts")
        }];
        let dep = DepGraph::new();
        let coupling = CouplingMap::new();
        // A second, still-`Critical`-severity group (circular) that does NOT carry a critical finding —
        // proves urgency_rank, not severity_rank alone, is what puts the urgent group first.
        let circular = vec![vec!["cyc-a.ts".to_string(), "cyc-b.ts".to_string()]];
        let findings = [critical_finding("bug.ts", "be-db/update-delete-no-where")];
        let input = BuildRecInput {
            nodes: &nodes,
            dep: &dep,
            coupling: &coupling,
            circular: &circular,
            scope_excludes: &[],
            permanent_ignores: &[],
            untested_paths: empty_set(),
            amplification_by_path: empty_map(),
            findings: &findings,
        };
        let recs = build_recommendations(&input, &RecommendationGates::default());

        assert_eq!(recs[0].id, RecId::UrgentBugRisk);
        assert_eq!(recs[0].severity, Severity::Critical);
        assert_eq!(recs[0].items.len(), 1);
        let escalated = &recs[0].items[0];
        assert_eq!(escalated.path, "bug.ts");
        assert_eq!(escalated.escalated_from, Some(RecId::BugProne));
        assert_eq!(
            escalated.bug_evidence,
            vec!["1 critical finding(s) in this file: be-db/update-delete-no-where".to_string()]
        );

        // Home group (bug-prone) had only this one item -> dropped entirely, never double-reported.
        assert!(recs.iter().all(|r| r.id != RecId::BugProne));
        // The still-Critical circular group survives, just not at the top.
        assert!(recs.iter().any(|r| r.id == RecId::Circular));
    }

    #[test]
    fn fix_ratio_evidence_rides_along_without_escalating() {
        let nodes = [FileNode {
            fan_out: 10,
            change_count: 10,
            tag_counts: tags(6),
            ..node("fat.ts")
        }];
        let dep = DepGraph::new();
        let coupling = CouplingMap::new();
        let input = empty_input(&nodes, &dep, &coupling);
        let recs = build_recommendations(&input, &RecommendationGates::default());
        let fat = recs.iter().find(|r| r.id == RecId::FatFanout).unwrap();
        let item = &fat.items[0];
        assert_eq!(item.path, "fat.ts");
        assert_eq!(
            item.bug_evidence,
            vec!["6 of 10 changes are bug-fix commits".to_string()]
        );
        assert_eq!(item.escalated_from, None);
        assert!(recs.iter().all(|r| r.id != RecId::UrgentBugRisk));
    }

    #[test]
    fn hotspot_blast_evidence_rides_along_without_escalating() {
        let nodes = [FileNode {
            fan_out: 10,
            fan_in: 5,
            hotspot_score: Some(42.0),
            ..node("fat.ts")
        }];
        let dep = DepGraph::new();
        let coupling = CouplingMap::new();
        let input = empty_input(&nodes, &dep, &coupling);
        let recs = build_recommendations(&input, &RecommendationGates::default());
        let fat = recs.iter().find(|r| r.id == RecId::FatFanout).unwrap();
        let item = &fat.items[0];
        assert_eq!(
            item.bug_evidence,
            vec!["frequently changed and imported by 5 files".to_string()]
        );
        assert_eq!(item.escalated_from, None);
    }

    #[test]
    fn no_evidence_item_is_unaffected() {
        let nodes = [FileNode {
            fan_out: 10,
            ..node("fat.ts")
        }];
        let dep = DepGraph::new();
        let coupling = CouplingMap::new();
        let input = empty_input(&nodes, &dep, &coupling);
        let recs = build_recommendations(&input, &RecommendationGates::default());
        let fat = recs.iter().find(|r| r.id == RecId::FatFanout).unwrap();
        let item = &fat.items[0];
        assert!(item.bug_evidence.is_empty());
        assert_eq!(item.escalated_from, None);
    }

    #[test]
    fn bug_evidence_order_is_critical_findings_then_fix_ratio_then_hotspot() {
        let nodes = [FileNode {
            tag_counts: tags(6),
            change_count: 10,
            fan_in: 5,
            hotspot_score: Some(42.0),
            risk_score: 100.0,
            ..node("bug.ts")
        }];
        let dep = DepGraph::new();
        let coupling = CouplingMap::new();
        let findings = [critical_finding("bug.ts", "be-db/update-delete-no-where")];
        let input = BuildRecInput {
            nodes: &nodes,
            dep: &dep,
            coupling: &coupling,
            circular: &[],
            scope_excludes: &[],
            permanent_ignores: &[],
            untested_paths: empty_set(),
            amplification_by_path: empty_map(),
            findings: &findings,
        };
        let recs = build_recommendations(&input, &RecommendationGates::default());
        let urgent = recs.iter().find(|r| r.id == RecId::UrgentBugRisk).unwrap();
        assert_eq!(
            urgent.items[0].bug_evidence,
            vec![
                "1 critical finding(s) in this file: be-db/update-delete-no-where".to_string(),
                "6 of 10 changes are bug-fix commits".to_string(),
                "frequently changed and imported by 5 files".to_string(),
            ]
        );
    }

    #[test]
    fn build_recommendations_is_deterministic_across_two_runs() {
        let nodes = [
            FileNode {
                tag_counts: tags(6),
                risk_score: 100.0,
                ..node("bug.ts")
            },
            FileNode {
                fan_out: 10,
                change_count: 10,
                tag_counts: tags(6),
                ..node("fat.ts")
            },
        ];
        let dep = DepGraph::new();
        let coupling = CouplingMap::new();
        let findings = [critical_finding("bug.ts", "be-db/update-delete-no-where")];
        let input = BuildRecInput {
            nodes: &nodes,
            dep: &dep,
            coupling: &coupling,
            circular: &[],
            scope_excludes: &[],
            permanent_ignores: &[],
            untested_paths: empty_set(),
            amplification_by_path: empty_map(),
            findings: &findings,
        };
        let gates = RecommendationGates::default();
        let r1 = build_recommendations(&input, &gates);
        let r2 = build_recommendations(&input, &gates);
        assert_eq!(
            serde_json::to_value(&r1).unwrap(),
            serde_json::to_value(&r2).unwrap()
        );
    }
}
