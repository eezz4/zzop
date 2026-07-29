//! Single-threaded, pre-sorted file walk feeding `run_file_pass`'s `rayon::par_iter`.

use std::path::{Component, Path, PathBuf};

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
///
/// **zzop's own output is excluded, on two independent axes.**
///
/// 1. `.zzop` is a RESERVED NAMESPACE (2026-07-29): a directory with that name is pruned wherever it
///    appears and whatever the config says — the same standing `git` gives `.git`. It is checked FIRST,
///    ahead of `config.skip_dirs` (which a caller may replace wholesale) and ahead of the `cache_dir`
///    comparison below, so neither can disarm it. The name is not a convention a project picks: this
///    codebase already defines it as zzop-owned derived output (`zzop_cache::TOOL_DIR`,
///    `DEFAULT_CACHE_DIR`, and the on-disk `.zzop/` = derived vs `zzop/` = user-authored split). The
///    dotless sibling `zzop/` is deliberately NOT reserved — that is source a human wrote.
///
///    This is a PARTIAL REVERSAL of the 2026-07-28 judgement recorded below, and it costs something real:
///    analyzing some OTHER project's `.zzop` tree on purpose is now impossible without a new opt-in knob.
///    Taken knowingly, because axis 2 alone left the protection disarmed in two measured cases — a run
///    with `cacheDir: null` (nothing to prune, so an earlier run's cache is walked as source: `files=2`
///    became `files=7` on a one-file tree) and a `cacheDir` moved from A to B (B is pruned, A's leftovers
///    are not). Both are sealed by behaviour in `tests/analyze_self_output_exclusion.rs`.
///
/// 2. **This run's own `cache_dir`** (`EngineConfig::cache_dir`, `None` when caching is off) is pruned by
///    RESOLVED DIRECTORY, so a `cacheDir` an author parked somewhere other than `.zzop` is still excluded
///    and one pointing outside `root` excludes nothing. Without this the run's output becomes the next
///    run's input and the file count compounds (3 -> 9 -> 21 -> 45 … on a two-file project), a
///    determinism-contract violation before it is anything else. Axis 1 does not make this redundant:
///    the two cover different sets, and only this one follows a relocated cache.
///
/// Neither weakens the determinism contract above — resolving `cache_dir` touches the filesystem, but what
/// it resolves is a run-local knob pointing at DERIVED output, so for one source tree plus one config the
/// set of SOURCE files walked is the same everywhere. They strengthen it, in fact: run N and run N+1 over
/// an unchanged tree were not previously byte-identical to each other.
pub(super) fn walk_files(
    root: &Path,
    config: &DispatchConfig,
    cache_dir: Option<&Path>,
) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let skip_config = config.clone();
    let ancestor_ignores = ancestor_gitignores(root);
    let own_output = cache_dir.and_then(|dir| own_output_dir(root, dir));
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
                    // RESERVED NAMESPACE, checked before anything configurable: `.zzop` is zzop's own
                    // derived-output directory by construction, so it is pruned wherever it appears and
                    // whatever the config says — the standing `git` gives `.git`. See this function's doc.
                    if name == zzop_cache::TOOL_DIR {
                        return false;
                    }
                    if dispatch::is_skip_dir(&name, &skip_config) {
                        return false;
                    }
                    if own_output.as_deref() == Some(entry.path()) {
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

/// `cache_dir` expressed as a path the walk will actually produce — `root` joined with `cache_dir`'s
/// position under it — or `None` when there is nothing for this walk to exclude:
/// - `cache_dir` resolves OUTSIDE `root` (an author who parked the cache elsewhere): the walk never
///   reaches it, so no exclusion is needed.
/// - `cache_dir` resolves TO `root` itself: pruning it would prune the whole tree. The walk root is exempt
///   from every other filter here for the same reason, so it is exempt from this one too.
///
/// Both sides are put in the same canonical-ish form before being compared, so `cacheDir` spelled relative
/// (resolved against the process CWD, as the config front-end resolves it), spelled with `.`/`..`, or
/// reaching `root` through a symlinked ancestor all still land on the same directory. The RESULT is then
/// re-expressed against the caller's own `root`, because that is the form `ignore` hands back for every
/// entry — comparing a canonicalized entry path per directory would be a syscall per directory instead of
/// two for the whole walk.
fn own_output_dir(root: &Path, cache_dir: &Path) -> Option<PathBuf> {
    let rel = canonical_ish(cache_dir);
    let rel = rel.strip_prefix(canonical_ish(root)).ok()?;
    if rel.as_os_str().is_empty() {
        return None;
    }
    Some(root.join(rel))
}

/// `path` made absolute and, as far as the filesystem can answer, canonical — including when it does not
/// exist yet (a cache directory is created by the store, and `walk_files` must not depend on the order of
/// those two). `..`/`.` are resolved lexically first, then the deepest ANCESTOR that does exist is
/// canonicalized and the missing tail re-appended, so two spellings of the same directory always produce
/// the same output regardless of which parts of them exist.
fn canonical_ish(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        match std::env::current_dir() {
            Ok(cwd) => cwd.join(path),
            Err(_) => path.to_path_buf(),
        }
    };
    let lexical = lexically_normalized(&absolute);
    let mut tail: Vec<&std::ffi::OsStr> = Vec::new();
    let mut cur: &Path = &lexical;
    loop {
        if let Ok(canonical) = cur.canonicalize() {
            let mut out = canonical;
            out.extend(tail.iter().rev());
            return out;
        }
        match (cur.file_name(), cur.parent()) {
            // `lexically_normalized` left no `.`/`..` behind, so every step here pops a real name.
            (Some(name), Some(parent)) => {
                tail.push(name);
                cur = parent;
            }
            _ => return lexical,
        }
    }
}

/// `path` with `.` dropped and `..` resolved against the preceding component textually — no filesystem
/// access, so it is only the FALLBACK layer under `canonical_ish`'s real canonicalization (textual `..`
/// resolution differs from the kernel's when a symlink precedes the `..`).
fn lexically_normalized(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
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
