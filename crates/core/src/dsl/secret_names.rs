//! The NAME VOCABULARY `security/hardcoded-secret` judges by, and the one place it is spelled.
//!
//! # Why this constant is here and not only in the pack
//! Four matcher arms in `rules/dsl/security/security.json` carry the same name alternation. Until
//! 2026-09-15 that was its only home, which made it unreachable from a config: a project whose secrets
//! are called something else could declare `vocabulary.secretNames` all day and the rule would go on
//! judging by its own seven. U103 measured that gap by declaring six names and watching three of them
//! be silently ignored — and `configWarnings` come back `[]`.
//!
//! So the pattern text needs an owner OUTSIDE the pack JSON, because two readers now need it: the
//! rewriter below (which has to FIND the alternation in order to replace it) and
//! `zzop_engine::VocabularyConfig::built_in` (which ships these names into the starter config so a
//! `zzop init` project behaves exactly as it does today). The pack still spells the text inline —
//! `${NAME}` substitutes only as a WHOLE value and this alternation sits mid-pattern — so the copy is
//! guarded rather than avoided: [`tests`] asserts the shipped pack carries this exact body in exactly
//! [`PACK_ARMS`] patterns, which is the same trade `fragments::is_shared_test_path_vocabulary` makes.
//!
//! # Why the list is these seven
//! 📏 Measured 2026-09-15 against the 17-tree corpus, on the question this rule actually asks (a NAMED
//! BINDING, not a query parameter). Every name the shipped `vocabulary.secretParamNames` carries beyond
//! this set was tried and produced no credential:
//!
//! | candidate | matches | what they were |
//! |---|---|---|
//! | `key` | 57 in 36 files | the rule's own standing negative already measured this: 46 findings, exactly ONE credential |
//! | `accesstoken` | 5 in 3 files | an enum member naming a cookie, and four test-fixture mocks |
//! | `signature` | 3 in 2 files | TensorFlow `signature=` keyword arguments — a different word |
//! | `auth` | 1 | a Redux action-type constant whose value IS its name |
//! | `apitoken`, `jwt` | 0 | nothing on this corpus, so nothing to weigh |
//!
//! Recount: `grep -rP '(?i)(?:^|[^A-Za-z0-9])(<name>)["'"'"']?\s*(?::=|[:=])\s*["'"'"'][^\s"'"'"']{8,}["'"'"']'`
//! over `corpus/oss`, restricted to source extensions. That is the line-scan arm without the rule's
//! value-shape gate, so each count is an UPPER BOUND — a name quiet there cannot be loud here.
//!
//! 🔴 The reason the reuse was proposed at all is worth keeping: `secretParamNames` is named for a
//! QUERY PARAMETER (its own template comment says so) and `cross-layer/secret-in-url` reads it, where a
//! bare `key` in a querystring IS a signal. One name was serving two questions. The fix is two names.

/// The built-in alternation, verbatim as the shipped pack spells it in all [`PACK_ARMS`] arms.
pub const BUILT_IN_ALTERNATION: &str =
    "secret[_-]?key|api[_-]?key|apikey|secret|passwd|password|token";

/// How many shipped matcher patterns carry [`group`]. A FLOOR the guard checks in both directions:
/// fewer means a rule was reworded and silently stopped receiving declarations, more means a new arm
/// joined and nobody looked at whether it should.
pub const PACK_ARMS: usize = 4;

/// The alternation WITH its capture parentheses — the form both the rewriter and its guard match on.
///
/// 🔴 Bare-substring matching is not enough, and that was measured rather than reasoned: the first
/// version of this seam matched [`BUILT_IN_ALTERNATION`] alone, and an invalidation drill that
/// appended one name to a single arm (`…|token|bearer`) left the arm count at 4 and the guard GREEN.
/// Worse than a missed alarm, the rewrite would then have produced `[^\s\S]|bearer` — a pattern that
/// still matches `bearer` — so a config declaring its own names would have silently kept one built-in
/// the pack author had just added. Anchoring on the closing paren makes an appended name a parse-level
/// mismatch: the count drops, and the guard says which arm stopped being reachable.
pub fn group() -> String {
    format!("({BUILT_IN_ALTERNATION})")
}

/// The same seven, expanded to the plain NAMES a config author writes — `[_-]?` spelled out, because a
/// declaration is a list of names and not a list of regexes. `built_in()` ships exactly this.
pub const BUILT_IN_NAMES: &[&str] = &[
    "secret_key",
    "secretkey",
    "secret-key",
    "api_key",
    "apikey",
    "api-key",
    "secret",
    "passwd",
    "password",
    "token",
];

/// Declared names -> the alternation that replaces [`BUILT_IN_ALTERNATION`] in the pack.
///
/// Every name is regex-ESCAPED, so a config cannot reshape the rule's pattern — a declaration says
/// which names are secrets, never how the surrounding regex behaves. Empty entries are dropped; an
/// empty result is `None`, which the caller reads as "this judgment was not asked for".
///
/// Sorted LONGEST FIRST. Rust's regex crate is leftmost-first over alternations, so `secret` ahead of
/// `secret_key` would capture the shorter name on a `SECRET_KEY = "…"` line. Matching still succeeds
/// either way (the surrounding pattern forces a backtrack), but the captured group is what the finding
/// reports, and a finding that names `secret` where the source says `SECRET_KEY` is a small lie the
/// ordering costs nothing to avoid.
pub fn alternation_from(names: &[String]) -> Option<String> {
    let mut usable: Vec<&str> = names
        .iter()
        .map(String::as_str)
        .filter(|n| !n.trim().is_empty())
        .collect();
    if usable.is_empty() {
        return None;
    }
    usable.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));
    usable.dedup();
    Some(
        usable
            .iter()
            .map(|n| regex::escape(n))
            .collect::<Vec<_>>()
            .join("|"),
    )
}

#[cfg(test)]
mod tests;
