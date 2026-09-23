//! `vocabulary_fingerprint` cases, kept beside `tests.rs` rather than inside it because that file
//! is at the repo per-file line cap. Same module tree, same `super::*` reach.

use super::*;

/// The new C# route vocabulary rides `vocabulary_fingerprint` with no per-key wiring, because that
/// function hashes the whole `VocabularyConfig` SERIALIZATION rather than a hand-listed tuple (see its
/// doc). This is the check that the claim holds for a key nobody listed anywhere: declaring a different
/// root route builder must miss the warm entries written under the previous declaration, or a `.cs`
/// file's cached `IoFacts` would answer with routes keyed under a vocabulary this run does not have.
#[test]
fn vocabulary_fingerprint_moves_with_the_csharp_root_route_builder_names() {
    let mut config = EngineConfig {
        vocabulary: crate::VocabularyConfig::built_in(),
        ..EngineConfig::default()
    };
    let shipped = vocabulary_fingerprint(&config);

    config.vocabulary.csharp_root_route_builder_variable_names = vec!["builder".to_string()];
    assert_ne!(
        shipped,
        vocabulary_fingerprint(&config),
        "declaring a different C# root name must invalidate the entries written under the old one"
    );

    config.vocabulary.csharp_root_route_builder_variable_names = Vec::new();
    assert_ne!(
        shipped,
        vocabulary_fingerprint(&config),
        "and so must UNDECLARING it — an empty list is a real declaration on this roof, not a no-op"
    );
}
