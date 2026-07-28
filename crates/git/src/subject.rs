//! Declared subject-pattern matching — the commit-subject axis that has NO built-in vocabulary.
//!
//! [`tags`](crate::tags) classifies a commit's TYPE and ships with the notion of a default table (the
//! caller supplies one; `zzop_metrics::default_commit_type_patterns` is the shipped default). This
//! module is deliberately the opposite: it matches ONLY what the caller declared in
//! `CollectOptions::commit_subject_patterns` (config `git.commitSubjectPatterns`), and an empty
//! declaration yields an empty result — never a fallback, never a heuristic. What a "revert", a
//! "ticket id" or a "hotfix" subject looks like differs per project, so any built-in table here would
//! be the engine guessing at a convention it cannot know, and silently mislabeling every project whose
//! convention differs. Declarations are read; nothing is inferred.
//!
//! Two further consequences of "read the declaration verbatim":
//! - **No implicit regex flags.** [`compile_commit_subject_pattern`] compiles the pattern source
//!   EXACTLY as written, unlike [`crate::compile_commit_type_pattern`], which prepends `(?i)`. A
//!   case-insensitive subject pattern is spelled `(?i)` by its author — deciding for them that case
//!   does not matter is the same class of assumption this whole module exists to refuse. (An author
//!   coming from `commitTypePatterns` who wants the old behavior writes the two characters.)
//! - **The whole subject, unstripped.** `tags` strips leading `[scope]` groups before running its
//!   classifiers; this matches against the raw subject, so a pattern anchored with `^` means the real
//!   start of the real subject.

use regex::Regex;

/// Compiles one `git.commitSubjectPatterns` regex source. The single source of the compile semantics
/// for this axis: both the USE site ([`SubjectMatchers::compile`]) and any DETECT site (the engine's
/// declared-pattern validator, which cannot name `SubjectMatchers` — it is `pub(crate)` here) call
/// this, so their verdicts on a given pattern can never drift. Deliberately NOT
/// [`crate::compile_commit_type_pattern`]: no `(?i)` (or any other) flag is injected — see the module
/// doc.
pub fn compile_commit_subject_pattern(src: &str) -> Result<Regex, regex::Error> {
    Regex::new(src)
}

/// Compiled (regex, label) declarations, in declaration order. A pattern that fails to compile is
/// dropped here (inert — matches nothing, never a panic); the engine separately warns naming it, so
/// the degradation is never silent.
pub(crate) struct SubjectMatchers(Vec<(Regex, String)>);

impl SubjectMatchers {
    pub(crate) fn compile(patterns: &[(String, String)]) -> SubjectMatchers {
        SubjectMatchers(
            patterns
                .iter()
                .filter_map(|(src, label)| {
                    compile_commit_subject_pattern(src)
                        .ok()
                        .map(|re| (re, label.clone()))
                })
                .collect(),
        )
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Every declared label whose pattern matches `subject`, in DECLARATION order with the first
    /// occurrence of a repeated label kept — order is a property of the caller's table, not of match
    /// position or of a hash iteration, so the same table over the same history always yields the
    /// same vector. Unlike `tags`'s classifiers this is NOT first-match-wins: a subject can carry
    /// several declared aspects at once (a revert that also names a ticket), and dropping all but
    /// the first would discard facts the author explicitly asked for.
    pub(crate) fn labels(&self, subject: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for (re, label) in &self.0 {
            if re.is_match(subject) && !out.iter().any(|l| l == label) {
                out.push(label.clone());
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matchers(pairs: &[(&str, &str)]) -> SubjectMatchers {
        SubjectMatchers::compile(
            &pairs
                .iter()
                .map(|(p, l)| (p.to_string(), l.to_string()))
                .collect::<Vec<_>>(),
        )
    }

    /// Seals never-guess at the mechanism level: an empty declaration table classifies NOTHING, for
    /// subjects that any plausible built-in vocabulary would have matched.
    #[test]
    fn no_declaration_means_no_label_at_all() {
        let m = matchers(&[]);
        assert!(m.is_empty());
        assert!(m.labels("Revert \"add caching\"").is_empty());
        assert!(m.labels("hotfix: PROJ-42 null deref").is_empty());
        assert!(m.labels("").is_empty());
    }

    /// Seals that a declared pattern really matches, and that several declarations can label one
    /// subject (this axis is not first-match-wins).
    #[test]
    fn declared_patterns_match_and_accumulate_in_declaration_order() {
        let m = matchers(&[
            (r"^Revert\b", "revert"),
            (r"PROJ-\d+", "ticket"),
            (r"\bnever\b", "unmatched"),
        ]);
        assert_eq!(
            m.labels("Revert \"PROJ-42 add caching\""),
            vec!["revert".to_string(), "ticket".to_string()]
        );
        assert_eq!(m.labels("PROJ-7 tidy"), vec!["ticket".to_string()]);
    }

    /// Seals that a repeated label collapses to one entry at its FIRST declared position — the
    /// output is a function of the table's order, so it is stable run to run.
    #[test]
    fn repeated_label_is_deduped_at_its_first_declared_position() {
        let m = matchers(&[
            (r"^Revert\b", "revert"),
            (r"^Roll ?back\b", "rollback"),
            (r"caching", "revert"),
        ]);
        assert_eq!(m.labels("Revert caching"), vec!["revert".to_string()]);
    }

    /// Seals that the declaration is read VERBATIM: no `(?i)` is injected (unlike the commit-type
    /// axis), and the author's own inline flag is what turns case-insensitivity on.
    #[test]
    fn no_implicit_case_insensitivity_the_author_declares_it() {
        assert!(matchers(&[(r"^Revert\b", "revert")])
            .labels("revert the thing")
            .is_empty());
        assert_eq!(
            matchers(&[(r"(?i)^revert\b", "revert")]).labels("Revert the thing"),
            vec!["revert".to_string()]
        );
    }

    /// Seals that a malformed declaration is inert rather than fatal — it drops out of the compiled
    /// table (the engine warns about it separately) and the valid siblings keep working.
    #[test]
    fn an_uncompilable_pattern_is_dropped_not_panicked_on() {
        let m = matchers(&[("[unclosed", "bad"), (r"^ok\b", "good")]);
        assert_eq!(m.labels("ok then"), vec!["good".to_string()]);
        assert!(m.labels("[unclosed anything").is_empty());
    }

    /// Seals that matching runs against the RAW subject — `tags`'s leading-`[scope]` stripping is not
    /// applied here, so `^` means the true start of the subject as git emitted it.
    #[test]
    fn matches_the_raw_subject_without_scope_stripping() {
        let m = matchers(&[(r"^\[ui\]", "ui")]);
        assert_eq!(m.labels("[ui] tweak button"), vec!["ui".to_string()]);
        assert!(matchers(&[(r"^tweak", "t")])
            .labels("[ui] tweak button")
            .is_empty());
    }
}
