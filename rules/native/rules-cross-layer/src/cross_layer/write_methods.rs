//! The write-verb vocabulary this crate gates on — its own spelling of a set owned elsewhere.
//!
//! Deliberately the write verbs only: HEAD/OPTIONS never appear in egress extraction (only the verbs
//! `parser-typescript/src/egress.rs` recognizes reach a consume key), so a route can only ever be
//! classified from this set.
//!
//! # Why a named `pub const` rather than an inline `matches!`
//! Because the set must be READABLE from outside. This crate depends on `zzop-core` alone and cannot
//! import `zzop_rules_http::WRITE_HTTP_METHODS`, which owns the set — so the relation between the two
//! spellings is sealed as a T2 pin in `crates/engine/tests/rule_contracts/policy_pins.rs`, the one
//! crate that depends on both. An inline `matches!` arm cannot be pinned at all, which is exactly how
//! this set reached five copies with no comparison between any two of them (2026-07-28 release audit).
//!
//! It lives in its own file because `cross_layer/mod.rs` sat exactly at the 300-line cap; naming the
//! set there would have cost a split anyway.

/// This crate's write-verb set. Pinned equal to `zzop_rules_http::WRITE_HTTP_METHODS` — see module doc.
pub const CROSS_LAYER_WRITE_METHODS: [&str; 4] = ["POST", "PUT", "PATCH", "DELETE"];

/// True for a method that mutates state. Case-sensitive on purpose: every caller reads the method out
/// of an already-normalized io key, so a lowercase verb here would mean the key itself is malformed.
pub(crate) fn is_write_method(method: &str) -> bool {
    CROSS_LAYER_WRITE_METHODS.contains(&method)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_write_verbs_are_matched_and_read_verbs_are_not() {
        for m in CROSS_LAYER_WRITE_METHODS {
            assert!(is_write_method(m), "{m}");
        }
        for m in ["GET", "HEAD", "OPTIONS", "TRACE", ""] {
            assert!(!is_write_method(m), "{m}");
        }
    }

    /// The keys this predicate reads are already normalized to uppercase; if that ever stops being
    /// true this test is the thing that has to change first, deliberately.
    #[test]
    fn a_lowercase_verb_is_not_a_write_method() {
        assert!(!is_write_method("post"));
    }
}
