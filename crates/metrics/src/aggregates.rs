//! Aggregation helpers — roll up per-file/per-edge/per-action data to a coarser granularity for
//! summary views (folder heatmaps, folder dep graphs, CI action blast-radius).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use zzop_core::{DepGraph, FileNode};

// ---------------------------------------------------------------------------------------------
// aggregateByFolder — rolls up FileNodes by folder prefix up to the given depth. Files shallower
// than depth map to ".".
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderSummary {
    pub folder: String,
    pub file_count: u32,
    pub total_risk: f64,
    pub avg_risk: f64,
    pub max_risk: f64,
    pub total_changes: u32,
    pub total_churn: u32,
    pub total_loc: u32,
}

/// Rolls up `FileNode`s by folder prefix. `depth` controls how many leading path segments form the
/// folder key (files shallower than `depth` collapse to ".").
pub fn aggregate_by_folder(nodes: &[FileNode], depth: usize) -> Vec<FolderSummary> {
    // BTreeMap (not a HashMap, which would iterate arbitrarily in Rust) keeps folder insertion
    // order deterministic before the final sort.
    let mut map: BTreeMap<String, FolderSummary> = BTreeMap::new();
    for n in nodes {
        let folder = folder_of(&n.path, depth);
        let cur = map.entry(folder.clone()).or_insert_with(|| FolderSummary {
            folder: folder.clone(),
            file_count: 0,
            total_risk: 0.0,
            avg_risk: 0.0,
            max_risk: 0.0,
            total_changes: 0,
            total_churn: 0,
            total_loc: 0,
        });
        cur.file_count += 1;
        cur.total_risk += n.risk_score;
        cur.max_risk = cur.max_risk.max(n.risk_score);
        cur.total_changes += n.change_count;
        cur.total_churn += n.churn;
        cur.total_loc += n.loc;
    }
    let mut out: Vec<FolderSummary> = map
        .into_values()
        .map(|mut s| {
            s.avg_risk = s.total_risk / s.file_count as f64;
            s
        })
        .collect();
    out.sort_by(|a, b| {
        b.total_risk
            .partial_cmp(&a.total_risk)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.folder.cmp(&b.folder))
    });
    out
}

// ---------------------------------------------------------------------------------------------
// aggregateDepByFolder — rolls up a dep map (file -> file[]) to folder pairs; drops intra-folder
// self-loops.
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderEdge {
    pub source: String,
    pub target: String,
    pub count: u32,
}

/// Rolls up a dep map (file -> file[]) to folder pairs. `dep` is the file->file[] import graph;
/// edges whose source and target folder collapse to the same folder (self-loops) are dropped.
pub fn aggregate_dep_by_folder(dep: &DepGraph, depth: usize) -> Vec<FolderEdge> {
    // Sort dep entries by key first: `dep` is a HashMap (arbitrary iteration order), so we iterate
    // deterministically to keep the accumulation order stable regardless of the underlying hasher.
    let mut sorted_dep: Vec<(&String, &Vec<String>)> = dep.iter().collect();
    sorted_dep.sort_by(|a, b| a.0.cmp(b.0));

    let mut counts: BTreeMap<(String, String), u32> = BTreeMap::new();
    for (src, targets) in sorted_dep {
        let sf = folder_of(src, depth);
        for tgt in targets {
            let tf = folder_of(tgt, depth);
            if sf == tf {
                continue;
            }
            *counts.entry((sf.clone(), tf)).or_insert(0) += 1;
        }
    }

    let mut out: Vec<FolderEdge> = counts
        .into_iter()
        .map(|((source, target), count)| FolderEdge {
            source,
            target,
            count,
        })
        .collect();
    out.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.source.cmp(&b.source))
            .then_with(|| a.target.cmp(&b.target))
    });
    out
}

// ---------------------------------------------------------------------------------------------
// FolderAggregates — AnalyzeOutput::folders' payload: `aggregate_by_folder` + `aggregate_dep_by_folder`
// bundled together, since both roll up the same tree at the same depth and are consumed as one summary
// view.
// ---------------------------------------------------------------------------------------------

/// Default folder-aggregation depth (2 leading path segments, e.g. `features/alpha`) — deep enough to
/// distinguish feature/module directories in a typical tree, shallow enough to stay a small, skimmable
/// summary on a large repo. `AnalyzeOutput::folders`'s sole caller (`engine::analyze::assemble`) uses this
/// constant; a caller with different aggregation needs can call `aggregate_by_folder`/
/// `aggregate_dep_by_folder` directly with its own depth.
pub const DEFAULT_FOLDER_DEPTH: usize = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderAggregates {
    pub summaries: Vec<FolderSummary>,
    pub edges: Vec<FolderEdge>,
}

