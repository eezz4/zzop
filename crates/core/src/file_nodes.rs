//! Combines dep graph, git stats, and LOC into `FileNode`s (includes risk score, sorted descending by risk).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::node::{
    calc_risk_score, classify_lifecycle, compute_median_churn, FileNode, RiskInput, RiskWeights,
    DEFAULT_RECENT_THRESHOLD_DAYS,
};

#[cfg(test)]
mod tests;
mod time;

use time::{now_ms, parse_iso_to_ms};

/// A file changed fewer than this many times is not "frequently changed" -> not a hotspot.
///
/// Lives here (rather than with `zzop_metrics::roi`'s other ROI scoring, its pre-R3 home) because
/// `build_file_nodes` below — a core mechanism `zzop_engine::analyze::assemble` always calls, git-backed
/// or not — is `hotspot_score`'s only caller; core must not gain an upward dependency on `zzop_metrics` to
/// reach it — a crate-boundary decision: mechanisms a core computation depends on stay in core even when
/// related scoring logic lives in a dedicated metrics crate.
pub const HOTSPOT_MIN_CHANGES: u32 = 2;

/// Canonical hotspot = change-frequency x complexity (LOC proxy); 0 for files changed fewer than the minimum.
pub fn hotspot_score(change_count: u32, loc: u32) -> u64 {
    if change_count >= HOTSPOT_MIN_CHANGES {
        change_count as u64 * loc as u64
    } else {
        0
    }
}

// ---------------------------------------------------------------------------------------------
// DepStats / GitStats: minimal plain-data input this builder needs, declared locally here rather
// than depending on a shared parser output type, since no dep-graph/git-log parser exists in this
// workspace yet (precedent: `ActionUse` in aggregates.rs). Unify these with the real parser output
// types once dep-graph/git-log parsing lands — the field names and semantics here are the intended
// target shape.
//
// BTreeMap/BTreeSet are used here (rather than a hash-based map/set) so iteration is deterministic
// regardless of construction order.
// ---------------------------------------------------------------------------------------------

/// Fan-in/fan-out stats derived from a dep graph.
#[derive(Debug, Clone, Default)]
pub struct DepStats {
    pub fan_in: BTreeMap<String, u32>,
    pub fan_out: BTreeMap<String, u32>,
    /// All file paths that appear in the graph (as a source or as a target).
    pub all_paths: BTreeSet<String>,
}

/// Per-canonical-path accumulated git metrics.
#[derive(Debug, Clone)]
pub struct GitPathStats {
    pub change_count: u32,
    pub churn: u32,
    pub last_modified: Option<String>,
    pub author_count: u32,
    pub tag_counts: HashMap<String, u32>,
    /// Churn within the last N days (used to keep rename-only commits from counting as a recent touch).
    pub recent_churn: Option<u32>,
    pub recent_change_count: Option<u32>,
    pub author_commits: Option<HashMap<String, u32>>,
    pub recent_author_commits: Option<HashMap<String, u32>>,
}

/// Normalized git-log stats + rename-chain map.
#[derive(Debug, Clone, Default)]
pub struct GitStats {
    /// canonical (current) path -> accumulated metrics.
    pub by_path: BTreeMap<String, GitPathStats>,
    /// old path -> canonical path (used when joining with the dep graph).
    pub alias_to_canonical: BTreeMap<String, String>,
}

// ---------------------------------------------------------------------------------------------
// buildFileNodes
// ---------------------------------------------------------------------------------------------

