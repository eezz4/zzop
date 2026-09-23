//! Repo-relative POSIX path arithmetic — dirname, ancestor walk, and join+normalize.
//!
//! Separate from `paths.rs` on a real seam: that module answers *questions about* a path (is it a test
//! file, is it a build artifact), this one *computes* paths. Both are "shared path helpers", and
//! folding them together would produce one module whose name explains nothing.
//!
//! ## Why these three are public (2026-09-07, review ledger V93 ⑵⑶)
//!
//! Each had two byte-identical copies inside `zzop_engine`, and each carried a comment justifying the
//! copy by the same argument: *the original is a private helper with no public home.* That argument
//! was true and self-perpetuating — nobody could import what nobody had promoted, so the third caller
//! copied it too. Giving them a public home is the whole fix; the copies are deleted, not synchronized.
//!
//! All three treat `""` as the tree root, which is what makes them composable: `dirname` of a
//! root-level file is `""`, and `join_and_normalize("", x)` is `x`.

/// The POSIX dirname of a repo-relative path, or `""` when the path has no directory component.
///
/// Returns `""` rather than `"."` for a root-level file, deliberately: `""` is the identity element
/// for [`join_and_normalize`], so a root-level path composes without a special case.
pub fn dirname(p: &str) -> &str {
    match p.rfind('/') {
        Some(i) => &p[..i],
        None => "",
    }
}

/// Every ancestor directory of `rel` — its own dirname, then each parent up to and including the tree
/// root `""` — most specific first.
///
/// The trailing `""` is always pushed even when `dirname(rel)` was already `""`, so a root-level path
/// yields `["", ""]`. Callers dedup (typically via a `BTreeSet`), which is why the duplicate is
/// harmless and why removing it would be a behaviour change, not a cleanup.
pub fn ancestor_dirs(rel: &str) -> Vec<String> {
    let mut dir = dirname(rel).to_string();
    let mut out = vec![dir.clone()];
    while let Some(idx) = dir.rfind('/') {
        dir.truncate(idx);
        out.push(dir.clone());
    }
    out.push(String::new());
    out
}

/// POSIX join + `.`/`..`-segment normalize, relative to `dir` (`""` = tree root).
///
/// A leading `/` on `candidate` is absorbed by the empty-segment rule, so an absolute string simply
/// fails to resolve rather than escaping the tree. A `..` that would climb above the root is kept as a
/// literal `..` segment for the same reason: the result stays a non-matching repo-relative path
/// instead of becoming a path outside the tree.
pub fn join_and_normalize(dir: &str, candidate: &str) -> String {
    let joined = if dir.is_empty() {
        candidate.to_string()
    } else {
        format!("{dir}/{candidate}")
    };
    let mut stack: Vec<&str> = Vec::new();
    for seg in joined.split('/') {
        match seg {
            "" | "." => continue,
            ".." => {
                if matches!(stack.last(), Some(&s) if s != "..") {
                    stack.pop();
                } else {
                    stack.push("..");
                }
            }
            s => stack.push(s),
        }
    }
    stack.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dirname_of_a_root_level_file_is_the_empty_root() {
        assert_eq!(dirname("package.json"), "");
        assert_eq!(dirname("apps/web/package.json"), "apps/web");
    }

    #[test]
    fn the_root_is_the_join_identity() {
        assert_eq!(join_and_normalize("", "src/index.ts"), "src/index.ts");
        assert_eq!(
            join_and_normalize(dirname("package.json"), "dist/main.js"),
            "dist/main.js"
        );
    }

    #[test]
    fn dot_and_dotdot_segments_normalize() {
        assert_eq!(
            join_and_normalize("apps/web", "./src/a.ts"),
            "apps/web/src/a.ts"
        );
        assert_eq!(
            join_and_normalize("apps/web", "../lib/b.ts"),
            "apps/lib/b.ts"
        );
        assert_eq!(join_and_normalize("apps/web", "../../c.ts"), "c.ts");
    }

    /// An absolute or over-climbing candidate must stay a repo-relative non-match, never escape.
    ///
    /// Note what "absorbed" actually means for a leading `/`: the slash becomes an EMPTY segment, which
    /// is skipped — so `/etc/passwd` under `apps` resolves to `apps/etc/passwd`, not `etc/passwd`. It is
    /// still confined to the tree, which is the property that matters, but it does not rebase to the
    /// root. (This assertion was written the other way first and the test caught it immediately.)
    #[test]
    fn nothing_escapes_the_tree() {
        assert_eq!(join_and_normalize("apps", "/etc/passwd"), "apps/etc/passwd");
        assert_eq!(join_and_normalize("", "/etc/passwd"), "etc/passwd");
        // Climbing above the root keeps the `..` literal, so the result stays a non-matching
        // repo-relative path rather than becoming a real parent-directory reference.
        assert_eq!(join_and_normalize("", "../../secret"), "../../secret");
    }

    #[test]
    fn ancestors_run_most_specific_first_and_end_at_the_root() {
        assert_eq!(
            ancestor_dirs("apps/web/src/index.ts"),
            vec!["apps/web/src", "apps/web", "apps", ""]
        );
    }

    /// A root-level file yields `""` twice. Pinned rather than fixed: callers dedup, and the callers
    /// that walk this list rely on the root being present.
    #[test]
    fn a_root_level_path_repeats_the_root() {
        assert_eq!(ancestor_dirs("go.mod"), vec!["", ""]);
    }
}
