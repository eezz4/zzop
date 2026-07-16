//! Exercises `build_file_nodes`: dep/churn/risk combine correctly and sort descending by risk, rename
//! chains merge a past path's dep metrics into the canonical id, files with dep edges but no git
//! history are still included as nodes, and recent/author-commit fields thread through from git
//! stats. Since no dep-graph/git-log parser exists in this workspace yet, the two small helpers below
//! build `DepStats`/`GitStats` directly from plain test fixtures.
use super::*;
use crate::node::DEFAULT_WEIGHTS;

fn dep_stats(edges: &[(&str, &[&str])]) -> DepStats {
    let mut fan_in = BTreeMap::new();
    let mut fan_out = BTreeMap::new();
    let mut all_paths = BTreeSet::new();
    for (src, deps) in edges {
        all_paths.insert((*src).to_string());
        fan_out.insert((*src).to_string(), deps.len() as u32);
        for d in *deps {
            all_paths.insert((*d).to_string());
            *fan_in.entry((*d).to_string()).or_insert(0) += 1;
        }
    }
    DepStats {
        fan_in,
        fan_out,
        all_paths,
    }
}

struct GitEntryInput {
    path: &'static str,
    change_count: u32,
    churn: u32,
    last_modified: &'static str,
    author_count: u32,
    renamed_from: &'static [&'static str],
}

fn git_stats(entries: &[GitEntryInput]) -> GitStats {
    let mut by_path = BTreeMap::new();
    let mut alias_to_canonical = BTreeMap::new();
    for e in entries {
        by_path.insert(
            e.path.to_string(),
            GitPathStats {
                change_count: e.change_count,
                churn: e.churn,
                last_modified: Some(e.last_modified.to_string()),
                author_count: e.author_count,
                tag_counts: HashMap::new(),
                recent_churn: None,
                recent_change_count: None,
                author_commits: None,
                recent_author_commits: None,
            },
        );
        for alias in e.renamed_from {
            alias_to_canonical.insert((*alias).to_string(), e.path.to_string());
        }
    }
    GitStats {
        by_path,
        alias_to_canonical,
    }
}

#[test]
fn combines_fan_change_churn_and_sorts_descending_by_risk() {
    let dep = dep_stats(&[("a.js", &["b.js", "c.js"]), ("b.js", &["c.js"])]);
    let git = git_stats(&[
        GitEntryInput {
            path: "a.js",
            change_count: 10,
            churn: 200,
            last_modified: "2026-01-01",
            author_count: 3,
            renamed_from: &[],
        },
        GitEntryInput {
            path: "b.js",
            change_count: 2,
            churn: 30,
            last_modified: "2026-01-02",
            author_count: 1,
            renamed_from: &[],
        },
    ]);
    let nodes = build_file_nodes(&dep, &git, &HashMap::new(), &DEFAULT_WEIGHTS, |_| true);
    assert_eq!(nodes[0].id, "a.js");
    assert_eq!(nodes[0].fan_out, 2);
    assert!(nodes[0].risk_score > nodes[1].risk_score);
}

#[test]
fn rename_tracking_merges_past_path_dep_metrics_into_canonical_id() {
    let dep = dep_stats(&[("old.js", &["x.js"]), ("y.js", &["old.js"])]);
    let git = git_stats(&[GitEntryInput {
        path: "new.js",
        change_count: 5,
        churn: 50,
        last_modified: "2026-01-03",
        author_count: 2,
        renamed_from: &["old.js"],
    }]);
    let nodes = build_file_nodes(&dep, &git, &HashMap::new(), &DEFAULT_WEIGHTS, |_| true);
    let node = nodes.iter().find(|n| n.id == "new.js").unwrap();
    assert_eq!(node.fan_in, 1);
    assert_eq!(node.fan_out, 1);
    assert_eq!(node.change_count, 5);
}

#[test]
fn files_with_dep_edges_but_no_git_history_are_still_included_as_nodes() {
    let dep = dep_stats(&[("lonely.js", &["other.js"])]);
    let git = git_stats(&[]);
    let nodes = build_file_nodes(&dep, &git, &HashMap::new(), &DEFAULT_WEIGHTS, |_| true);
    assert!(nodes.iter().any(|n| n.id == "lonely.js"));
}