/// `is_source(id)` — true when `id` is a language this engine has a parser frontend for (the same
/// classification `zzop_engine::dispatch::dispatch` makes; threaded in as a closure, not a hardcoded
/// extension list here, since `core` must not gain an upward dependency on `zzop_engine` — same crate-
/// boundary rule as the `zzop_metrics` note above). Files it says "no" to (data/config/assets — json,
/// md, lockfiles, images, ... — see `build_one`'s doc) still get real `churn`/`loc`/`change_count`;
/// only `risk_score`/`hotspot_score` are zeroed for them, so they never misleadingly dominate a
/// risk-sorted list or a folder's `totalRisk` rollup (`zzop_metrics::aggregate_by_folder`) — a large
/// generated JSON dump is honest churn/size data but is not "risk" (misleading diagnostics are treated
/// as product defects; see docs/modules/napi.md's `nodes` field note).
pub fn build_file_nodes<F>(
    dep: &DepStats,
    git: &GitStats,
    loc_by_path: &HashMap<String, u32>,
    weights: &RiskWeights,
    is_source: F,
) -> Vec<FileNode>
where
    F: Fn(&str) -> bool,
{
    let ids = collect_canonical_ids(dep, git);

    let mut rename_by: BTreeMap<String, u32> = BTreeMap::new();
    for canonical in git.alias_to_canonical.values() {
        *rename_by.entry(canonical.clone()).or_insert(0) += 1;
    }

    let mut nodes: Vec<FileNode> = ids
        .iter()
        .map(|id| {
            let rename_count = rename_by.get(id).copied().unwrap_or(0);
            build_one(
                id,
                dep,
                git,
                loc_by_path,
                weights,
                rename_count,
                is_source(id),
            )
        })
        // Drop files absent from both the dep graph and disk (deleted git-history remnants).
        .filter(|n| !(n.loc == 0 && n.fan_in == 0 && n.fan_out == 0))
        .collect();

    let churns: Vec<u32> = nodes.iter().map(|n| n.churn).collect();
    let median_churn = compute_median_churn(&churns);
    let now = now_ms();
    for n in &mut nodes {
        n.lifecycle = Some(classify_lifecycle(
            n.churn,
            n.last_modified.as_deref().and_then(parse_iso_to_ms),
            median_churn,
            now,
            DEFAULT_RECENT_THRESHOLD_DAYS,
            n.recent_churn,
        ));
    }

    nodes.sort_by(|a, b| {
        b.risk_score
            .partial_cmp(&a.risk_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    nodes
}

fn collect_canonical_ids(dep: &DepStats, git: &GitStats) -> BTreeSet<String> {
    let mut ids: BTreeSet<String> = BTreeSet::new();
    for p in &dep.all_paths {
        let canonical = git
            .alias_to_canonical
            .get(p)
            .cloned()
            .unwrap_or_else(|| p.clone());
        ids.insert(canonical);
    }
    for p in git.by_path.keys() {
        ids.insert(p.clone());
    }
    ids
}

/// `is_source` = this id's own classification from `build_file_nodes`'s `is_source` closure — see that
/// function's doc for why `risk_score`/`hotspot_score` are the only two fields gated on it (`churn`/
/// `loc`/`change_count` etc. stay real either way; a non-source file's size/churn is honest data, just
/// not "risk").
#[allow(clippy::too_many_arguments)]
fn build_one(
    id: &str,
    dep: &DepStats,
    git: &GitStats,
    loc_by_path: &HashMap<String, u32>,
    weights: &RiskWeights,
    rename_count: u32,
    is_source: bool,
) -> FileNode {
    let dep_path = find_dep_path(id, dep, git);
    let git_entry = git.by_path.get(id);
    let fan_in = dep_path
        .as_deref()
        .and_then(|p| dep.fan_in.get(p))
        .copied()
        .unwrap_or(0);
    let fan_out = dep_path
        .as_deref()
        .and_then(|p| dep.fan_out.get(p))
        .copied()
        .unwrap_or(0);
    let total_connections = fan_in + fan_out;
    let change_count = git_entry.map(|g| g.change_count).unwrap_or(0);
    let churn = git_entry.map(|g| g.churn).unwrap_or(0);
    let author_count = git_entry.map(|g| g.author_count).unwrap_or(0);
    let tag_counts = git_entry.map(|g| g.tag_counts.clone()).unwrap_or_default();
    let loc = loc_by_path.get(id).copied().unwrap_or(0);
    let risk_score = if is_source {
        calc_risk_score(
            &RiskInput {
                change_count,
                churn,
                total_connections,
            },
            weights,
        )
    } else {
        0.0
    };
    // Canonical hotspot = change-FREQUENCY x complexity (LOC proxy); `hotspot_score` above already applies the
    // "changed at least HOTSPOT_MIN_CHANGES times" gate. Zeroed for non-source files for the same reason as
    // `risk_score` just above: a data file's `loc` is a lexical line count, not a complexity signal, so
    // change_count * loc for e.g. a huge JSON dump is not a meaningful "hotspot".
    let hotspot = if is_source {
        hotspot_score(change_count, loc) as f64
    } else {
        0.0
    };
    FileNode {
        id: id.to_string(),
        path: id.to_string(),
        change_count,
        churn,
        last_modified: git_entry.and_then(|g| g.last_modified.clone()),
        author_count,
        loc,
        tag_counts,
        fan_in,
        fan_out,
        total_connections,
        risk_score,
        hotspot_score: Some(hotspot),
        rename_count: Some(rename_count),
        lifecycle: None,
        recent_churn: git_entry.and_then(|g| g.recent_churn),
        recent_change_count: git_entry.and_then(|g| g.recent_change_count),
        author_commits: git_entry.and_then(|g| g.author_commits.clone()),
        recent_author_commits: git_entry.and_then(|g| g.recent_author_commits.clone()),
    }
}

/// Resolves the dep-graph key for a canonical id: either the id itself, or (if the id was renamed) a past alias
/// that still appears in the dep graph — this is how a renamed file's OLD fan-in/fan-out metrics are merged onto
/// its new canonical id.
fn find_dep_path(canonical_id: &str, dep: &DepStats, git: &GitStats) -> Option<String> {
    if dep.all_paths.contains(canonical_id) {
        return Some(canonical_id.to_string());
    }
    for (alias, canonical) in &git.alias_to_canonical {
        if canonical == canonical_id && dep.all_paths.contains(alias) {
            return Some(alias.clone());
        }
    }
    None
}
