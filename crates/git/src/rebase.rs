//! Coordinate rebasing: turning a REPOSITORY-relative collection into a TREE-relative one.
//!
//! This crate collects the whole repository, in the repository's coordinate system, on purpose —
//! [`crate::process::run_git_log`] pins `diff.relative=false` precisely so the result is independent of
//! the directory git ran in, which is what lets one run's trees share a single collection. That choice
//! is right for COLLECTING and wrong for REPORTING: a caller analyzing `packages/alpha` publishes
//! `nodes`/`dep`/`folders` relative to `packages/alpha`, so a collection still speaking in
//! `packages/alpha/...` joins with nothing and — worse — carries paths (`packages/beta/...`,
//! `README.md`) the analyzed tree does not contain.
//!
//! Nothing downstream can detect that. A repository-relative path is a well-formed path, so every
//! consumer classifies it happily and emits confident pairs; measured on the zzop repository on
//! 2026-08-13, a 24-file subtree received 1903 co-change edges over 317 paths, none of them its own,
//! byte-identical to the list the repository root produces. So the translation happens HERE, at the one
//! place that knows both coordinate systems, rather than at each of the five consumers downstream.
//!
//! **This is a projection, and projections lose things — deliberately, and only outward.** A path
//! outside the tree is dropped rather than kept or renamed: it is not a file this tree has, and an
//! under-report is this codebase's accepted failure direction. A rename whose two ends straddle the
//! tree boundary loses its alias link for the same reason (the outside end has no in-tree spelling).
//! What is never lost is anything inside the tree.
//!
//! **It costs one pass and no git process.** The rebase runs per tree over an already-collected
//! `GitCollection`; the collection memo stays keyed by repository root, so N trees of one repository
//! still spawn git once (`crates/engine/tests/git_spawn_census.rs` is the gate on that, and it now also
//! asserts that the shared collection arrives correctly rebased).

use std::path::Path;

use crate::GitCollection;

/// The prefix a repository-relative path carries for files inside `tree`, as a `/`-terminated string —
/// `Some("packages/alpha/")` for a subtree, `None` when there is nothing to rebase.
///
/// `None` covers three cases that all mean "leave the collection alone": `tree` IS the repository root
/// (the coordinate systems already coincide — the overwhelmingly common single-repo shape, and it must
/// stay bit-for-bit unchanged), `tree` is not under `repo_root` at all (nothing sound can be said, so
/// nothing is done), and `tree` resolves to only non-`Normal` components.
///
/// Compared by COMPONENTS rather than by string so a `\`-spelled Windows tree root and git's `/`-spelled
/// output meet in the same alphabet, and so a `.` component cannot produce a prefix that matches no path.
pub fn tree_prefix(repo_root: &Path, tree: &Path) -> Option<String> {
    let rel = tree.strip_prefix(repo_root).ok()?;
    let segments: Vec<&str> = rel
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => s.to_str(),
            _ => None,
        })
        .collect();
    if segments.is_empty() {
        return None;
    }
    Some(format!("{}/", segments.join("/")))
}

/// The tree-relative spelling of a repository-relative path, or `None` when the path is outside.
///
/// `prefix` ends in `/`, so this can never accept a sibling whose name merely starts with the tree's
/// (`packages/alpha-legacy/x.ts` is not inside `packages/alpha/`).
fn strip(prefix: &str, path: &str) -> Option<String> {
    path.strip_prefix(prefix)
        .filter(|rest| !rest.is_empty())
        .map(|rest| rest.to_string())
}

impl GitCollection {
    /// Rebases every path in this collection onto the tree `prefix` names, dropping what falls outside.
    ///
    /// Applied to `stats.by_path`, `stats.alias_to_canonical`, each `CommitFileSet::files`, and the
    /// `window` — the whole surface, because a partially rebased collection is the same silent
    /// coordinate mix this function exists to remove, only harder to see.
    ///
    /// A commit left with no in-tree file is DROPPED rather than kept empty: an empty commit is not a
    /// commit that touched this tree, and every downstream count over `commits` (the 2..=25 co-change
    /// window, `window.commits`) would otherwise be counting the enclosing repository's activity while
    /// naming this tree. The corollary is intended: the co-change window then measures how many of THIS
    /// TREE's files a commit touched, which is the only version of the question a tree-scoped answer can
    /// pose — judging it on the repository-wide file count would make a tree's answer depend on files it
    /// cannot see, which is the defect, not the fix.
    pub fn rebase_onto(&mut self, prefix: &str) {
        self.stats.by_path = std::mem::take(&mut self.stats.by_path)
            .into_iter()
            .filter_map(|(path, stats)| Some((strip(prefix, &path)?, stats)))
            .collect();
        // Both ends must be in-tree: an alias is a claim that these two spellings are the same file, and
        // half of that claim is unstatable in this tree's coordinates.
        self.stats.alias_to_canonical = std::mem::take(&mut self.stats.alias_to_canonical)
            .into_iter()
            .filter_map(|(alias, canonical)| {
                Some((strip(prefix, &alias)?, strip(prefix, &canonical)?))
            })
            .collect();
        self.commits.retain_mut(|commit| {
            commit.files = commit
                .files
                .iter()
                .filter_map(|f| strip(prefix, f))
                .collect();
            !commit.files.is_empty()
        });
        // The window described the repository's history; after the filter it must describe this tree's,
        // or it reports a span and a commit count no surviving commit supports.
        self.window = crate::parse::build_window(&self.commits, self.window.since.clone());
    }
}

#[cfg(test)]
mod tests;
