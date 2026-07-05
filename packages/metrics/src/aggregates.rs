//! Aggregation helpers — roll up per-file/per-edge/per-action data to a coarser granularity for
//! summary views (folder heatmaps, folder dep graphs, CI action blast-radius).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use zpz_core::{DepGraph, FileNode};

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
// into `AnalyzeOutputView` (no caller in `zpz-engine`/`zpz-napi` today), so they never reach the napi
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
mod tests {
    //! Exercises `aggregate_by_folder`, `aggregate_dep_by_folder`, and `aggregate_action_deps`.
    use super::*;

    // --- aggregateByFolder ---

    fn node(path: &str, risk_score: f64, change_count: u32, churn: u32, loc: u32) -> FileNode {
        FileNode {
            id: path.to_string(),
            path: path.to_string(),
            change_count,
            churn,
            last_modified: Some("2026-01-01".to_string()),
            author_count: 1,
            loc,
            tag_counts: std::collections::HashMap::new(),
            fan_in: 0,
            fan_out: 0,
            total_connections: 0,
            risk_score,
            ..Default::default()
        }
    }

    fn n(path: &str, risk_score: f64) -> FileNode {
        node(path, risk_score, 1, 10, 100)
    }

    #[test]
    fn depth_2_aggregates_by_top_2_path_segments() {
        let nodes = vec![
            n("features/alpha/a.tsx", 10.0),
            n("features/alpha/b.tsx", 20.0),
            n("features/beta/c.tsx", 5.0),
        ];
        let agg = aggregate_by_folder(&nodes, 2);
        assert_eq!(agg[0].folder, "features/alpha");
        assert_eq!(agg[0].file_count, 2);
        assert_eq!(agg[0].total_risk, 30.0);
        assert_eq!(agg[0].avg_risk, 15.0);
        assert_eq!(agg[0].max_risk, 20.0);
    }

    #[test]
    fn root_level_files_are_grouped_under_dot() {
        let agg = aggregate_by_folder(&[n("App.tsx", 7.0)], 2);
        assert_eq!(agg[0].folder, ".");
    }

    /// A non-source file's `risk_score` is zeroed upstream (`zpz_core::build_file_nodes`'s `is_source`
    /// gate) before it ever reaches `aggregate_by_folder` — this exercises the folder rollup's side of
    /// that contract: a huge, high-churn data file (risk pre-zeroed, as the real pipeline would produce)
    /// must not dominate `totalRisk`/`maxRisk`, even though its `churn`/`loc` still roll up honestly.
    #[test]
    fn zeroed_risk_non_source_file_does_not_dominate_folder_total_risk() {
        let nodes = vec![
            node("data/recipes_de-DE.json", 0.0, 500, 9_000, 200_000), // huge churn/loc, risk pre-zeroed
            node("data/app.ts", 12.0, 5, 50, 300),
        ];
        let agg = aggregate_by_folder(&nodes, 1);
        assert_eq!(agg[0].folder, "data");
        assert_eq!(agg[0].total_risk, 12.0); // only the source file's risk counts
        assert_eq!(agg[0].max_risk, 12.0);
        assert_eq!(agg[0].avg_risk, 6.0);
        // churn/loc still aggregate the data file's real numbers — only risk is affected.
        assert_eq!(agg[0].total_churn, 9_050);
        assert_eq!(agg[0].total_loc, 200_300);
    }

    #[test]
    fn sorted_descending_by_total_risk() {
        let agg = aggregate_by_folder(&[n("a/x.ts", 5.0), n("b/x.ts", 50.0)], 1);
        assert_eq!(agg[0].folder, "b");
    }

    // --- aggregateDepByFolder ---

    fn dep(pairs: &[(&str, &[&str])]) -> DepGraph {
        pairs
            .iter()
            .map(|(k, vs)| (k.to_string(), vs.iter().map(|s| s.to_string()).collect()))
            .collect()
    }

    #[test]
    fn intra_folder_edges_self_loops_are_dropped() {
        let d = dep(&[
            ("features/a/x.ts", &["features/a/y.ts"]),
            ("features/a/y.ts", &["features/a/z.ts"]),
        ]);
        assert_eq!(aggregate_dep_by_folder(&d, 2), vec![]);
    }

    #[test]
    fn cross_folder_edges_are_summed_at_folder_granularity() {
        let d = dep(&[
            ("features/a/x.ts", &["features/b/y.ts", "features/b/z.ts"]),
            ("features/a/p.ts", &["features/b/q.ts"]),
        ]);
        let edges = aggregate_dep_by_folder(&d, 2);
        assert_eq!(
            edges,
            vec![FolderEdge {
                source: "features/a".to_string(),
                target: "features/b".to_string(),
                count: 3,
            }]
        );
    }

