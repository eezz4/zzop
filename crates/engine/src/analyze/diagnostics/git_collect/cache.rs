//! The run-scoped git-collection memo — see [`GitCache`]. Split out of `git_collect.rs` on
//! 2026-08-08, the batch that introduced it, because that file crossed the line-count ratchet; the
//! cut is along the seam the memo already formed (a self-contained mechanism with one caller) rather
//! than an arbitrary one.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// A RUN-scoped memo of `zzop_git::collect` results, keyed by `(repo root, CollectOptions)`.
///
/// Why it exists. `collect_git` runs once per analyzed TREE, and in a monorepo every tree resolves to
/// the same `.git`; `zzop_git` does no path or branch scoping, so all of those calls spawn the same
/// `git log --numstat` and get byte-identical output back. Measured 2026-08-07 on a 22-tree monorepo:
/// one collection is ~1.66s, and 22 of them accounted for **90.9%** of the warm wall clock.
///
/// Why the key is the REPO ROOT and not the tree root. Keying by tree root would give 22 distinct keys
/// for one repository — a cache with a 0% hit rate. The root is resolved by `zzop_git::repo_root`, an
/// in-process ancestor walk, precisely so that computing the key does not spend the git process the
/// cache is trying to save. A tree that resolves to NO repo root is not memoized at all: it has no
/// identity to share, and `zzop_git::collect` will fail for it anyway.
///
/// Why the options are in the key. They are the entire input to the collection besides the repo
/// (`--since`, the recency window, and the two commit-pattern tables), so equal key means equal output
/// by construction. A config CANNOT vary them per tree today — `crates/config/src/mapper.rs` injects
/// one top-level `git` block into every tree — but the key does not rely on that: if per-tree git
/// options are ever added, differing options simply miss the memo instead of silently sharing a
/// collection.
///
/// Why RUN-scoped and not process-global. A `zzop-mcp` process outlives many analyses, and commits
/// land between them; a process-global memo would keep answering with the history as of the first
/// call. This is the same reason the regex-memo experiment was reverted.
#[derive(Default)]
pub(crate) struct GitCache {
    /// `Err` holds the DISPLAY of the error rather than the `GitError`: the failure is shared (it is a
    /// property of the repo), but the warning sentence naming the tree root is not, so each tree
    /// re-derives its own line from the shared cause.
    entries: Mutex<
        HashMap<(PathBuf, zzop_git::CollectOptions), Result<zzop_git::GitCollection, String>>,
    >,
}

impl GitCache {
    /// The collection for `(repo_root, opts)`, running at most one `zzop_git::collect` per distinct key
    /// for the life of this cache. A poisoned lock degrades to an uncached collection rather than
    /// panicking — the memo is an optimization and must never be able to fail an analysis.
    pub(super) fn get_or_collect(
        &self,
        repo_root: &std::path::Path,
        run_in: &std::path::Path,
        opts: &zzop_git::CollectOptions,
    ) -> Result<zzop_git::GitCollection, String> {
        let key = (repo_root.to_path_buf(), opts.clone());
        let Ok(mut entries) = self.entries.lock() else {
            return zzop_git::collect(run_in, opts).map_err(|e| e.to_string());
        };
        if let Some(hit) = entries.get(&key) {
            return hit.clone();
        }
        let fresh = zzop_git::collect(run_in, opts).map_err(|e| e.to_string());
        entries.insert(key, fresh.clone());
        fresh
    }
}
