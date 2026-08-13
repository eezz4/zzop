//! Exercises the repository->tree coordinate projection: which paths survive, in what spelling, and
//! which shapes must be left untouched.

use std::path::Path;

use zzop_core::{CommitFileSet, GitPathStats};

use super::*;

fn stats(change_count: u32) -> GitPathStats {
    GitPathStats {
        change_count,
        churn: change_count * 10,
        last_modified: Some("2026-08-13T00:00:00Z".to_string()),
        author_count: 1,
        tag_counts: Default::default(),
        recent_churn: Some(0),
        recent_change_count: Some(0),
        author_commits: None,
        recent_author_commits: None,
    }
}

fn commit(sha: &str, files: &[&str]) -> CommitFileSet {
    CommitFileSet {
        sha: sha.to_string(),
        files: files.iter().map(|f| f.to_string()).collect(),
        tags: vec![],
        date: Some(format!("2026-08-{sha}T00:00:00Z")),
        subject: None,
        labels: vec![],
    }
}

fn collection(commits: Vec<CommitFileSet>) -> GitCollection {
    let mut c = GitCollection {
        commits,
        ..Default::default()
    };
    c.window = crate::parse::build_window(&c.commits, None);
    c
}

#[test]
fn a_subtree_prefix_is_slash_joined_regardless_of_how_the_root_was_spelled() {
    assert_eq!(
        tree_prefix(Path::new("/repo"), Path::new("/repo/packages/alpha")).as_deref(),
        Some("packages/alpha/")
    );
    // A `.` component must not survive into the prefix — a `packages/./alpha/` prefix matches no path
    // git ever emits, so the whole tree would silently read as empty.
    assert_eq!(
        tree_prefix(Path::new("/repo"), Path::new("/repo/./packages/alpha")).as_deref(),
        Some("packages/alpha/")
    );
}

#[test]
fn a_tree_that_is_the_repository_root_has_no_prefix_at_all() {
    assert_eq!(tree_prefix(Path::new("/repo"), Path::new("/repo")), None);
    // Not under the root: nothing sound can be said about the relationship, so nothing is done.
    assert_eq!(
        tree_prefix(Path::new("/repo"), Path::new("/elsewhere/pkg")),
        None
    );
}

#[test]
fn rebasing_keeps_in_tree_paths_in_tree_spelling_and_drops_the_rest() {
    let mut c = collection(vec![
        commit("01", &["packages/alpha/a.ts", "packages/alpha/b.ts"]),
        commit("02", &["packages/beta/x.ts", "README.md"]),
        commit("03", &["packages/alpha/a.ts", "README.md"]),
    ]);
    c.stats
        .by_path
        .insert("packages/alpha/a.ts".to_string(), stats(2));
    c.stats
        .by_path
        .insert("packages/beta/x.ts".to_string(), stats(1));

    c.rebase_onto("packages/alpha/");

    assert_eq!(
        c.stats.by_path.keys().collect::<Vec<_>>(),
        vec!["a.ts"],
        "beta's file is not a file of this tree"
    );
    assert_eq!(c.stats.by_path["a.ts"].change_count, 2, "stats ride along");
    let files: Vec<&[String]> = c.commits.iter().map(|x| x.files.as_slice()).collect();
    assert_eq!(
        files,
        vec![
            &["a.ts".to_string(), "b.ts".to_string()][..],
            &["a.ts".to_string()][..]
        ],
        "the all-outside commit is dropped, and the mixed one keeps only its in-tree half"
    );
}

#[test]
fn a_sibling_whose_name_merely_starts_with_the_trees_is_not_swallowed() {
    let mut c = collection(vec![commit(
        "01",
        &["packages/alpha/a.ts", "packages/alpha-legacy/a.ts"],
    )]);
    c.rebase_onto("packages/alpha/");
    assert_eq!(
        c.commits[0].files,
        vec!["a.ts".to_string()],
        "the trailing slash in the prefix is what makes `alpha-legacy` a different directory"
    );
}

#[test]
fn a_rename_keeps_its_alias_only_when_both_ends_are_in_the_tree() {
    let mut c = collection(vec![]);
    c.stats.alias_to_canonical.insert(
        "packages/alpha/old.ts".to_string(),
        "packages/alpha/new.ts".to_string(),
    );
    // Moved INTO the tree: the old spelling has no tree-relative form, so the link cannot be stated.
    // The canonical file keeps all of its stats — only the alias mapping is lost.
    c.stats.alias_to_canonical.insert(
        "attic/moved.ts".to_string(),
        "packages/alpha/moved.ts".to_string(),
    );
    c.rebase_onto("packages/alpha/");
    assert_eq!(
        c.stats.alias_to_canonical,
        [("old.ts".to_string(), "new.ts".to_string())]
            .into_iter()
            .collect()
    );
}

#[test]
fn the_window_is_re_derived_from_the_commits_that_survived() {
    let mut c = collection(vec![
        commit("01", &["packages/alpha/a.ts", "packages/alpha/b.ts"]),
        commit("09", &["README.md", "packages/beta/x.ts"]),
    ]);
    assert_eq!(c.window.commits, 2, "precondition: the repository's window");
    assert_eq!(c.window.last.as_deref(), Some("2026-08-09T00:00:00Z"));

    c.rebase_onto("packages/alpha/");

    assert_eq!(c.window.commits, 1);
    assert_eq!(
        c.window.last.as_deref(),
        Some("2026-08-01T00:00:00Z"),
        "the repository's newest commit never touched this tree, so it cannot bound this tree's window"
    );
}

#[test]
fn a_tree_with_no_history_of_its_own_becomes_empty_rather_than_inheriting_the_repositorys() {
    let mut c = collection(vec![commit("01", &["README.md", "packages/beta/x.ts"])]);
    c.rebase_onto("packages/gamma/");
    assert!(c.commits.is_empty());
    assert!(c.stats.by_path.is_empty());
    assert_eq!(c.window.commits, 0);
    // The caller's `git_active` is unaffected: collection SUCCEEDED and this tree genuinely has no
    // history, which is `Some(vec![])` downstream — measured, and nothing found. That is a different
    // sentence from the `None` a git-less run produces, and the distinction is the whole point.
}
