//! Contracts 1-2: derived suppress-marker global uniqueness and the message "how to exclude" leg.
//!
//! Markers are no longer stored per rule — `RuleDef::suppress_marker()` DERIVES `<id>-ok` (see its doc).
//! That collapses three formerly-hand-guarded invariants into construction guarantees: every rule now has a
//! non-empty marker (ids are never empty), and every marker ends in `-ok` by definition. What derivation
//! does NOT guarantee is cross-pack uniqueness — two rules in different packs sharing an id would derive the
//! same marker and co-suppress — so that is the one presence/uniqueness invariant still worth a test.

use std::collections::BTreeMap;

use crate::load_all_packs;

// ---------------------------------------------------------------------------------------------
// 1. Derived-marker global uniqueness
// ---------------------------------------------------------------------------------------------

/// No two shipped rules — in the same pack OR across packs — may derive the same suppress marker. Since the
/// marker is `<id>-ok`, this is exactly "rule ids are globally unique". It matters because a `// x-ok`
/// comment a reader placed to vet ONE rule's finding would silently also suppress any OTHER rule that
/// derives `x-ok` wherever their line/lookback windows overlap — the reader never opted into that. The
/// within-pack case was the old contract; deriving from the id widened the blast radius to every pack, so
/// the guard widens with it.
#[test]
fn derived_suppress_markers_are_globally_unique() {
    let packs = load_all_packs();
    let mut by_marker: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for pack in &packs {
        for rule in &pack.rules {
            by_marker
                .entry(rule.suppress_marker())
                .or_default()
                .push(format!("{}/{}", pack.id, rule.id));
        }
    }
    let offenders: Vec<String> = by_marker
        .into_iter()
        .filter(|(_, rules)| rules.len() > 1)
        .map(|(marker, rules)| {
            format!("marker `{marker}` shared by rules {rules:?} (co-suppression risk)")
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "rules that derive a duplicate suppress marker: {offenders:#?}"
    );
}

/// Uniqueness above compares markers for EQUALITY, which is not the whole aliasing surface: `compile_marker`
/// anchors the marker as `//\s*<marker>\b`, and `\b` fires at a word/non-word boundary — so rule `x`'s marker
/// `x-ok` also matches inside rule `x-ok-y`'s marker `x-ok-y-ok` (the boundary sits between `k` and `-`).
/// A reader annotating a `x-ok-y` finding would silently suppress `x` on that line too, having opted into
/// neither. Zero shipped ids have this shape today (it needs an id containing `-ok-` or ending `-ok`), which
/// is exactly why it is worth pinning now — nothing else stops the first such id from being authored.
#[test]
fn no_derived_marker_is_a_word_boundary_prefix_of_another() {
    let packs = load_all_packs();
    let ids: Vec<String> = packs
        .iter()
        .flat_map(|pack| pack.rules.iter().map(|rule| rule.id.clone()))
        .collect();
    let offenders: Vec<String> = ids
        .iter()
        .flat_map(|shorter| {
            let prefix = format!("{shorter}-ok");
            ids.iter()
                .filter(move |longer| longer.as_str() != shorter && longer.starts_with(&prefix))
                .map(move |longer| {
                    format!(
                        "rule `{shorter}` (marker `{shorter}-ok`) also fires inside rule `{longer}`'s marker \
                         `{longer}-ok` (co-suppression risk)"
                    )
                })
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "rule ids whose derived markers alias by word boundary: {offenders:#?}"
    );
}

// ---------------------------------------------------------------------------------------------
// 2. Message triple — problem + fix + exclude (this leg)
// ---------------------------------------------------------------------------------------------

/// Every DSL rule's `message` names its own derived suppress marker (`<id>-ok`) OR the literal
/// `disabled_rules`/`disabledRules` string somewhere in the text — the "how to exclude" leg of zzop's
/// finding contract (every finding must tell the reader the problem, the fix, AND how to turn it off; see
/// docs/rules/authoring-guide.md's quality bar). A rule that legitimately has no per-finding marker still
/// passes via the `disabled_rules` leg — this test accepts EITHER, not just the marker.
#[test]
fn every_dsl_rule_message_documents_how_to_exclude_it() {
    let packs = load_all_packs();
    let mut offenders = Vec::new();
    for pack in &packs {
        for rule in &pack.rules {
            let marker = rule.suppress_marker();
            let marker_leg = rule.message.contains(&marker);
            let disabled_leg =
                rule.message.contains("disabled_rules") || rule.message.contains("disabledRules");
            if !(marker_leg || disabled_leg) {
                offenders.push(format!(
                    "{}/{} (derived marker `{marker}`) — message mentions neither its own marker nor \
                     disabled_rules/disabledRules",
                    pack.id, rule.id
                ));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "rule messages missing the \"how to exclude\" leg: {offenders:#?}"
    );
}
