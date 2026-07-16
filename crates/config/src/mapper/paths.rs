// ---------------------------------------------------------------------------------------------------
// Path resolution — the documented deviation from JS: `root`/`cacheDir`/`packs.extraDirs` entries are
// resolved to absolute against `base_dir` here (a server host's process cwd is meaningless, unlike a
// CLI's), while overlay paths resolve against each TREE'S resolved root (JS parity, see
// `resolve_overlays_for_root`). Resolution is purely LEXICAL (`.`/`..` segments collapsed against the
// path text, no symlink following, no filesystem access, no existence requirement) — the same
// contract Node's `path.resolve` gives the JS mapper, so a nonexistent `cacheDir` or `packsDir` still
// resolves cleanly (existence is the engine's problem at load time, not this mapper's).
// ---------------------------------------------------------------------------------------------------

use std::path::{Path, PathBuf};

/// Resolves `raw` against `base_dir`: absolute inputs are normalized as-is, relative inputs are
/// joined onto `base_dir` first. Assumes `base_dir` is itself already absolute (the caller's
/// responsibility — an MCP host always hands this crate an absolute repo root).
pub(super) fn resolve_path(base_dir: &Path, raw: &str) -> PathBuf {
    let candidate = Path::new(raw);
    let joined = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        base_dir.join(candidate)
    };
    normalize_lexically(&joined)
}

/// Collapses `.`/`..` path components purely lexically (no filesystem access), mirroring Node's
/// `path.resolve`/`path.normalize` semantics: a `..` pops the previous real (`Normal`) component when
/// there is one to pop, otherwise it is kept (there is nothing left under this path to collapse into,
/// e.g. `..` past a root or past another un-collapsed `..`).
fn normalize_lexically(path: &Path) -> PathBuf {
    use std::path::Component;

    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(result.components().next_back(), Some(Component::Normal(_))) {
                    result.pop();
                } else {
                    result.push("..");
                }
            }
            other => result.push(other.as_os_str()),
        }
    }
    result
}

pub(super) fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// A `rules[].exclude`/top-level `exclude` entry is a glob (full-path, anchored, engine-side) when it
/// carries a glob metacharacter; otherwise it is a plain substring filter. `[`/`]` are deliberately
/// NOT glob characters so raw Next.js dynamic-segment paths like `app/[locale]/` stay substring
/// matches instead of being (mis)parsed as a character class.
pub(super) fn is_glob_pattern(s: &str) -> bool {
    s.chars().any(|c| matches!(c, '*' | '?' | '{' | '}'))
}
