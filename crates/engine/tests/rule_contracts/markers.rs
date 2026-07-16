//! Contracts 1-2: suppress-marker presence/uniqueness/convention and the message "how to exclude" leg.

use std::collections::BTreeMap;

use crate::load_all_packs;

// ---------------------------------------------------------------------------------------------
// 1. Marker presence
// ---------------------------------------------------------------------------------------------

/// Every DSL rule ships a non-empty `suppress_marker`. A rule with no marker (or an empty-string one)
/// cannot be suppressed inline (see `RuleDef::suppress_marker`'s doc in `crates/core/src/dsl.rs`) — the
/// only way to quiet a single false positive is `disabled_rules`, which throws away every future true
/// positive from that rule too. A prior audit found DSL rules shipped with no marker at all; this test
/// makes that class of drift a hard failure instead of a convention someone has to remember.
#[test]
fn every_dsl_rule_has_a_non_empty_suppress_marker() {
    let packs = load_all_packs();
    let mut offenders = Vec::new();
    for pack in &packs {
        for rule in &pack.rules {
            let ok = rule
                .suppress_marker
                .as_deref()
                .is_some_and(|m| !m.trim().is_empty());
            if !ok {
                offenders.push(format!("{}/{}", pack.id, rule.id));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "DSL rules with no non-empty suppress_marker: {offenders:#?}"
    );
}

/// Within one pack, no two rules may share a `suppress_marker`. Two rules sharing a marker co-suppress: a
/// `// marker-ok` comment a reader placed to vet ONE rule's finding silently also suppresses the OTHER
/// rule's finding wherever its own line/lookback window overlaps — the reader never opted into that.
#[test]
fn suppress_markers_are_unique_within_each_pack() {
    let packs = load_all_packs();
    let mut offenders = Vec::new();
    for pack in &packs {
        let mut by_marker: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for rule in &pack.rules {
            if let Some(marker) = rule.suppress_marker.as_deref() {
                if !marker.trim().is_empty() {
                    by_marker.entry(marker).or_default().push(rule.id.as_str());
                }
            }
        }
        for (marker, rules) in by_marker {
            if rules.len() > 1 {
                offenders.push(format!(
                    "pack `{}`: marker `{marker}` shared by rules {rules:?} (co-suppression risk)",
                    pack.id
                ));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "duplicate suppress_marker within a pack: {offenders:#?}"
    );
}

/// Every `suppress_marker` ends in `-ok` — the naming convention every one of the shipped markers follows
/// (a 2026-07-13 uniformity sweep measured 112/112) and the shape the authoring guide's example teaches
/// (`debug-token-ok`). The convention is load-bearing for users, not cosmetic: someone who has learned
/// `// <marker>-ok` from one rule will type that shape for the next rule from memory, and a rule whose
/// marker deviates (`nplus1_allow`, `skip-x`) silently fails to suppress for them. Deviating on purpose is
/// a policy change: adjust this test in the same commit and say why.
#[test]
fn every_suppress_marker_follows_the_dash_ok_naming_convention() {
    let packs = load_all_packs();
    let mut offenders = Vec::new();
    for pack in &packs {
        for rule in &pack.rules {
            if let Some(marker) = rule.suppress_marker.as_deref() {
                if !marker.trim().is_empty() && !marker.ends_with("-ok") {
                    offenders.push(format!("{}/{}: `{marker}`", pack.id, rule.id));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "suppress_marker values deviating from the `-ok` suffix convention every other marker follows: \
         {offenders:#?}"
    );
}

// ---------------------------------------------------------------------------------------------
// 2. Message triple — problem (the rest of `message`) + fix (the rest of `message`) + exclude (this leg)
// ---------------------------------------------------------------------------------------------

/// Every DSL rule's `message` names its own suppress marker OR the literal `disabled_rules`/`disabledRules`
/// string somewhere in the text — the "how to exclude" leg of zzop's finding contract (every finding must
/// tell the reader the problem, the fix, AND how to turn it off — zzop's finding-output design
/// principle; see docs/rules/authoring-guide.md's quality bar). A rule that legitimately has no
/// per-finding marker (native-analysis-style disable-only rules ported into the DSL, if any ever are) still
/// passes via the `disabled_rules` leg — this test accepts EITHER, not just the marker.
#[test]
fn every_dsl_rule_message_documents_how_to_exclude_it() {
    let packs = load_all_packs();
    let mut offenders = Vec::new();
    for pack in &packs {
        for rule in &pack.rules {
            let marker_leg = rule
                .suppress_marker
                .as_deref()
                .is_some_and(|m| !m.trim().is_empty() && rule.message.contains(m));
            let disabled_leg =
                rule.message.contains("disabled_rules") || rule.message.contains("disabledRules");
            if !(marker_leg || disabled_leg) {
                offenders.push(format!(
                    "{}/{} (suppress_marker={:?}) — message mentions neither its own marker nor \
                     disabled_rules/disabledRules",
                    pack.id, rule.id, rule.suppress_marker
                ));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "rule messages missing the \"how to exclude\" leg: {offenders:#?}"
    );
}
