//! Single-threaded, pre-sorted file walk feeding `run_file_pass`'s `rayon::par_iter`.

use std::path::{Path, PathBuf};

use ignore::gitignore::Gitignore;
use ignore::WalkBuilder;

use crate::dispatch::{self, DispatchConfig};

/// Walks `root` collecting every file not under a `config.skip_dirs` directory and not excluded by a
/// committed `.gitignore` (nested ones, plus ancestor ones up to the git toplevel), as `(normalized rel
/// path, absolute path)` pairs sorted by the rel path. A read error on a subdirectory is swallowed —
/// the walk continues, never panics.
///
/// **Ancestor `.gitignore`s**: when `root` is below the git toplevel (e.g. a monorepo subdir), a
/// `.gitignore` above `root` is just as "committed" as one under it, and real `git` honors it.
/// `WalkBuilder`'s own `parents(true)` is unsuitable — it climbs unboundedly past the repo — so this
/// function does its own bounded walk (`ancestor_gitignores`): from `root` upward, stopping at the
/// first `.git` found, loading each ancestor `.gitignore` anchored to its own directory, OR'd with the
/// crate's built-in handling for files at-or-below `root`. Known gap: an at-or-below-`root` `!pattern`
/// re-inclusion of something an ancestor ignores would win under real `git` but not here.
///
/// **Determinism contract**: output must be byte-identical across machines/clones of the same commit,
/// so only `.gitignore` files on disk are honored — every machine-local ignore source (`core
/// .excludesFile`, `.git/info/exclude`, `WalkBuilder`'s own unbounded `parents`, ripgrep's `.ignore`)
/// is explicitly turned off, while `require_git`/`git_ignore` stay on so a non-git tree is still
/// scanned. Dotfiles are walked like any other file; symlinks are never followed (avoids loops/escaping
/// `root`). `config.skip_dirs` is enforced unconditionally via `filter_entry`, independent of
/// `.gitignore`; the walk root itself is exempt.
pub(super) fn walk_files(root: &Path, config: &DispatchConfig) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let skip_config = config.clone();
    let ancestor_ignores = ancestor_gitignores(root);
    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(false)
        .git_global(false)
        .git_exclude(false)
        .parents(false)
        .ignore(false)
        .require_git(false)
        .git_ignore(true)
        .follow_links(false)
        .filter_entry(move |entry| {
            if entry.depth() == 0 {
                return true;
            }
            if let Some(ft) = entry.file_type() {
                if ft.is_dir() {
                    let name = entry.file_name().to_string_lossy();
                    if dispatch::is_skip_dir(&name, &skip_config) {
                        return false;
                    }
                }
            }
            !ancestor_ignored(entry, &ancestor_ignores)
        });
    for entry in builder.build().filter_map(Result::ok) {
        let is_file = entry.file_type().map(|ft| ft.is_file()).unwrap_or(false);
        if is_file {
            out.push((to_rel(root, entry.path()), entry.path().to_path_buf()));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// The directory containing `.git` at or above `root` (a `.git` entry may be a dir or, for a worktree, a
/// file pointing elsewhere — presence alone marks the boundary, same as `git` itself checks). `None` if
/// the filesystem root is reached with no `.git` found (a non-git tree).
fn find_git_toplevel(root: &Path) -> Option<PathBuf> {
    let mut dir = root;
    loop {
        if dir.join(".git").exists() {
            return Some(dir.to_path_buf());
        }
        dir = dir.parent()?;
    }
}

/// Every `.gitignore` between the git toplevel (inclusive) and `root` (exclusive — `root`'s own, and
/// everything below it, is already handled by `WalkBuilder`'s built-in nested traversal), ordered
/// farthest-from-`root` first. Empty when `root` is the toplevel, or no toplevel is found.
fn ancestor_gitignores(root: &Path) -> Vec<Gitignore> {
    let Some(toplevel) = find_git_toplevel(root) else {
        return Vec::new();
    };
    if toplevel == root {
        return Vec::new();
    }
    let mut dirs = Vec::new();
    let mut cur = root.parent();
    while let Some(dir) = cur {
        dirs.push(dir.to_path_buf());
        if dir == toplevel {
            break;
        }
        cur = dir.parent();
    }
    dirs.reverse(); // farthest (toplevel) first, nearest-to-root last.
    dirs.into_iter()
        .filter_map(|dir| {
            let gi_path = dir.join(".gitignore");
            if !gi_path.is_file() {
                return None;
            }
            // Errors (a malformed glob line) are swallowed: `Gitignore::new` still returns a matcher
            // built from whichever lines did parse.
            let (gitignore, _err) = Gitignore::new(&gi_path);
            Some(gitignore)
        })
        .collect()
}

/// Whether any ancestor `.gitignore` ignores `entry`. `ancestors` is ordered farthest-from-`root`
/// first, so a nearer matcher's verdict overrides a farther one — "closer `.gitignore` wins", same as
/// real `git`. A matcher with no opinion (`Match::None`) never changes the running verdict.
fn ancestor_ignored(entry: &ignore::DirEntry, ancestors: &[Gitignore]) -> bool {
    if ancestors.is_empty() {
        return false;
    }
    let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
    let path = entry.path();
    let mut ignored = false;
    for gi in ancestors {
        match gi.matched(path, is_dir) {
            ignore::Match::Ignore(_) => ignored = true,
            ignore::Match::Whitelist(_) => ignored = false,
            ignore::Match::None => {}
        }
    }
    ignored
}

/// `path` relative to `root`, joined with forward slashes regardless of host OS separator — every
/// downstream consumer expects POSIX-style rel paths.
fn to_rel(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}
