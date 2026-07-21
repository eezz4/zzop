//! Commit-tag extraction: an uppercase `[TAG]` bracket grammar (authoritative when present) falling
//! back to an ordered set of (regex, TAG) commit-type classifiers applied to the subject with any
//! leading `[scope]` prefix stripped. REVERT is classified before FIX/FEAT so a reverted change is not
//! miscounted as the change it reverts.
//!
//! This module owns the classification *mechanism* only ([`extract_tags`] / [`CommitClassifiers`]) —
//! the default FIX/FEAT/... vocabulary itself lives in `zzop_metrics::default_commit_type_patterns`
//! (a collection crate must not own analysis vocabulary). Callers pass their own table via
//! `CollectOptions::commit_type_patterns`;
//! [`CollectOptions::default`](crate::CollectOptions) ships an empty table (bracket tags only) since
//! this crate has no vocabulary of its own to default to. This module's tests use a small local copy
//! of that table (`test_commit_type_patterns`, `#[cfg(test)]`-only) rather than depending on
//! `zzop-metrics` — this crate must stay a leaf collector with no upward dependency on an analysis
//! crate.

use regex::Regex;

use crate::compile_commit_type_pattern;

/// Compiled (regex, tag) classifiers, in match order, matched case-insensitively. Patterns that fail
/// to compile are skipped (defensive — the built-in table above always compiles; a caller-injected
/// pattern should not be able to panic the collector).
pub(crate) struct CommitClassifiers(Vec<(Regex, String)>);

impl CommitClassifiers {
    pub(crate) fn compile(patterns: &[(String, String)]) -> CommitClassifiers {
        CommitClassifiers(
            patterns
                .iter()
                .filter_map(|(src, tag)| {
                    compile_commit_type_pattern(src)
                        .ok()
                        .map(|re| (re, tag.clone()))
                })
                .collect(),
        )
    }

    fn classify(&self, body: &str) -> Option<String> {
        self.0
            .iter()
            .find(|(re, _)| re.is_match(body))
            .map(|(_, tag)| tag.clone())
    }
}