    #[test]
    fn depth_option_depth_1_collapses_features_star_into_features_causing_self_loop_drop() {
        let d = dep(&[
            ("features/a/x.ts", &["features/b/y.ts"]),
            ("features/c/x.ts", &["pages/recommendation/RecPage.tsx"]),
        ]);
        let edges = aggregate_dep_by_folder(&d, 1);
        assert_eq!(
            edges,
            vec![FolderEdge {
                source: "features".to_string(),
                target: "pages".to_string(),
                count: 1,
            }]
        );
    }

    #[test]
    fn sorted_descending_by_count() {
        let d = dep(&[
            ("features/a/x.ts", &["features/b/y.ts"]),
            ("features/c/x.ts", &["features/d/y.ts", "features/d/z.ts"]),
        ]);
        let edges = aggregate_dep_by_folder(&d, 2);
        let counts: Vec<u32> = edges.iter().map(|e| e.count).collect();
        assert_eq!(counts, vec![2, 1]);
    }

    // --- FolderAggregates ---

    #[test]
    fn build_folder_aggregates_bundles_summaries_and_edges() {
        let nodes = vec![
            n("features/alpha/a.tsx", 10.0),
            n("features/beta/c.tsx", 5.0),
        ];
        let dep = dep(&[("features/alpha/a.tsx", &["features/beta/c.tsx"] as &[&str])]);
        let agg = build_folder_aggregates(&nodes, &dep, 2);
        assert_eq!(agg.summaries.len(), 2);
        assert_eq!(agg.summaries[0].folder, "features/alpha"); // sorted by total_risk desc
        assert_eq!(
            agg.edges,
            vec![FolderEdge {
                source: "features/alpha".to_string(),
                target: "features/beta".to_string(),
                count: 1,
            }]
        );
    }

    // --- aggregateActionDeps ---

    fn u(owner: &str, name: &str, reference: &str, workflow_file: &str) -> ActionUse {
        ActionUse {
            action: format!("{owner}/{name}@{reference}"),
            owner: owner.to_string(),
            name: name.to_string(),
            reference: reference.to_string(),
            workflow_file: workflow_file.to_string(),
            line: 1,
        }
    }

    #[test]
    fn distinct_actions_workflows_and_count_per_owner() {
        let summary = aggregate_action_deps(&[
            u("actions", "checkout", "v4", "a.yml"),
            u("actions", "checkout", "v4", "b.yml"),
            u("actions", "setup-node", "v4", "a.yml"),
            u("pnpm", "action-setup", "v4", "a.yml"),
        ]);
        assert_eq!(summary.len(), 2);
        let actions = summary.iter().find(|s| s.vendor == "actions").unwrap();
        assert_eq!(
            actions.actions,
            vec![
                "actions/checkout".to_string(),
                "actions/setup-node".to_string()
            ]
        );
        assert_eq!(
            actions.workflows,
            vec!["a.yml".to_string(), "b.yml".to_string()]
        );
        assert_eq!(actions.count, 3);
    }

    #[test]
    fn sorted_descending_by_workflow_count() {
        let summary = aggregate_action_deps(&[
            u("small", "checkout", "v4", "a.yml"),
            u("big", "checkout", "v4", "a.yml"),
            u("big", "checkout", "v4", "b.yml"),
        ]);
        let vendors: Vec<String> = summary.into_iter().map(|s| s.vendor).collect();
        assert_eq!(vendors, vec!["big".to_string(), "small".to_string()]);
    }

    #[test]
    fn pinned_ratio_sha_ref_counts_as_pinned_tag_does_not() {
        let summary = aggregate_action_deps(&[
            u("x", "checkout", "v4", "a.yml"),
            u("x", "checkout", "a1b2c3d4e5f6", "a.yml"),
            u("x", "checkout", "abc1234", "a.yml"),
        ]);
        assert!((summary[0].pinned_ratio - 2.0 / 3.0).abs() < 0.01);
    }

    #[test]
    fn all_pinned_ratio_1() {
        let summary = aggregate_action_deps(&[
            u("x", "checkout", &"a".repeat(40), "a.yml"),
            u("x", "checkout", &"b".repeat(40), "a.yml"),
        ]);
        assert_eq!(summary[0].pinned_ratio, 1.0);
    }

    #[test]
    fn empty_input_empty_array() {
        assert_eq!(aggregate_action_deps(&[]), vec![]);
    }
}
