//! The cross-crate invariant behind `zzop_core::vocab_norm::NORMALIZED_VOCABULARY_KEYS`: every key
//! that table names is a real field of [`VocabularyConfig`].
//!
//! ## Why the table is not here any more
//!
//! It was, until 2026-09-08 (review ledger V122). One consumer outside this crate read it —
//! `zzop-config`'s mapper, to warn about a declared entry that cannot survive the normalization its
//! consuming rule applies — and reading it was the ENTIRE shipped reason `zzop-config` depended on
//! `zzop-engine`. 📏 One line of shipped code, and behind it a dependency closure containing swc and
//! eight parser crates, in a front end whose own manifest says it "only ever produces request JSON".
//! The table moved to the kernel that already owns its transforms; the edge went with it.
//!
//! ## Why the TEST stays here
//!
//! The table names config-key SPELLINGS; the authority on which spellings exist is
//! [`VocabularyConfig`]'s own serde surface, which lives in this crate. So the table can live in the
//! kernel and the check that it names real keys cannot — this file is that check, and it is the
//! reason the file still exists at all. A typo in the kernel's table would otherwise be silent, which
//! is precisely the failure the table was created to close, one layer up.

#[cfg(test)]
mod tests {
    use zzop_core::vocab_norm::{normalizer_for, NORMALIZED_VOCABULARY_KEYS};

    use crate::VocabularyConfig;

    #[test]
    fn every_listed_key_is_a_real_vocabulary_key() {
        // A typo here would silently check nothing — the same class of failure the table exists to
        // close, one layer up. The authority is the struct's own serde surface rather than a second
        // reading of `config-surface.json`: `VocabularyConfig` IS the shape being validated, so a key
        // renamed on the struct breaks this test in the same commit that renames it.
        let json = serde_json::to_value(VocabularyConfig::default()).expect("serializes");
        let object = json.as_object().expect("VocabularyConfig is an object");
        for (key, _) in NORMALIZED_VOCABULARY_KEYS {
            assert!(
                object.contains_key(*key),
                "{key} is not a field of VocabularyConfig; known: {:?}",
                object.keys().collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn the_shipped_built_in_values_all_survive_their_own_normalizer() {
        // `zzop init` writes the built-ins into the user's file. If any of them could not survive its
        // own key's normalization, running `init` would immediately produce the very warning this table
        // powers — a tool warning about the file it just wrote. This is the assertion that makes the
        // warning trustworthy rather than noise.
        let json = serde_json::to_value(VocabularyConfig::built_in())
            .expect("VocabularyConfig serializes");
        for (key, normalize) in NORMALIZED_VOCABULARY_KEYS {
            let Some(entries) = json.get(key).and_then(serde_json::Value::as_array) else {
                continue;
            };
            for entry in entries.iter().filter_map(serde_json::Value::as_str) {
                assert_eq!(
                    normalize(entry),
                    entry,
                    "built-in {key} entry {entry:?} cannot match its own normalized input"
                );
            }
        }
    }

    #[test]
    fn an_unlisted_key_reports_no_normalizer() {
        assert!(normalizer_for("javaSourceRoot").is_none());
        assert!(normalizer_for("secretParamNames").is_some());
    }
}
