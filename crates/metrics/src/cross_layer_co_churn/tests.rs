//! Exercises cross-layer co-churn aggregation.
use super::*;

/// Local slash-prefix layer fixture for `build_cross_layer_co_churn` tests below — distinct from the
/// real `layer_of` (this module's function under test in the "--- layer_of ---" section further down),
/// which additionally takes a `shared_dirs` set.
fn fixture_layer_of(p: &str) -> String {
    match p.find('/') {
        Some(i) => p[..i].to_string(),
        None => "(root)".to_string(),
    }
}

fn commit(sha: &str, files: &[&str]) -> CommitFileSet {
    CommitFileSet {
        sha: sha.to_string(),
        files: files.iter().map(|s| s.to_string()).collect(),
        tags: vec![],
        date: None,
    }
}

// --- layer_of ---

fn shared_dirs() -> BTreeSet<String> {
    ["utils", "types"].into_iter().map(String::from).collect()
}

#[test]
fn layer_of_uses_the_top_level_path_segment() {
    assert_eq!(layer_of("api/routes/x.ts", &shared_dirs()), "api");
    assert_eq!(layer_of("domains/y.ts", &shared_dirs()), "domains");
}

#[test]
fn layer_of_folds_shared_dirs_into_a_sentinel() {
    assert_eq!(layer_of("utils/format.ts", &shared_dirs()), "(shared)");
    assert_eq!(layer_of("types/index.ts", &shared_dirs()), "(shared)");
}

#[test]
fn layer_of_folds_root_level_files_into_a_sentinel() {
    assert_eq!(layer_of("App.tsx", &shared_dirs()), "(root)");
}

#[test]
fn only_cross_layer_pairs_are_counted() {
    let commits = vec![
        commit("a", &["api/x.ts", "domains/y.ts"]), // cross: api<->domains
        commit("b", &["api/x.ts", "api/z.ts"]),     // same layer -> excluded
        commit("c", &["api/x.ts", "domains/y.ts"]), // cross recurrence (co-change 2)
    ];
    let out = build_cross_layer_co_churn(
        &commits,
        fixture_layer_of,
        &CrossLayerCoChurnOptions {
            min_co_changes: Some(1),
            ..Default::default()
        },
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].layer_a, "api");
    assert_eq!(out[0].layer_b, "domains");
    assert_eq!(out[0].co_changes, 2);
    assert_eq!(out[0].pairs, 1);
    assert_eq!(
        out[0].examples[0],
        CrossLayerExample {
            a: "api/x.ts".to_string(),
            b: "domains/y.ts".to_string(),
            count: 2
        }
    );
}

#[test]
fn layer_pairs_below_min_co_changes_are_excluded() {
    let commits = vec![commit("a", &["api/x.ts", "lib/u.ts"])]; // co-change 1
    assert_eq!(
        build_cross_layer_co_churn(
            &commits,
            fixture_layer_of,
            &CrossLayerCoChurnOptions {
                min_co_changes: Some(2),
                ..Default::default()
            }
        )
        .len(),
        0
    );
    assert_eq!(
        build_cross_layer_co_churn(
            &commits,
            fixture_layer_of,
            &CrossLayerCoChurnOptions {
                min_co_changes: Some(1),
                ..Default::default()
            }
        )
        .len(),
        1
    );
}

#[test]
fn commits_exceeding_max_files_per_commit_are_skipped() {
    let mut files: Vec<String> = (0..30).map(|i| format!("api/f{i}.ts")).collect();
    files.push("domains/y.ts".to_string());
    let file_refs: Vec<&str> = files.iter().map(|s| s.as_str()).collect();
    let big = commit("big", &file_refs);
    let out = build_cross_layer_co_churn(
        &[big],
        fixture_layer_of,
        &CrossLayerCoChurnOptions {
            max_files_per_commit: Some(25),
            min_co_changes: Some(1),
            ..Default::default()
        },
    );
    assert_eq!(out.len(), 0);
}

#[test]
fn sorted_descending_by_co_changes_plus_top_pairs_cap() {
    let commits = vec![
        commit("a", &["api/x.ts", "domains/y.ts"]),
        commit("b", &["api/x.ts", "domains/y.ts"]),
        commit("c", &["lib/u.ts", "ui/v.ts"]),
    ];
    let out = build_cross_layer_co_churn(
        &commits,
        fixture_layer_of,
        &CrossLayerCoChurnOptions {
            min_co_changes: Some(1),
            top_pairs: Some(1),
            ..Default::default()
        },
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].co_changes, 2); // api<->domains ranks above lib<->ui(1)
}
