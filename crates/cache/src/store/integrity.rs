//! Entry self-hash: does this entry still hold what zzop wrote into it?
//!
//! ## What this defends against, stated exactly
//! Every OTHER staleness axis was already covered, and measured to be: seven separate attempts to make
//! the cache serve a wrong answer — a millisecond-identical mtime with the same byte length, a
//! byte-identical file's cross-file invalidation, truncated / zero-byte / garbage entries, a changed
//! config fingerprint, four concurrent runs — all recomputed correctly. One got through: **hand-writing
//! a well-formed entry**. Replace `findings` with `[]` in `.zzop/cache/findings/<hash>.json`, leave the
//! four fingerprints alone, and a real hardcoded credential still sitting in the source vanishes from
//! every subsequent run. Three re-runs, no self-healing, exit 0, no warning.
//!
//! The reason the existing defenses miss it is structural, not an oversight. Every fingerprint in the
//! key describes the INPUTS (content, parser, scope, vocabulary, ruleset); nothing described the
//! OUTPUT. An entry whose key is untouched and whose payload is a lie is, to a key comparison, a
//! perfectly good entry.
//!
//! ## What this does NOT defend against, equally exactly
//! [`crate::hash::digest128`] is FNV-1a — public, keyless, and computed by code the attacker can read.
//! Anyone able to write a valid entry can also write a valid digest for it. This is a **detector of
//! edits that did not know about the digest**: accidental corruption, a partial overwrite, a
//! hand-edit, a script that rewrote the payload. It is not an authenticity check and must never be
//! described as one. A keyed MAC would be, and would need a key with somewhere to live — a separate
//! decision, and one the threat here does not obviously earn: `.zzop/` is gitignored and local, so this
//! is not a repository-borne supply-chain vector, and anything with local write access has easier
//! targets than the cache.
//!
//! ## Why a mismatch is a MISS and not an error
//! The same reason every other cache-contract violation is: the cache is derived state and recomputing
//! is always available. Failing the run would turn a corrupt byte into an outage; recomputing turns it
//! into a slower run with the right answer. The count rides
//! [`crate::AnalysisCache::rejected_entries`] so the run can still say it happened — a self-heal that
//! leaves no trace is how you get a cache that has been quietly wrong for a month.

/// The byte sequence a payload is DIGESTED as — deliberately NOT the byte sequence the entry file
/// happens to hold.
///
/// A digest is only a corruption detector if the same VALUE always produces the same bytes, and
/// `serde_json::to_vec` of a payload does not: `FileIrSlice::const_map_fragment` is a
/// `std::collections::HashMap`, whose iteration — and therefore whose JSON object-key order — follows a
/// per-instance randomized seed. Writing hashed the order of the map the parser built; reading hashed
/// the order of the map deserialization built, which is a different instance with a different seed. So
/// the detector accused zzop's own writes, on a random subset of entries, on every warm run: measured
/// on a getredash/redash checkout (1289 files, 42 of them carrying a 2+-key `const_map_fragment`),
/// seven consecutive runs over a cache zzop had just written reported 33, 33, 32, 38, 38, 34 and 35
/// refused entries. Two harms, the second worse: those files never converged to a cache hit, and a
/// detector that cries wolf at itself every run is how a reader learns to skip the one run that matters.
///
/// Routing through `serde_json::Value` fixes it at the root rather than at the one field: this crate's
/// `serde_json` has no `preserve_order` feature, so `Value`'s object type is a `BTreeMap` and EVERY
/// object in the payload — including any map some future field introduces — re-emits with its keys in
/// sorted order. A payload built from a `HashMap`, a `BTreeMap`, or read back off disk all canonicalize
/// to identical bytes.
///
/// **What this gives up, exactly**: an on-disk edit that only REORDERS an object's keys no longer
/// registers. That edit does not modify the entry — it deserializes to the identical value, so serving
/// it is serving what zzop wrote (pinned by `a_pure_key_reordering_is_not_treated_as_tampering`).
/// Everything the detector exists for — a changed value, an added or removed key, a reordered or
/// truncated array, a blanked payload — still moves these bytes.
pub(super) fn canonical_payload<T: serde::Serialize + ?Sized>(
    value: &T,
) -> serde_json::Result<Vec<u8>> {
    serde_json::to_vec(&serde_json::to_value(value)?)
}

/// Binds a payload to the key it was stored under. Both halves matter: hashing the payload alone would
/// let a valid entry be COPIED over another key's file (each self-consistent, both wrong), and hashing
/// the key alone would re-check what the key comparison already checks.
///
/// `payload` must come from [`canonical_payload`] on both the write and the read side — see its doc for
/// why the entry file's own bytes are not usable here.
///
/// `key_input` is the entry key's own `digest_input()` — the generated field list, so a key field added
/// later joins this digest with no edit here.
pub(super) fn payload_digest(key_input: &str, payload: &[u8]) -> String {
    let mut bytes = Vec::with_capacity(key_input.len() + payload.len() + 1);
    bytes.extend_from_slice(key_input.as_bytes());
    bytes.push(0); // NUL, same separator convention the key digests use
    bytes.extend_from_slice(payload);
    crate::hash::digest128(&bytes)
}

impl crate::AnalysisCache {
    /// Checks one read entry's stored digest, counting a mismatch. Returns `false` to make the caller
    /// treat it as a miss.
    pub(super) fn payload_verifies(&self, key_input: &str, payload: &[u8], stored: &str) -> bool {
        if payload_digest(key_input, payload) == stored {
            return true;
        }
        self.record_rejected();
        false
    }
}

#[cfg(test)]
mod tests {
    use super::payload_digest;

    #[test]
    fn the_same_key_and_payload_digest_identically() {
        assert_eq!(
            payload_digest("k", b"payload"),
            payload_digest("k", b"payload")
        );
    }

    /// The defect this exists for: the payload changed, the key did not.
    #[test]
    fn a_changed_payload_changes_the_digest() {
        assert_ne!(
            payload_digest("k", b"[{\"ruleId\":\"x\"}]"),
            payload_digest("k", b"[]")
        );
    }

    /// The other half: a valid entry copied onto a different key's file must not verify. Without the
    /// key in the digest input, both entries would be internally consistent and one would be a lie.
    #[test]
    fn the_same_payload_under_a_different_key_digests_differently() {
        assert_ne!(
            payload_digest("k1", b"payload"),
            payload_digest("k2", b"payload")
        );
    }

    /// The NUL separator earns its place: without it, `("ab", "c")` and `("a", "bc")` would hash the
    /// same bytes, so a key ending in payload-shaped text could be shifted across the boundary.
    #[test]
    fn the_separator_keeps_the_key_and_payload_boundary_unambiguous() {
        assert_ne!(payload_digest("ab", b"c"), payload_digest("a", b"bc"));
    }
}
