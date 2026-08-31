//! The axis: a declared pattern that cannot compile must not be indistinguishable from one that was
//! never declared.

use super::*;

#[test]
fn a_run_that_declares_nothing_warns_about_nothing() {
    assert!(uncompilable_vocabulary_warnings(&VocabularyConfig::default()).is_empty());
}

/// The product's own default vocabulary must not trip its own validator — the cheapest possible check
/// that the derived key set is not accidentally matching something it should not.
#[test]
fn the_built_in_vocabulary_compiles_clean() {
    let out = uncompilable_vocabulary_warnings(&VocabularyConfig::built_in());
    assert!(out.is_empty(), "{out:?}");
}

/// The measured case, verbatim. Before this, the run produced `configWarnings: []`, exit 0, and output
/// byte-identical to a valid pattern's — because `Regex::new(..).ok()` turns an unparseable value into
/// the same `None` an UNDECLARED key has, and undeclared means "make no judgment".
#[test]
fn an_uncompilable_pattern_is_named_along_with_what_it_cost() {
    let vocab = VocabularyConfig {
        auth_guard_pattern: Some("(?i)((((zorp[".to_string()),
        ..VocabularyConfig::default()
    };
    let out = uncompilable_vocabulary_warnings(&vocab);
    assert_eq!(out.len(), 1, "{out:?}");
    assert!(out[0].contains("vocabulary.authGuardPattern"), "{}", out[0]);
    // The consequence, not just the syntax error: the author can see the bracket once they know to
    // look; what they cannot see is that the run continued and read as clean.
    assert!(out[0].contains("made no judgment"), "{}", out[0]);
    assert!(
        out[0].contains("as if the key had never been declared"),
        "{}",
        out[0]
    );
    // And the offending value, so the author does not have to guess which of several patterns it was.
    assert!(out[0].contains("(?i)((((zorp["), "{}", out[0]);
}

/// The invalidation on the other side: a VALID pattern must produce nothing. Without it, "warn when it
/// does not compile" and "warn whenever the key is present" pass identically.
#[test]
fn a_valid_pattern_produces_no_warning() {
    let vocab = VocabularyConfig {
        auth_guard_pattern: Some("(?i)(auth|guard)".to_string()),
        ..VocabularyConfig::default()
    };
    assert!(uncompilable_vocabulary_warnings(&vocab).is_empty());
}

/// Every pattern key is checked, not just the first — one bad key must not mask another.
#[test]
fn each_bad_pattern_key_gets_its_own_warning() {
    let vocab = VocabularyConfig {
        auth_guard_pattern: Some("[".to_string()),
        orm_receiver_pattern: Some("(".to_string()),
        api_version_segment_pattern: Some("valid".to_string()),
        ..VocabularyConfig::default()
    };
    let out = uncompilable_vocabulary_warnings(&vocab);
    assert_eq!(out.len(), 2, "{out:?}");
    assert!(
        out.iter().any(|w| w.contains("authGuardPattern")),
        "{out:?}"
    );
    assert!(
        out.iter().any(|w| w.contains("ormReceiverPattern")),
        "{out:?}"
    );
}

/// The reason the key set is DERIVED rather than listed. If a future `*Pattern` field could fall
/// outside the check, this defect would simply reappear one key over — so the test asserts the
/// selection covers every regex-valued scalar the struct actually has today, computed the same way the
/// code does rather than from a copy of the list.
#[test]
fn every_pattern_suffixed_string_field_is_covered_by_the_derived_selection() {
    let all_bad = serde_json::to_value(VocabularyConfig::built_in()).unwrap();
    let pattern_keys: Vec<String> = all_bad
        .as_object()
        .unwrap()
        .iter()
        .filter(|(k, v)| k.ends_with("Pattern") && v.is_string())
        .map(|(k, _)| k.clone())
        .collect();
    assert!(
        pattern_keys.len() >= 6,
        "the built-in vocabulary should declare several pattern keys; got {pattern_keys:?}"
    );
    // Break each one in turn and confirm the checker names it — a hand-listed key set would pass this
    // only for the keys someone remembered to add.
    for key in &pattern_keys {
        let mut obj = all_bad.clone();
        obj[key] = serde_json::json!("[");
        let vocab: VocabularyConfig = serde_json::from_value(obj).unwrap();
        let out = uncompilable_vocabulary_warnings(&vocab);
        assert!(
            out.iter().any(|w| w.contains(&format!("vocabulary.{key}"))),
            "{key} is a regex-valued key the checker does not reach: {out:?}"
        );
    }
}

/// `extraTestPathPatterns` is plural and a LIST, and validates its own entries where it is applied.
/// Pulling it into this scalar check would double-report it.
#[test]
fn the_list_valued_test_path_patterns_key_is_not_pulled_into_this_check() {
    let vocab = VocabularyConfig {
        extra_test_path_patterns: vec!["[".to_string()],
        ..VocabularyConfig::default()
    };
    assert!(uncompilable_vocabulary_warnings(&vocab).is_empty());
}
