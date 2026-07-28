//! Cache keys — the fingerprints that key the incremental cache (content hash, parser fingerprint,
//! scope, ruleset fingerprint; see `docs/ARCHITECTURE.md`, "Caching"), plus the IR-side projection of
//! them.
//!
//! ## Why the key types generate their own digest input
//!
//! A cache entry's on-disk filename is a digest of the key, and every entry also re-stores its key so
//! a read can reject a mismatch. Both of those used to be hand-written field lists living in
//! `store.rs` — a `format!` join for the digest, and a field-by-field `!=` chain for the comparison.
//! A fifth `CacheKey` field threaded through the engine but forgotten in either place would not fail:
//! two semantically different runs would map onto ONE digest and silently serve each other's result.
//! A `CACHE_SCHEMA_VERSION` bump does not fix that — it only makes the first run cold, after which
//! the aliasing resumes.
//!
//! So the field list is written exactly once, and everything else is generated from it:
//! - [`cache_key_struct`] declares the struct AND its `digest_input`, so a new field is in the digest
//!   by construction — there is no second list to forget.
//! - The stored-key comparison is the derived `PartialEq` on the whole key type (`store.rs` compares
//!   key values, never field pairs), so a new field is in the comparison by construction too.
//! - [`IrKey`] makes the IR/findings split a TYPE rather than a convention, and its `From<&CacheKey>`
//!   destructures exhaustively (no `..`): a new `CacheKey` field fails to compile there until someone
//!   decides whether the IR depends on it.

use serde::{Deserialize, Serialize};