#[test]
fn recent_and_author_commit_fields_are_threaded_through_from_git_stats() {
    let dep = dep_stats(&[("a.js", &["b.js"])]);
    let mut by_path = BTreeMap::new();
    by_path.insert(
        "a.js".to_string(),
        GitPathStats {
            change_count: 10,
            churn: 200,
            last_modified: Some("2026-01-01".to_string()),
            author_count: 2,
            tag_counts: HashMap::new(),
            recent_churn: Some(5),
            recent_change_count: Some(3),
            author_commits: Some(HashMap::from([("a@example.com".to_string(), 7)])),
            recent_author_commits: Some(HashMap::from([("a@example.com".to_string(), 2)])),
        },
    );
    let git = GitStats {
        by_path,
        alias_to_canonical: BTreeMap::new(),
    };
    let nodes = build_file_nodes(&dep, &git, &HashMap::new(), &DEFAULT_WEIGHTS, |_| true);
    let node = nodes.iter().find(|n| n.id == "a.js").unwrap();
    assert_eq!(node.recent_churn, Some(5));
    assert_eq!(node.recent_change_count, Some(3));
    assert_eq!(
        node.author_commits,
        Some(HashMap::from([("a@example.com".to_string(), 7)]))
    );
    assert_eq!(
        node.recent_author_commits,
        Some(HashMap::from([("a@example.com".to_string(), 2)]))
    );
}

#[test]
fn hotspot_requires_min_changes() {
    assert_eq!(hotspot_score(5, 100), 500);
    assert_eq!(hotspot_score(1, 100), 0); // changed once -> not a hotspot
}

/// Non-source files (per `is_source`) get `risk_score` and `hotspot_score` zeroed even under heavy
/// churn/loc — the misleading-diagnostics fix: a huge data file (e.g. a degraded JSON dump) must not
/// dominate a risk-sorted list or a folder's `totalRisk` rollup. `churn`/`loc`/`change_count` stay
/// real (only risk/hotspot are gated). A same-shaped source file is unaffected.
#[test]
fn non_source_files_get_zero_risk_and_hotspot_but_keep_real_churn_and_loc() {
    let dep = dep_stats(&[]);
    let mut loc_by_path = HashMap::new();
    loc_by_path.insert("data/recipes_de-DE.json".to_string(), 50_000);
    loc_by_path.insert("src/app.ts".to_string(), 50_000);
    let git = git_stats(&[
        GitEntryInput {
            path: "data/recipes_de-DE.json",
            change_count: 20,
            churn: 500,
            last_modified: "2026-01-01",
            author_count: 3,
            renamed_from: &[],
        },
        GitEntryInput {
            path: "src/app.ts",
            change_count: 20,
            churn: 500,
            last_modified: "2026-01-01",
            author_count: 3,
            renamed_from: &[],
        },
    ]);
    let nodes = build_file_nodes(&dep, &git, &loc_by_path, &DEFAULT_WEIGHTS, |id| {
        id == "src/app.ts"
    });

    let data_node = nodes
        .iter()
        .find(|n| n.id == "data/recipes_de-DE.json")
        .unwrap();
    assert_eq!(data_node.risk_score, 0.0);
    assert_eq!(data_node.hotspot_score, Some(0.0));
    // churn/loc/change_count untouched — only risk/hotspot are zeroed.
    assert_eq!(data_node.churn, 500);
    assert_eq!(data_node.loc, 50_000);
    assert_eq!(data_node.change_count, 20);

    let source_node = nodes.iter().find(|n| n.id == "src/app.ts").unwrap();
    assert!(source_node.risk_score > 0.0);
    assert_eq!(source_node.hotspot_score, Some(20.0 * 50_000.0));

    // With risk zeroed, the data file no longer outranks the source file in the risk-sorted output.
    assert_eq!(nodes[0].id, "src/app.ts");
}
