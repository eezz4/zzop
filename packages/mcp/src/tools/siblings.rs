//! Sibling-directory scope disclosure for the `cross_repo` summary — the live-fire gap this closes:
//! a monorepo's `e2e/` tree (1,693 files) was simply never passed to the join and NOTHING said so,
//! because the tool reports only on trees it was handed and never enumerates what it didn't see.
//! When every analyzed tree root sits under ONE common parent directory, that parent's remaining
//! immediate subdirectories are a knowable, factual "not part of this join" set — so we say it.
//! This is a DISCLOSURE, never a recommendation engine: no common parent means no guess and no
//! warning, and the wording states only what exists and what was not analyzed.

use std::collections::BTreeSet;
use std::path::PathBuf;

/// Cap on sibling names spelled out in the warning text; the remainder is disclosed as `(+k more)`
/// — a presentation bound in the same never-silent style as the output caps in `crate::output`.
const MAX_NAMED_SIBLINGS: usize = 5;

/// Returns a ready-to-push `configWarnings`-style entry when ALL analyzed tree roots share one
/// common parent directory AND that parent holds immediate subdirectories that are not any analyzed
/// root (dot-prefixed directories and `node_modules` excluded). `None` whenever the roots do not
/// share a single parent (never guess a scope), the parent is unreadable, or the analyzed set
/// already covers every sibling. Names are sorted for determinism (`read_dir` order is
/// OS-dependent) and capped at `MAX_NAMED_SIBLINGS` with the remainder counted.
pub(super) fn sibling_scope_warning(roots: &[PathBuf]) -> Option<String> {
    let parent = roots.first()?.parent()?;
    if roots.iter().any(|r| r.parent() != Some(parent)) {
        return None;
    }
    // On Windows the caller-supplied casing may differ from the on-disk casing (`./FE` vs `fe`) —
    // a byte-exact compare would then disclose an analyzed root as its own "unanalyzed sibling".
    // Compare case-insensitively there; on case-sensitive filesystems byte-exact stays correct
    // (two dirs differing only by case are genuinely distinct trees).
    let fold = |s: &std::ffi::OsStr| -> String {
        let s = s.to_string_lossy().into_owned();
        if cfg!(windows) {
            s.to_lowercase()
        } else {
            s
        }
    };
    let analyzed: BTreeSet<String> = roots
        .iter()
        .filter_map(|r| r.file_name())
        .map(fold)
        .collect();
    let mut siblings: Vec<String> = std::fs::read_dir(parent)
        .ok()?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .filter(|entry| !analyzed.contains(&fold(&entry.file_name())))
        .filter_map(|entry| entry.file_name().to_str().map(String::from))
        .filter(|name| !name.starts_with('.') && name != "node_modules")
        .collect();
    siblings.sort();
    if siblings.is_empty() {
        return None;
    }
    let mut named = siblings
        .iter()
        .take(MAX_NAMED_SIBLINGS)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if siblings.len() > MAX_NAMED_SIBLINGS {
        named.push_str(&format!(" (+{} more)", siblings.len() - MAX_NAMED_SIBLINGS));
    }
    let (noun, verb) = if siblings.len() == 1 {
        ("directory", "is")
    } else {
        ("directories", "are")
    };
    Some(format!(
        "{} sibling {noun} under {} {verb} not part of this join: {named} — pass them as paths or add them to the config's trees",
        siblings.len(),
        parent.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::sibling_scope_warning;
    use std::path::PathBuf;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(prefix: &str) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }

        fn mkdir(&self, rel: &str) -> PathBuf {
            let p = self.0.join(rel);
            std::fs::create_dir_all(&p).unwrap();
            p
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn unanalyzed_siblings_are_disclosed_with_the_name_cap_and_remainder() {
        let parent = TempDir::new("zzop-mcp-siblings-cap");
        let fe = parent.mkdir("fe");
        let be = parent.mkdir("be");
        for name in ["e2e", "pkg-a", "pkg-b", "pkg-c", "pkg-d", "pkg-e", "pkg-f"] {
            parent.mkdir(name);
        }
        parent.mkdir(".git");
        parent.mkdir("node_modules");
        let w = sibling_scope_warning(&[fe, be]).expect("siblings must be disclosed");
        assert!(
            w.starts_with("7 sibling directories under"),
            "dot-dirs and node_modules never count: {w}"
        );
        assert!(
            w.contains(": e2e, pkg-a, pkg-b, pkg-c, pkg-d (+2 more) — "),
            "sorted, capped at 5, remainder counted: {w}"
        );
        assert!(
            w.ends_with("pass them as paths or add them to the config's trees"),
            "got: {w}"
        );
        assert!(
            !w.contains(".git") && !w.contains("node_modules"),
            "got: {w}"
        );
    }

    #[test]
    fn one_sibling_reads_singular() {
        let parent = TempDir::new("zzop-mcp-siblings-one");
        let fe = parent.mkdir("fe");
        let be = parent.mkdir("be");
        parent.mkdir("e2e");
        let w = sibling_scope_warning(&[fe, be]).expect("the sibling must be disclosed");
        assert!(
            w.starts_with("1 sibling directory under")
                && w.contains("is not part of this join: e2e"),
            "got: {w}"
        );
    }

    #[test]
    fn roots_without_one_common_parent_stay_silent() {
        // Never guess: two roots under DIFFERENT parents define no knowable sibling scope.
        let a = TempDir::new("zzop-mcp-siblings-parent-a");
        let b = TempDir::new("zzop-mcp-siblings-parent-b");
        let fe = a.mkdir("fe");
        let be = b.mkdir("be");
        a.mkdir("e2e");
        assert_eq!(sibling_scope_warning(&[fe, be]), None);
    }

    #[test]
    fn an_analyzed_set_covering_every_sibling_stays_silent() {
        let parent = TempDir::new("zzop-mcp-siblings-complete");
        let fe = parent.mkdir("fe");
        let be = parent.mkdir("be");
        assert_eq!(sibling_scope_warning(&[fe, be]), None);
    }

    #[test]
    fn files_next_to_the_roots_are_not_directories_and_stay_silent() {
        // A zzop.config.jsonc (or any file) sitting in the parent is not a sibling DIRECTORY.
        let parent = TempDir::new("zzop-mcp-siblings-files");
        let fe = parent.mkdir("fe");
        let be = parent.mkdir("be");
        std::fs::write(parent.0.join("zzop.config.jsonc"), "{}").unwrap();
        assert_eq!(sibling_scope_warning(&[fe, be]), None);
    }
}