/// Declares a cache-key struct (all-`String` fields) together with the digest input derived from its
/// own field list. See the module doc for the failure mode this closes.
macro_rules! cache_key_struct {
    (
        $(#[$struct_meta:meta])*
        $name:ident { $( $(#[$field_meta:meta])* $field:ident ),+ $(,)? }
    ) => {
        $(#[$struct_meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        pub struct $name {
            $( $(#[$field_meta])* pub $field: String, )+
        }

        impl $name {
            /// Every field of this key, in declaration order, NUL-joined — the exact byte string
            /// `store.rs` digests into an entry filename.
            ///
            /// This is a SHARDING key, not an injective encoding: `scope` is itself NUL-joined by the
            /// engine, so the separator does not prove two distinct keys produce distinct inputs. That
            /// is why nothing trusts the digest alone — every read compares the stored key value
            /// against the requested one and treats a mismatch as a miss (see `store.rs`/`hash.rs`).
            pub(crate) fn digest_input(&self) -> String {
                [ $( self.$field.as_str() ),+ ].join("\u{0}")
            }
        }
    };
}

cache_key_struct! {
    /// (content hash, parser fingerprint, scope, vocabulary fingerprint, ruleset fingerprint) for one
    /// file — the full key a findings lookup uses. IR lookups use the [`IrKey`] projection of it (the
    /// first four fields).
    CacheKey {
        /// Hash of the file's raw bytes, not mtime, so the cache survives checkouts/CI restores that
        /// change mtimes but not content.
        content_hash,
        /// Parser id + pinned parser version + parser-logic version; bumping any invalidates every IR
        /// entry the old parser produced.
        parser_fingerprint,
        /// Disambiguates "which file, in which tree" (normalized relative path + tree id) — projected
        /// IR/findings can embed the file's own path, so byte-identical files must not alias each
        /// other's entry. Part of the IR key for that reason, not just the findings key.
        scope,
        /// Hash of the run's DECLARED convention vocabulary (`zzop_engine::VocabularyConfig`) — the
        /// names a project states for its own guards, ORM receivers, generated-file banners and so on.
        /// Deliberately ONE fingerprint over the WHOLE vocabulary rather than a per-lane subset: a
        /// subset declaration is precise right up until it drifts from what the lanes actually read,
        /// and the two failure modes are not symmetric — over-invalidating costs a recompute, while
        /// under-invalidating serves a WRONG answer. Kept out of `parser_fingerprint` on purpose: that
        /// field is BUILD identity (which parser, which version), and folding a run-time setting into
        /// it would make two runs of the same binary claim different builds.
        vocabulary_fingerprint,
        /// Fingerprint of the active per-file rule packs; bumping invalidates findings but leaves the
        /// IR entry (parser+scope+vocabulary-keyed) reusable.
        ruleset_fingerprint,
    }
}

cache_key_struct! {
    /// The IR half of a [`CacheKey`] — everything a cached `FileIrSlice` actually depends on.
    ///
    /// The asymmetry with the findings key is deliberate and load-bearing in both directions:
    /// `ruleset_fingerprint` is OUT (parsing does not consult the active rule packs, so IR survives a
    /// pack change), and `scope` is IN (a `FileIrSlice`'s `symbols`/`io` embed their own originating
    /// path, so byte-identical files in different places must not alias).
    ///
    /// `vocabulary_fingerprint` is IN: a declared vocabulary reaches the per-file PROJECTION itself
    /// (the ORM-receiver pattern that decides `SourceSymbol::write_sites`, the Prisma client getter
    /// that decides which `db-table` consumes exist, the router-mount guard words that decide a
    /// fragment's `auth-guarded` attribute), so the same bytes parsed under two vocabularies are two
    /// different slices.
    IrKey {
        content_hash,
        parser_fingerprint,
        scope,
        vocabulary_fingerprint,
    }
}

impl From<&CacheKey> for IrKey {
    fn from(key: &CacheKey) -> IrKey {
        // Exhaustive destructuring, deliberately without `..`: a new `CacheKey` field makes THIS line
        // fail to compile (E0027) until someone states whether the IR depends on it. `ruleset_fingerprint`
        // is the one field explicitly answered "no" — see `IrKey`'s doc.
        let CacheKey {
            content_hash,
            parser_fingerprint,
            scope,
            vocabulary_fingerprint,
            ruleset_fingerprint: _,
        } = key;
        IrKey {
            content_hash: content_hash.clone(),
            parser_fingerprint: parser_fingerprint.clone(),
            scope: scope.clone(),
            vocabulary_fingerprint: vocabulary_fingerprint.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CacheKey, IrKey};

    fn sample() -> CacheKey {
        CacheKey {
            content_hash: "c".to_string(),
            parser_fingerprint: "p".to_string(),
            scope: "s".to_string(),
            vocabulary_fingerprint: "v".to_string(),
            ruleset_fingerprint: "r".to_string(),
        }
    }

    /// The digest input is the key's OWN field list, in declaration order — pinned literally so a
    /// silent reordering (which would orphan every warm entry) is visible as a test change, and so the
    /// "adding a field changes the digest" property has a concrete anchor.
    #[test]
    fn digest_input_is_every_field_in_declaration_order() {
        assert_eq!(sample().digest_input(), "c\u{0}p\u{0}s\u{0}v\u{0}r");
        assert_eq!(IrKey::from(&sample()).digest_input(), "c\u{0}p\u{0}s\u{0}v");
    }

    /// The IR projection drops `ruleset_fingerprint` and NOTHING else: two keys differing only in the
    /// ruleset must project to the same `IrKey` (that is what lets a pack change reuse parsed IR),
    /// while a difference in any other field must survive the projection.
    #[test]
    fn the_ir_projection_drops_only_the_ruleset_fingerprint() {
        let mut other_ruleset = sample();
        other_ruleset.ruleset_fingerprint = "r2".to_string();
        assert_eq!(IrKey::from(&sample()), IrKey::from(&other_ruleset));

        for mutate in [
            (|k: &mut CacheKey| k.content_hash = "c2".to_string()) as fn(&mut CacheKey),
            |k: &mut CacheKey| k.parser_fingerprint = "p2".to_string(),
            |k: &mut CacheKey| k.scope = "s2".to_string(),
            |k: &mut CacheKey| k.vocabulary_fingerprint = "v2".to_string(),
        ] {
            let mut changed = sample();
            mutate(&mut changed);
            assert_ne!(
                IrKey::from(&sample()),
                IrKey::from(&changed),
                "a non-ruleset field change must survive the IR projection"
            );
        }
    }
}
