//! Default commit-type classifier table — analysis vocabulary (regex source, TAG pairs) used by this
//! crate's own score channels (`scores::fix_ratio`, `recommendations`'s FIX-hotspot gate) and by
//! `zzop_engine::analyze::collect_git` as the DEFAULT `zzop_git::CollectOptions::commit_type_patterns`
//! override — used unless a config `git.commitTypePatterns` table (`GitOptions::commit_type_patterns`)
//! replaces it whole for a given run. Lives here rather than in `zzop-git`: that
//! crate owns collection plus the injectable classification *mechanism* only, not the domain vocabulary
//! for what a "FIX" commit looks like. `zzop-git` keeps its own small test-only copy of this table so it
//! need not depend on this crate — see `zzop_git::tags`'s test module doc for why the content is
//! intentionally duplicated there.

/// Default commit-type classifier table (regex source, TAG), in match order. REVERT is checked before
/// FIX/FEAT so a reverted change is not miscounted as the change it reverts — see
/// `zzop_git::tags::extract_tags`'s doc for the full bracket-vs-keyword grammar this table feeds.
pub fn default_commit_type_patterns() -> Vec<(String, String)> {
    [
        (r"^\s*revert(?:ed|s|ing)?\b", "REVERT"),
        (r"^\s*(?:fix(?:ed|es|ing)?|bug\s?fix(?:ed|es)?|hotfix)\b", "FIX"),
        (
            r"^\s*(?:feat(?:ure)?|add(?:ed|s)?|new|implement(?:ed|s|ation)?|introduce[ds]?)\b",
            "FEAT",
        ),
        (
            r"^\s*(?:refactor(?:ed|ing)?|cleanup|clean[\s-]?up|rework(?:ed)?|simplif(?:y|ied|ies))\b",
            "REFACTOR",
        ),
        (r"^\s*(?:perf(?:ormance)?|optimi[sz])", "PERF"),
        (r"^\s*(?:docs?|documentation|readme)\b", "DOCS"),
        (r"^\s*(?:tests?|testing|spec)\b", "TEST"),
        (r"^\s*(?:chore|build|ci|bump|release|version|deps?)\b", "CHORE"),
        (r"^\s*(?:style|lint|format|prettier)\b", "STYLE"),
    ]
    .into_iter()
    .map(|(re, tag)| (re.to_string(), tag.to_string()))
    .collect()
}

#[cfg(test)]
mod tests {
    //! `default_commit_type_patterns` is exercised end-to-end (bracket precedence, scope stripping,
    //! REVERT-before-FIX ordering, pattern override) by `zzop_git::tags`'s tests against its own copy
    //! of this table — this test only pins the shape that moved here, so a future edit to either copy
    //! surfaces as a diff instead of silent drift.
    use super::*;

    #[test]
    fn has_nine_patterns_in_revert_first_order() {
        let patterns = default_commit_type_patterns();
        assert_eq!(patterns.len(), 9);
        assert_eq!(patterns[0].1, "REVERT");
        assert_eq!(patterns[1].1, "FIX");
        assert_eq!(patterns[2].1, "FEAT");
    }

    #[test]
    fn every_pattern_compiles_as_a_case_insensitive_regex() {
        for (src, _tag) in default_commit_type_patterns() {
            regex::Regex::new(&format!("(?i){src}"))
                .unwrap_or_else(|e| panic!("pattern {src:?} failed to compile: {e}"));
        }
    }
}
