//! Contract: the `nativeAnalyses` disclosure derives its two lists from the registrations themselves,
//! and cannot go vacuously green.
//!
//! `zzop_engine::NativeAnalyses` publishes "which native analyses could not have keyed this reply's
//! `findings`". Every failure mode of that field is SILENT — an empty
//! `reportedInCrossLayerFindings` reads exactly like a build where every native analysis reports into
//! `findings`, which is the false all-clear the field exists to remove. So the floor lives here rather
//! than in the field: a derivation that stopped extracting must turn this file red, not turn a reply
//! reassuring.
//!
//! The floors below are LOWER BOUNDS, never the live counts (60 registered, 27 cross-layer today).
//! Pinning the exact numbers would make every added analysis a red test for no reason, which is how a
//! guard gets deleted; pinning nothing lets an empty extraction pass. Raise a floor only when the
//! population has moved far enough that the old one stopped bounding anything.

use zzop_core::RuleConfig;
use zzop_engine::{register_all_native, NativeAnalyses};

/// Both id sets, read off real registrations — never a hand list here, so this test cannot be edited
/// into agreement with a broken derivation.
fn registry_ids() -> Vec<String> {
    let mut registry = zzop_core::RuleRegistry::new();
    register_all_native(&mut registry);
    registry.ids().to_vec()
}

fn cross_layer_ids() -> Vec<String> {
    let mut registry = zzop_core::RuleRegistry::new();
    zzop_rules_cross_layer::register_native_analyses(&mut registry);
    registry.ids().to_vec()
}

/// The floor. A cross-layer registration that emptied — or a `register_all_native` that stopped
/// composing it — would publish `reportedInCrossLayerFindings: []`, indistinguishable on the wire from
/// a healthy build in which nothing needed disclosing.
#[test]
fn the_cross_layer_subset_is_a_non_empty_proper_subset_of_the_registry() {
    let all = registry_ids();
    let cross = cross_layer_ids();

    assert!(
        cross.len() >= 20,
        "the cross-layer registration collapsed to {} ids — `reportedInCrossLayerFindings` would go \
         quiet and a reader would take the missing `cross-layer/*` keys in `findings` for a clean \
         bill (measured: on the nine-tree corpus that blank hid 975 findings)",
        cross.len()
    );
    assert!(
        all.len() >= 40,
        "only {} native analyses registered — `nativeAnalyses.registered` is the denominator the two \
         lists are read against, and a collapsed one makes them unreadable",
        all.len()
    );
    for id in &cross {
        assert!(
            all.contains(id),
            "{id:?} is registered by zzop_rules_cross_layer but is absent from register_all_native — \
             the disclosure would name an id the config id space does not have"
        );
    }
    assert!(
        cross.len() < all.len(),
        "every registered native analysis is a cross-layer one; the disclosure would say no native \
         analysis ever reports into `findings`, which is false while graph/http/schema rules ship"
    );
}

/// The two lists are DISJOINT by construction, and that is load-bearing rather than tidy:
/// `reportedInCrossLayerFindings` carries an implicit remedy ("run the join and you will see these"),
/// and that remedy is wrong for an analysis the caller switched off. Bidirectional — one arm proves
/// the id is claimed when enabled, the other that it moves rather than duplicates.
#[test]
fn a_disabled_cross_layer_analysis_moves_out_of_the_cross_layer_list_into_disabled() {
    let subject = cross_layer_ids()
        .into_iter()
        .next()
        .expect("the floor test above guarantees a non-empty cross-layer registration");

    let open = NativeAnalyses::of(&RuleConfig::default());
    assert!(
        open.reported_in_cross_layer_findings.contains(&subject),
        "{subject:?} should be listed while enabled"
    );
    assert!(
        open.disabled.is_empty(),
        "a default RuleConfig disables nothing, so this list must be empty — an empty list is still \
         serialized, which is what lets a reader tell 'nothing was switched off' from 'this build \
         does not report'"
    );

    let gated = NativeAnalyses::of(&RuleConfig {
        disabled_rules: vec![subject.clone()],
        ..RuleConfig::default()
    });
    assert!(
        gated.disabled.contains(&subject),
        "{subject:?} was disabled and must say so"
    );
    assert!(
        !gated.reported_in_cross_layer_findings.contains(&subject),
        "{subject:?} is switched off, so telling the reader to run the join to see it is false advice"
    );
    assert_eq!(
        gated.reported_in_cross_layer_findings.len() + 1,
        open.reported_in_cross_layer_findings.len(),
        "exactly one id should have moved — a count that did not change means the gate was not \
         consulted at all"
    );
    assert_eq!(
        gated.registered, open.registered,
        "`registered` counts what this build CONTAINS; disabling must not shrink the denominator"
    );
}

/// A native analysis outside the cross-layer crate is gated by the same call, so the `disabled` list
/// is not a cross-layer-only feature. Uses a registry id read live rather than a spelled one.
#[test]
fn a_disabled_non_cross_layer_analysis_is_listed_too() {
    let cross = cross_layer_ids();
    let subject = registry_ids()
        .into_iter()
        .find(|id| !cross.contains(id))
        .expect("the proper-subset floor above guarantees at least one");

    let gated = NativeAnalyses::of(&RuleConfig {
        disabled_rules: vec![subject.clone()],
        ..RuleConfig::default()
    });
    assert!(
        gated.disabled.contains(&subject),
        "{subject:?} was disabled and must be named — `ruleOverridesApplied.disabled` is a different \
         top-level key answering a different question (which of the CALLER's entries took effect), \
         and reading a blank `findings` against it is the join this field removes"
    );
    let mut cross_sorted = cross;
    cross_sorted.sort();
    assert_eq!(
        gated.reported_in_cross_layer_findings, cross_sorted,
        "disabling a non-cross-layer analysis must not disturb the cross-layer list"
    );
}

/// Sorted, both of them — the field is diffed across runs by consumers, and the engine's determinism
/// contract does not admit iteration-order output.
#[test]
fn both_lists_are_sorted() {
    let all = registry_ids();
    let gated = NativeAnalyses::of(&RuleConfig {
        disabled_rules: all.clone(),
        ..RuleConfig::default()
    });
    let mut expected = all;
    expected.sort();
    assert_eq!(
        gated.disabled, expected,
        "disabling every registered id must reproduce the whole registry, sorted"
    );
    assert!(
        gated.reported_in_cross_layer_findings.is_empty(),
        "with everything disabled nothing is left to point at the join"
    );
}
