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
pub(crate) fn sibling_scope_warning(roots: &[PathBuf]) -> Option<String> {
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
    // Non-prescriptive by design: a live-fire misfire showed this firing on ALTERNATIVE-STACK repos
    // (parallel demo implementations) where following the old "pass them / add them" imperative would
    // have wrecked the join. State only what exists (a knowable fact) and leave the judgment call —
    // same system or not — to the reader.
    Some(format!(
        "{} sibling {noun} under {} {verb} not part of this join: {named}. Add them to the config's trees only if they are part of the same system as the analyzed roots; unrelated or alternative-stack repos (e.g. a parallel demo implementation) should stay out.",
        siblings.len(),
        parent.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::sibling_scope_warning;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(prefix: &str) -> Self {
            // A clock is not a unique name. Windows' `SystemTime` granularity is coarse enough
            // that two threads entering here together read the SAME nanos, and two tests that then
            // `git init` one directory collide inside git's own template copy — a red gate with
            // nothing to do with the change under test. The counter is what makes the name unique;
            // the clock only keeps runs apart, and this file was one of the last without it.
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let dir =
                std::env::temp_dir().join(format!("{prefix}-{}-{nanos}-{n}", std::process::id()));
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
            w.contains(": e2e, pkg-a, pkg-b, pkg-c, pkg-d (+2 more). Add them to the config's trees only if"),
            "sorted, capped at 5, remainder counted: {w}"
        );
        assert!(
            w.ends_with("unrelated or alternative-stack repos (e.g. a parallel demo implementation) should stay out."),
            "wording must be conditional/non-prescriptive, never an imperative to always pass/add: {w}"
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

    /// The CASE-SENSITIVE arm of `fold`. On such a filesystem `FE` and `fe` are two directories,
    /// so analyzing one must still disclose the other; folding unconditionally would hide a real
    /// unanalyzed tree, and this goes red if someone "simplifies" it that way.
    ///
    /// 🔴 IT PROBES THE FILESYSTEM INSTEAD OF ASSUMING ONE, because the first version of this test
    /// assumed and was wrong: macOS ships a case-INSENSITIVE volume by default, so `mkdir("FE")`
    /// beside `fe` returns the SAME directory and the assertion failed on a correct implementation.
    /// A skip that says why beats a red that means nothing.
    ///
    /// ⚠ DECLARED GAP, stated rather than left for someone to discover: between this skip and the
    /// `cfg!(windows)` arm being compiled out, `fold` has NO arm that is exercised on a developer
    /// mac. Linux CI runs the branch below; the Windows arm is exercised by nothing anywhere.
    #[test]
    #[cfg(not(windows))]
    fn on_a_case_sensitive_filesystem_a_case_variant_sibling_is_still_disclosed() {
        let parent = TempDir::new("zzop-mcp-siblings-case");
        let fe = parent.mkdir("fe");
        parent.mkdir("FE");
        let distinct = std::fs::read_dir(&parent.0)
            .expect("read parent")
            .filter_map(Result::ok)
            .filter(|e| e.path().is_dir())
            .count();
        if distinct < 2 {
            zzop_test_support::skip_notice!(
                "case-insensitive filesystem: `FE` and `fe` are one directory here, so the \
                 case-sensitive arm of `fold` cannot be reached"
            );
            return;
        }
        let warning = sibling_scope_warning(&[fe]).expect("FE is a distinct, unanalyzed directory");
        assert!(
            warning.contains("FE"),
            "case-sensitive fold must keep `FE` and `fe` apart: {warning}"
        );
    }

    /// THE WINDOWS ARM OF `fold` HAD NO TEST AT ALL UNTIL 2026-09-27 (review ledger V447,
    /// Fable round 34), and its absence was the reason gate P5's recommended option bought nothing.
    ///
    /// P5 proposes paying for a Windows CI lane to exercise this one branch. Measured before that
    /// decision: `sibling_scope_warning` had five unconditional tests and none of them varies case,
    /// plus one `cfg(not(windows))` case test — so a Windows run would have executed `fold` five
    /// times without ever reaching the property the Windows arm exists for. The lane would have
    /// proven compilation, which `prebuild.yml` already proves.
    ///
    /// This is the mirror of its `cfg(not(windows))` sibling above: there, `FE` beside `fe` are two
    /// trees and the unanalyzed one must be DISCLOSED; here they are one tree, so analyzing `fe`
    /// covers `FE` and the warning must stay SILENT. Disclosing it would name the root the caller
    /// just analyzed as its own unanalyzed sibling, which is the false report the arm prevents.
    ///
    /// ⚠ This test does not run on the machine that wrote it (macOS) and does not run in CI today
    /// (`ci.yml` is entirely ubuntu). It is written so the P5 lane has something to prove the day it
    /// is bought — stated plainly rather than left for someone to discover that the lane is idle.
    #[test]
    #[cfg(windows)]
    fn on_windows_a_case_variant_of_an_analyzed_root_is_not_disclosed_as_unanalyzed() {
        let parent = TempDir::new("zzop-mcp-siblings-case-win");
        let fe = parent.mkdir("fe");
        // On a case-insensitive filesystem this is the SAME directory as `fe`; the call is kept so
        // the fixture reads the same as its case-sensitive sibling above.
        parent.mkdir("FE");
        let distinct = std::fs::read_dir(&parent.0)
            .expect("read parent")
            .filter_map(Result::ok)
            .filter(|e| e.path().is_dir())
            .count();
        if distinct != 1 {
            zzop_test_support::skip_notice!(
                "case-SENSITIVE filesystem on Windows: `FE` and `fe` are two directories here, so                  the folding arm this test exists for is not the one that runs"
            );
            return;
        }
        // Ask about the root spelled the other way: the caller passed `./FE`, the directory on disk
        // is `fe`, and a byte-exact compare would call the analyzed root an unanalyzed sibling.
        let shouty = parent.0.join("FE");
        assert!(
            sibling_scope_warning(&[shouty]).is_none(),
            "the Windows fold must treat `FE` and `fe` as the same tree, so an analyzed root is              never disclosed as its own unanalyzed sibling"
        );
        let _ = fe;
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