/// Builds `AnalyzeOutput::folders`: `aggregate_by_folder` + `aggregate_dep_by_folder` over the same
/// `nodes`/`dep` `analyze::assemble` already has in scope, at `depth`. Both inputs are already produced
/// unconditionally by `assemble` (dep-graph + LOC only when git is inactive, real churn when active), so
/// this never needs git history itself — it just rolls up whatever `nodes`/`dep` the caller has.
pub fn build_folder_aggregates(
    nodes: &[FileNode],
    dep: &DepGraph,
    depth: usize,
) -> FolderAggregates {
    FolderAggregates {
        summaries: aggregate_by_folder(nodes, depth),
        edges: aggregate_dep_by_folder(dep, depth),
    }
}

/// First `min(depth, parts.len()-1)` path segments joined by "/"; "." if the path has no folder.
/// Shared by `aggregate_by_folder` and `aggregate_dep_by_folder`.
fn folder_of(path: &str, depth: usize) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() <= 1 {
        return ".".to_string();
    }
    let take = depth.min(parts.len() - 1);
    parts[..take].join("/")
}

// ---------------------------------------------------------------------------------------------
// aggregateActionDeps — aggregates ActionUse[] by owner/vendor for CI external action blast-radius
// visibility.
//
// Design note: no GitHub Actions workflow parser exists in this crate yet. Rather than build one
// just for this aggregate (out of scope here, and not a dependency this file should own),
// `ActionUse` is declared locally as the minimal plain-data input this aggregate needs. If a real
// GitHub Actions parser is added later, its output type should be unified with this one.
//
// Casing note: unlike every other type in this file, `ActionUse`/`ActionDepSummary` do NOT carry
// `#[serde(rename_all = "camelCase")]` — neither `aggregate_action_deps` nor these two types is wired
// into `AnalyzeOutputView` (no caller in `zzop-engine`/`zzop-napi` today), so they never reach the napi
// JSON boundary this casing unification covers. Add the attribute when/if this aggregate is wired up.
// ---------------------------------------------------------------------------------------------

/// A single `uses:` reference extracted from a GitHub Actions workflow file (see module doc above
/// for why this type is declared locally rather than sourced from a dedicated parser).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionUse {
    /// raw `owner/name@ref` or `owner/name/sub@ref`.
    pub action: String,
    /// organization/user.
    pub owner: String,
    /// action name (sub-path excluded).
    pub name: String,
    /// tag/branch/sha.
    pub reference: String,
    /// relative workflow file path.
    pub workflow_file: String,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionDepSummary {
    /// vendor = owner; actions/* is GitHub official.
    pub vendor: String,
    /// distinct action names (owner/name), sorted.
    pub actions: Vec<String>,
    /// distinct workflow files, sorted.
    pub workflows: Vec<String>,
    /// total uses count.
    pub count: u32,
    /// ratio of pinned (SHA ref) uses among this vendor's actions (0..1).
    pub pinned_ratio: f64,
}

const TWO_DECIMAL_FACTOR: f64 = 100.0;

/// True for a 7-40 char hex string (equivalent to the regex `/^[0-9a-f]{7,40}$/i`).
fn is_sha_ref(reference: &str) -> bool {
    let len = reference.len();
    (7..=40).contains(&len) && reference.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Aggregates `ActionUse`s by owner/vendor for CI external-action blast-radius visibility.
pub fn aggregate_action_deps(uses: &[ActionUse]) -> Vec<ActionDepSummary> {
    struct Acc {
        actions: std::collections::BTreeSet<String>,
        workflows: std::collections::BTreeSet<String>,
        count: u32,
        pinned: u32,
    }

    // BTreeMap keeps vendor accumulation order deterministic; final output order is decided by the
    // explicit sort below regardless.
    let mut by_vendor: BTreeMap<String, Acc> = BTreeMap::new();
    for u in uses {
        let cur = by_vendor.entry(u.owner.clone()).or_insert_with(|| Acc {
            actions: std::collections::BTreeSet::new(),
            workflows: std::collections::BTreeSet::new(),
            count: 0,
            pinned: 0,
        });
        cur.actions.insert(format!("{}/{}", u.owner, u.name));
        cur.workflows.insert(u.workflow_file.clone());
        cur.count += 1;
        if is_sha_ref(&u.reference) {
            cur.pinned += 1;
        }
    }

    let mut out: Vec<ActionDepSummary> = by_vendor
        .into_iter()
        .map(|(vendor, v)| ActionDepSummary {
            vendor,
            actions: v.actions.into_iter().collect(),
            workflows: v.workflows.into_iter().collect(),
            count: v.count,
            pinned_ratio: (v.pinned as f64 / v.count as f64 * TWO_DECIMAL_FACTOR).round()
                / TWO_DECIMAL_FACTOR,
        })
        .collect();
    out.sort_by(|a, b| {
        b.workflows
            .len()
            .cmp(&a.workflows.len())
            .then_with(|| a.vendor.cmp(&b.vendor))
    });
    out
}

#[cfg(test)]
mod tests;
