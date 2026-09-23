//! Contract: the `nativeAnalyses` disclosure derives every one of its lists from the registrations
//! themselves, and cannot go vacuously green.
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

use zzop_core::{is_enabled, RuleConfig, Suppression};
use zzop_engine::{register_all_native, shipped_off_native_ids, NativeAnalyses};

/// A `RuleConfig` carrying the shipped-off set the SURFACE builds — what `zzop analyze` runs with, as
/// opposed to `RuleConfig::default()`, which is the library caller's every-analysis config. Composed
/// here from `shipped_off_native_ids` rather than spelled, so a fourth id added later is tested by
/// these pins the day it lands.
fn product_config() -> RuleConfig {
    RuleConfig {
        default_off: shipped_off_native_ids(),
        ..RuleConfig::default()
    }
}

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
        "only {} native analyses registered — `nativeAnalyses.registered` is the denominator the \
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

/// The shipped-off set's own floor, and the one property the JOIN lane silently depends on.
///
/// `zzop_summary::cross::native_analyses` builds the join's cross-layer population as the UNION of each
/// tree's `reportedInCrossLayerFindings`, and it has no `shippedOff` key of its own. That is safe only
/// while no shipped-off id is a cross-layer one: each tree's list is already `is_enabled`-filtered, so a
/// cross-layer id added to `DEFAULT_OFF` would vanish from every tree's list and therefore from the
/// union — leaving the join reply unable to say why, since the key that would explain it does not exist
/// there. The dependency is real and invisible at the join, so it is asserted here, at the set that
/// would break it.
#[test]
fn the_shipped_off_set_is_registered_and_holds_no_cross_layer_analysis() {
    let shipped_off = shipped_off_native_ids();
    let all = registry_ids();
    let cross = cross_layer_ids();

    assert!(
        !shipped_off.is_empty(),
        "the shipped-off composition returned nothing — `nativeAnalyses.shippedOff` would be empty on \
         every reply, which reads as 'this build ships every analysis on' and is the wrong sentence"
    );
    for id in &shipped_off {
        assert!(
            all.contains(id),
            "{id:?} ships off but is not registered — the gate would refuse an id no reply can name, \
             so nothing would ever say it was skipped"
        );
        assert!(
            !cross.contains(id),
            "{id:?} ships off AND is a cross-layer analysis. The join reply has no `shippedOff` key: \
             this id would drop out of every tree's `reportedInCrossLayerFindings`, and the join's \
             union of those lists would lose it with no channel able to say why. Give the join lane \
             its own shipped-off key before adding a cross-layer id here"
        );
    }
    assert!(
        shipped_off.len() < all.len(),
        "every registered analysis ships off; a default run would report nothing native at all"
    );
}

/// The gate itself, in the three states a config can put a shipped-off id in. The middle two are the
/// SAME gesture written two ways — `rules` maps a severity value to `severity_overrides` and an object
/// value carrying only `exclude` to `suppressions` — and reading one map alone would make the second
/// spelling a silent no-op, which is the spelling `docs/getting-started.md` uses in its example.
#[test]
fn a_shipped_off_analysis_runs_only_when_the_config_names_it_in_either_spelling() {
    let subject = shipped_off_native_ids()
        .into_iter()
        .next()
        .expect("the floor above guarantees a non-empty shipped-off set");
    let product = product_config();

    assert!(
        !is_enabled(&product, &subject),
        "{subject:?} ships off, so an untouched product config must not evaluate it"
    );
    assert!(
        is_enabled(&RuleConfig::default(), &subject),
        "a library caller building an EngineConfig by hand states their own config and gets every \
         registered analysis — the shipped-off set is a property of the product surface, not of the \
         engine, and folding it into the kernel default would take the choice away from an embedder"
    );

    let mut by_severity = product.clone();
    by_severity
        .severity_overrides
        .insert(subject.clone(), zzop_core::Severity::Info);
    assert!(
        is_enabled(&by_severity, &subject),
        "naming {subject:?} with a severity is the opt-in, and it is the only one documented"
    );

    let mut by_exclude = product.clone();
    by_exclude.suppressions.push(Suppression {
        rule: subject.clone(),
        path: Some("legacy/".to_string()),
        glob: None,
    });
    assert!(
        is_enabled(&by_exclude, &subject),
        "`rules: {{ \"{subject}\": {{ \"exclude\": [..] }} }}` writes a suppression and no severity. \
         Excluding paths from an analysis is asking for it everywhere else, so it counts as naming it \
         — otherwise a user who scoped a shipped-off rule would get zero findings and no explanation"
    );

    let mut disabled_too = by_severity.clone();
    disabled_too.disabled_rules.push(subject.clone());
    assert!(
        !is_enabled(&disabled_too, &subject),
        "an explicit `\"off\"` must beat the opt-in — the caller's last word about their own config"
    );
}

/// The two non-evaluation lists are split by CAUSE and never duplicate: `disabled` is what the caller
/// chose, `shippedOff` is what the build chose, and a reader acts on the difference (`stop disabling
/// it` versus `name it with a severity`). Their union is exactly the set the gate refused.
#[test]
fn a_shipped_off_analysis_is_listed_under_shipped_off_and_not_under_disabled() {
    let shipped_off = shipped_off_native_ids();
    let subject = shipped_off
        .first()
        .cloned()
        .expect("the floor above guarantees a non-empty shipped-off set");

    let product = NativeAnalyses::of(&product_config());
    let mut expected = shipped_off.clone();
    expected.sort();
    assert_eq!(
        product.shipped_off, expected,
        "the published list must be the composed set, sorted — consumers diff this field across runs"
    );
    assert!(
        product.disabled.is_empty(),
        "the product config disables nothing by id, so `disabled` must stay empty: a project default \
         listed there tells every reader they made a choice they did not make, which is the entire \
         reason these are two lists"
    );
    for id in &product.shipped_off {
        assert!(
            !product.reported_in_cross_layer_findings.contains(id),
            "{id:?} is in both lists — a shipped-off analysis must not also carry the join's implicit \
             remedy, which would tell the reader to run a join that still would not evaluate it"
        );
    }

    let also_disabled = NativeAnalyses::of(&RuleConfig {
        disabled_rules: vec![subject.clone()],
        ..product_config()
    });
    assert!(
        also_disabled.disabled.contains(&subject),
        "{subject:?} was disabled BY THE CALLER, and that authorship is what `disabled` reports"
    );
    assert!(
        !also_disabled.shipped_off.contains(&subject),
        "{subject:?} appears in both lists — they partition one gate answer and must stay disjoint, or \
         the counts a reader adds up double-count it"
    );
    assert_eq!(
        also_disabled.disabled.len() + also_disabled.shipped_off.len(),
        product.shipped_off.len(),
        "the union must be unchanged: the same ids were refused, only the attributed cause moved"
    );
}
