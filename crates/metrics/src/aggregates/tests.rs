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

/// A non-source file's `risk_score` is zeroed upstream (`zzop_core::build_file_nodes`'s `is_source`
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