/// Extracts uppercase `[TAG]` tokens from a commit subject (`[FIX]`, `[ADD][PERF]`, …), in order of
/// appearance. Empty when none are present.
fn extract_bracket_tags(subject: &str) -> Vec<String> {
    let chars: Vec<char> = subject.chars().collect();
    let mut tags = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '[' && i + 1 < chars.len() && chars[i + 1].is_ascii_uppercase() {
            let start = i + 1;
            let mut j = start + 1;
            while j < chars.len()
                && (chars[j].is_ascii_uppercase() || chars[j].is_ascii_digit() || chars[j] == '_')
            {
                j += 1;
            }
            if j < chars.len() && chars[j] == ']' {
                tags.push(chars[start..j].iter().collect());
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    tags
}

/// Strips one or more leading `[scope]` groups (any content, not just uppercase tags) plus the
/// whitespace trailing each — mirrors `subject.replace(/^(?:\s*\[[^\]]*\]\s*)+/, "")`. Applied only
/// when no `[TAG]` bracket was found, so a bare keyword after a scope prefix (`[ui] fix …`) is still
/// classified.
fn strip_leading_scopes(subject: &str) -> String {
    let mut rest = subject;
    loop {
        let after_ws = rest.trim_start();
        let Some(stripped) = after_ws.strip_prefix('[') else {
            break;
        };
        let Some(end) = stripped.find(']') else {
            break;
        };
        rest = stripped[end + 1..].trim_start();
    }
    rest.to_string()
}

/// Extracts commit tags from a subject: `[TAG]` brackets win when present; otherwise the first
/// matching classifier (in order) after stripping any leading `[scope]` prefix. Empty when neither
/// applies (e.g. `"Merge branch 'master'"`).
pub(crate) fn extract_tags(subject: &str, classifiers: &CommitClassifiers) -> Vec<String> {
    let brackets = extract_bracket_tags(subject);
    if !brackets.is_empty() {
        return brackets;
    }
    let body = strip_leading_scopes(subject);
    classifiers.classify(&body).into_iter().collect()
}

/// Local test-only copy of `zzop_metrics::default_commit_type_patterns`'s table content (byte-identical
/// regex/tag pairs) — this crate must not depend on `zzop-metrics` (see this module's doc), so its own
/// tests exercise `extract_tags`'s keyword-classification path against a table it owns outright rather
/// than importing the production one. `pub(crate)` so `crate::lib`'s test module can reuse it for the
/// one test there that needs real (non-bracket) keyword classification, instead of a third copy.
#[cfg(test)]
pub(crate) fn test_commit_type_patterns() -> Vec<(String, String)> {
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
    //! Coverage for `extract_tags`'s bracket-tag and keyword-classifier grammar, against this module's
    //! own local `test_commit_type_patterns` table (this crate does not depend on `zzop-metrics`, the
    //! production table's new home — see module doc).
    use super::*;

    fn tags(subject: &str) -> Vec<String> {
        let classifiers = CommitClassifiers::compile(&test_commit_type_patterns());
        extract_tags(subject, &classifiers)
    }

    #[test]
    fn extracts_bracket_tags_and_skips_keyword_classification() {
        assert_eq!(tags("[FIX] correct off-by-one"), vec!["FIX"]);
        assert_eq!(tags("[ADD][PERF] new cache"), vec!["ADD", "PERF"]);
        // Would classify as FEAT by keyword, but the bracket tag wins.
        assert_eq!(tags("[CHORE] add new dependency"), vec!["CHORE"]);
    }

    #[test]
    fn conventional_and_natural_classification_without_brackets() {
        let cases: &[(&str, &str)] = &[
            ("Fix issue with 'Bodies.circle' usage.", "FIX"),
            ("fixed Runner.stop docs, closes #586", "FIX"),
            ("fix: missing mousewheel event", "FIX"),
            ("fix(render): pixel ratio", "FIX"),
            ("added pixel ratio scaling to debug stats", "FEAT"),
            ("feat: introduce sleeping", "FEAT"),
            ("Refactoring code - Runner.js error handling", "REFACTOR"),
            ("perf: optimize broadphase", "PERF"),
            ("docs: update readme", "DOCS"),
            ("test: add SAT specs", "TEST"),
            ("chore: bump deps", "CHORE"),
            ("lint", "STYLE"),
            ("Revert \"fix: missing mousewheel event\"", "REVERT"),
            ("revert: introduce sleeping", "REVERT"),
            ("reverted broadphase change", "REVERT"),
        ];
        for (subject, expected) in cases {
            assert_eq!(
                tags(subject),
                vec![expected.to_string()],
                "subject: {subject}"
            );
        }
    }

    #[test]
    fn does_not_misclassify_substrings_or_unrelated_subjects() {
        assert!(!tags("fixture setup for tests").contains(&"FIX".to_string()));
        assert!(!tags("prefix cleanup").contains(&"FIX".to_string()));
        assert!(tags("Merge branch 'master' into forked").is_empty());
    }

    #[test]
    fn bare_keyword_after_scope_prefix_is_found() {
        assert_eq!(tags("[ui] fix button alignment"), vec!["FIX"]);
        assert_eq!(tags("[core] fixed null deref"), vec!["FIX"]);
        assert_eq!(tags("[a][b] fix race condition"), vec!["FIX"]);
        assert_eq!(tags("[ui] add dark mode"), vec!["FEAT"]);
        assert_eq!(tags("[build] bump deps"), vec!["CHORE"]);
    }

    #[test]
    fn uppercase_bracket_stays_authoritative_over_scope_stripping() {
        assert_eq!(tags("[UI] fix button"), vec!["UI"]);
    }

    #[test]
    fn gitmoji_is_not_classified_without_an_injected_pattern() {
        assert!(tags("\u{1f41b} Keep env validation out of the client bundle").is_empty());
        assert!(tags(":bug: missing mousewheel event").is_empty());
    }

    #[test]
    fn injected_commit_type_patterns_override_the_default_table() {
        let classifiers =
            CommitClassifiers::compile(&[(r"^\s*corrige\b".to_string(), "FIX".to_string())]);
        assert_eq!(
            extract_tags("corrige le bug de fuseau horaire", &classifiers),
            vec!["FIX"]
        );
        // The default English vocabulary was replaced, not merged.
        assert!(extract_tags("fix the timezone bug", &classifiers).is_empty());
    }

    #[test]
    fn injected_emoji_pattern_classifies_gitmoji_prefix() {
        let classifiers = CommitClassifiers::compile(&[
            ("^\\s*\u{1f41b}".to_string(), "FIX".to_string()),
            ("^\\s*\u{267b}".to_string(), "REFACTOR".to_string()),
        ]);
        assert_eq!(
            extract_tags("\u{1f41b} fix the bundle", &classifiers),
            vec!["FIX"]
        );
        // U+FE0F (variant selector) trailing the base emoji is tolerated (no end anchor).
        assert_eq!(
            extract_tags("\u{267b}\u{fe0f} Replace poll_views table", &classifiers),
            vec!["REFACTOR"]
        );
    }
}
