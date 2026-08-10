//! The resolution rule for [`VocabularyConfig::extra_test_path_patterns`] — the one key in that struct
//! whose declaration ADDS to a built-in rather than replacing it, so the one that cannot go through
//! [`super::resolved::ResolvedVocabulary`] (whose whole job is the opposite normalization). Split from
//! `vocabulary.rs` for the repo per-file line cap, on the seam the exception already draws; the parent
//! module doc owns the contract prose and the field doc owns the argument.

use super::VocabularyConfig;

/// The declared extra test-path arms folded into ONE alternation, plus a self-report line for every
/// declared pattern that could not be used. `None` means there is nothing to add — no declaration, or a
/// declaration in which nothing compiled — and the run then judges by the built-in language conventions
/// alone, which is the documented default rather than a degradation.
///
/// ## Why an invalid pattern is DROPPED here rather than left for the matcher to choke on
/// A `file_exclude_pattern` that fails to compile makes its whole rule silent (zero findings — see
/// `docs/rules/dsl-reference.md`). Splicing an author's typo into the exclusion of every rule that
/// references the shared vocabulary — 132 of the 144 bundled ones, measured 2026-08-10 — would turn one
/// bad character in a config file into that many rules reporting nothing, with a green run and no
/// finding to notice. Compiling each arm on its own, keeping the ones that work and NAMING the ones that
/// do not, is the same contract `GitOptions::commit_type_patterns` states for the same reason.
///
/// Each arm is wrapped `(?:…)` before joining so an author's own top-level `|` cannot rewrite the
/// grouping of the arms beside it.
pub(crate) fn extra_test_path_tail(vocab: &VocabularyConfig) -> (Option<String>, Vec<String>) {
    let mut usable: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    for pattern in &vocab.extra_test_path_patterns {
        if pattern.is_empty() {
            continue;
        }
        match regex::Regex::new(pattern) {
            Ok(_) => usable.push(format!("(?:{pattern})")),
            Err(e) => warnings.push(format!(
                "vocabulary.extraTestPathPatterns: \"{pattern}\" is not a valid regex ({e}) — it adds \
                 nothing to the built-in test-path conventions this run declines. Every other declared \
                 pattern in the list still applies."
            )),
        }
    }
    let tail = (!usable.is_empty()).then(|| usable.join("|"));
    (tail, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_undeclared_or_empty_vocabulary_adds_nothing_and_says_nothing() {
        for patterns in [vec![], vec![String::new()]] {
            let (tail, warnings) = extra_test_path_tail(&VocabularyConfig {
                extra_test_path_patterns: patterns.clone(),
                ..VocabularyConfig::default()
            });
            assert_eq!(tail, None, "{patterns:?}");
            assert!(warnings.is_empty(), "{patterns:?} is not an error");
        }
    }

    /// Each arm keeps its own group, so an author's top-level `|` cannot swallow the arm beside it.
    #[test]
    fn each_declared_arm_is_grouped_before_the_alternation_is_built() {
        let (tail, warnings) = extra_test_path_tail(&VocabularyConfig {
            extra_test_path_patterns: vec!["a|b".to_string(), "(^|/)it/".to_string()],
            ..VocabularyConfig::default()
        });
        assert_eq!(tail.as_deref(), Some("(?:a|b)|(?:(^|/)it/)"));
        assert!(warnings.is_empty());
    }

    #[test]
    fn an_uncompilable_arm_is_dropped_by_name_and_its_siblings_survive() {
        let (tail, warnings) = extra_test_path_tail(&VocabularyConfig {
            extra_test_path_patterns: vec!["([unclosed".to_string(), "(^|/)it/".to_string()],
            ..VocabularyConfig::default()
        });
        assert_eq!(tail.as_deref(), Some("(?:(^|/)it/)"));
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("([unclosed"), "{warnings:?}");
        assert!(
            warnings[0].contains("extraTestPathPatterns"),
            "the warning must name the key a reader can go fix: {warnings:?}"
        );
    }

    /// The degenerate case worth pinning separately: when NOTHING compiles the result is `None`, not an
    /// empty string. An empty alternation spliced into 132 exclusions would be a regex matching every
    /// path — silence everywhere, from a typo.
    #[test]
    fn a_declaration_in_which_nothing_compiles_yields_no_tail_at_all() {
        let (tail, warnings) = extra_test_path_tail(&VocabularyConfig {
            extra_test_path_patterns: vec!["([unclosed".to_string()],
            ..VocabularyConfig::default()
        });
        assert_eq!(tail, None);
        assert_eq!(warnings.len(), 1);
    }
}
