use super::*;
use std::collections::BTreeSet;

/// The pin: the EXACT `(id, status)` map every registered class must carry. Pinning the STATUS (not
/// just the id) is the honesty guard — this registry's whole point is to not overclaim, so an
/// aspirational flip of any class to a stronger status (e.g. `provide-side-unextracted` ->
/// `notYetDetected` promoted to `asserted` before the detection actually ships) MUST fail the gate
/// rather than pass silently. Adding/renaming/removing a class, or changing any status, fails here —
/// update this table deliberately, in lock-step with the real shipped detection.
const EXPECTED: &[(&str, &str)] = &[
    ("capability-absent-vs-empty", "asserted"),
    ("channel-empty-family-dark", "partial"),
    ("classified-skip", "partial"),
    ("coincidental-match", "asserted"),
    ("config-error", "asserted"),
    ("consume-side-unextracted", "asserted"),
    ("generated-client-unrecognized", "partial"),
    ("input-scope-error", "partial"),
    ("key-mismatch-drift", "partial"),
    ("language-unparsed", "partial"),
    ("overlay-facts-unverified", "notYetDetected"),
    ("provide-side-unextracted", "partial"),
    ("resolution-gap", "asserted"),
    ("silent-truncation", "partial"),
    ("stale-cache", "partial"),
];

#[test]
fn registry_matches_the_pinned_id_and_status_map() {
    let actual: BTreeSet<(&str, &str)> = BLINDNESS_REGISTRY
        .iter()
        .map(|c| (c.id, c.status.as_str()))
        .collect();
    let expected: BTreeSet<(&str, &str)> = EXPECTED.iter().copied().collect();
    assert_eq!(
        actual, expected,
        "the blindness registry drifted from its pinned (id, status) map — a class was added, \
         renamed, removed, or (crucially) had its status changed. Update EXPECTED deliberately, and \
         only promote a status once the real detection ships (never aspirationally)."
    );
    // No duplicate ids (the BTreeSet would swallow a dup on `id` only if statuses also matched, so
    // check the raw count too).
    assert_eq!(BLINDNESS_REGISTRY.len(), EXPECTED.len());
}

#[test]
fn every_group_is_valid_and_all_four_are_represented() {
    let valid = [
        EXTRACTION_BLIND,
        ANALYSIS_DARK,
        INPUT_CONFIG,
        TRUST_CALIBRATION,
    ];
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for class in BLINDNESS_REGISTRY {
        assert!(
            valid.contains(&class.group),
            "unknown group {:?} on {:?}",
            class.group,
            class.id
        );
        assert!(
            !class.summary.trim().is_empty(),
            "empty summary on {:?}",
            class.id
        );
        seen.insert(class.group);
    }
    assert_eq!(
        seen.len(),
        valid.len(),
        "not all four taxonomy groups are represented"
    );
}

#[test]
fn status_tokens_are_the_three_known_camel_case_values() {
    for class in BLINDNESS_REGISTRY {
        assert!(
            matches!(
                class.status.as_str(),
                "asserted" | "partial" | "notYetDetected"
            ),
            "unexpected status token {:?}",
            class.status.as_str()
        );
    }
}
