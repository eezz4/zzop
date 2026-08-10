//! Declared-vocabulary normalizers — the transforms a rule applies to an INPUT NAME before comparing it
//! against a `vocabulary.*` list, defined once so the comparison and the config front end's
//! "can this entry ever match?" check cannot disagree.
//!
//! ## Why these live in the kernel
//! Same reasoning as [`crate::io::key::db_table_channel_casing`], applied to a different axis: the
//! functions have consumers in crates that cannot depend on each other (`zzop-rules-cross-layer`,
//! `zzop-rules-schema`, `zzop-parser-typescript`) plus a reader in the config front end, and all of them
//! already depend on this crate. A local twin in each would make agreement a CONVENTION; a shared symbol
//! makes drift a compile error.
//!
//! **This is not vocabulary and does not breach kernel ignorance.** Nothing here names a rule, a
//! framework, a config key, or a token — these are text transforms. WHICH transform a given
//! `vocabulary.*` key gets is a rule-side fact, and it is declared rule-side
//! (`zzop_engine::vocabulary::normalizers`), not here.
//!
//! ## The failure this closes
//! A declared list was compared VERBATIM against a normalized input, and nothing checked that the
//! declared spelling could survive that normalization. Declaring `sensitiveResponseFieldExactNames:
//! ["sessionToken"]` therefore produced a permanently silent rule with no warning: the input side
//! normalized `sessionToken` to `sessiontoken`, the declared side stayed `sessionToken`, and the two
//! could never be equal. The user believed a protection was on while it was off — the failure mode this
//! repo ranks first.
//!
//! ## Two transforms, and the ASCII/Unicode difference between them is INHERITED
//! [`ascii_lowercase`] folds ASCII case only; [`unicode_lowercase_without_separators`] folds Unicode case
//! AND drops separators. The separator half is a real judgment (see each function's doc). The ASCII-vs-
//! Unicode half is not — it is where two authors happened to land, and it means a non-ASCII name folds on
//! one axis and not the other. It is preserved rather than smoothed because collapsing it would CHANGE
//! DETECTION on a non-ASCII name, and a detection move belongs in a batch that re-measures the corpus,
//! not in one that adds a warning. The pin in this module's tests is what keeps the difference visible
//! until that batch happens.

/// ASCII-only lowercase, no separator handling.
///
/// Consumers: `zzop_rules_cross_layer`'s `external-secret-in-url` query-parameter name test,
/// `zzop_rules_schema`'s `float-money` field-name test, and `zzop_parser_typescript`'s
/// idempotency-header literal test.
///
/// Separators are NOT stripped, and for the query-parameter consumer that is load-bearing rather than an
/// omission: `api_key` and `api-key` are both legitimate entries because a URL query parameter really can
/// be spelled either way, so folding them together would merge two distinct declarations into one. The
/// shipped built-in list carries both spellings for exactly that reason.
pub fn ascii_lowercase(name: &str) -> String {
    name.to_ascii_lowercase()
}

/// Unicode-aware lowercase with `_` and `-` removed, so `password_hash`, `PASSWORD-HASH` and
/// `passwordHash` all reduce to `passwordhash` — one spelling for the vocabulary to judge.
///
/// Consumer: `zzop_rules_cross_layer`'s `sensitive-response-field` name test (all three of its axes).
///
/// The stripping is right HERE and wrong for [`ascii_lowercase`] because the subject differs: a response
/// FIELD name is one identifier written in whatever case convention its language prefers, where a URL
/// query parameter is a wire token whose separator is part of the token.
pub fn unicode_lowercase_without_separators(name: &str) -> String {
    name.chars()
        .filter(|c| *c != '_' && *c != '-')
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_lowercase_keeps_separators_because_both_spellings_are_real() {
        assert_eq!(ascii_lowercase("API_KEY"), "api_key");
        assert_eq!(ascii_lowercase("Api-Key"), "api-key");
        assert_eq!(ascii_lowercase("Idempotency-Key"), "idempotency-key");
    }

    #[test]
    fn separator_stripping_folds_every_case_convention_to_one_spelling() {
        for spelling in [
            "password_hash",
            "PASSWORD-HASH",
            "passwordHash",
            "PasswordHash",
        ] {
            assert_eq!(
                unicode_lowercase_without_separators(spelling),
                "passwordhash",
                "{spelling} must fold"
            );
        }
    }

    #[test]
    fn the_inherited_ascii_vs_unicode_difference_is_pinned_not_assumed_away() {
        // Module doc's "INHERITED" section. A batch that unifies the two has to delete this assertion
        // deliberately, with a corpus re-measurement, rather than discover the split by surprise.
        assert_eq!(ascii_lowercase("TOTALÉ"), "totalÉ");
        assert_eq!(unicode_lowercase_without_separators("TOTALÉ"), "totalé");
    }

    #[test]
    fn every_normalizer_is_idempotent() {
        // The config front end's check is `normalize(entry) != entry`, which only means "this entry can
        // never match" if normalizing is a fixed point on already-normalized input. Without this, a
        // correctly-declared entry could warn forever.
        for f in [
            ascii_lowercase as fn(&str) -> String,
            unicode_lowercase_without_separators,
        ] {
            for s in [
                "passwordHash",
                "API_KEY",
                "Idempotency-Key",
                "",
                "x",
                "TOTALÉ",
            ] {
                let once = f(s);
                assert_eq!(f(&once), once, "not idempotent on {s:?}");
            }
        }
    }
}
