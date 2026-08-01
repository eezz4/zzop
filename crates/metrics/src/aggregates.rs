//! Aggregation helpers — roll up per-file/per-edge data to a coarser granularity for summary views
//! (folder heatmaps, folder dep graphs).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use zzop_core::{DepGraph, FileNode};

// ---------------------------------------------------------------------------------------------
// aggregateByFolder — rolls up FileNodes by folder prefix up to the given depth. Files shallower
// than depth map to ".".
// ---------------------------------------------------------------------------------------------

/// One folder row of the rollup.
///
/// ## `node_count` is not the run's `fileCount`
/// Named for its UNIVERSE, not for "files", because the two are different sets and summing this column
/// never reproduces the reply's top-level `fileCount`. That field counts files WALKED; a `FileNode`
/// exists only for a path that is a dep-graph key OR was touched in the collected git history
/// (`zzop_core::build_file_nodes`'s `collect_canonical_ids`), minus the ones with no LOC and no edges.
/// So with `git` off, every lexical-only file — anything the walk saw but no import edge names — has no
/// node and is in no folder row at all, and the two numbers diverge by exactly that remainder on a
/// perfectly healthy run. It was called `file_count` until 2026-07-31; one reply carrying that word at
/// two levels over two universes read as a rollup that should add up, which it never did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderSummary {
    pub folder: String,
    /// `FileNode`s in this folder — see the type doc for why this is not the walked-file count.
    pub node_count: u32,
    pub total_risk: f64,
    pub avg_risk: f64,
    pub max_risk: f64,
    pub total_changes: u32,
    pub total_churn: u32,
    pub total_loc: u32,
}

/// Rolls up `FileNode`s by folder prefix. `depth` controls how many leading path segments form the
/// folder key (files shallower than `depth` collapse to ".").
///
/// The input is the node list, so every count below is over the node universe — see [`FolderSummary`].
pub fn aggregate_by_folder(nodes: &[FileNode], depth: usize) -> Vec<FolderSummary> {
    // BTreeMap (not a HashMap, which would iterate arbitrarily in Rust) keeps folder insertion
    // order deterministic before the final sort.
    let mut map: BTreeMap<String, FolderSummary> = BTreeMap::new();
    for n in nodes {
        let folder = folder_of(&n.path, depth);
        let cur = map.entry(folder.clone()).or_insert_with(|| FolderSummary {
            folder: folder.clone(),
            node_count: 0,
            total_risk: 0.0,
            avg_risk: 0.0,
            max_risk: 0.0,
            total_changes: 0,
            total_churn: 0,
            total_loc: 0,
        });
        cur.node_count += 1;
        cur.total_risk += n.risk_score;
        cur.max_risk = cur.max_risk.max(n.risk_score);
        cur.total_changes += n.change_count;
        cur.total_churn += n.churn;
        cur.total_loc += n.loc;
    }
    let mut out: Vec<FolderSummary> = map
        .into_values()
        .map(|mut s| {
            s.avg_risk = s.total_risk / s.node_count as f64;
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

#[cfg(test)]
mod tests;
