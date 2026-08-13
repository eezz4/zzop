//! The cross-pack suppress-marker collision census: every derived `zzop-<rule id>-ok` marker that more
//! than ONE loaded rule answers to.
//!
//! Split out of `pack_loader.rs` (300-line cap) for the same reason `rule_issues.rs` was — one defect
//! class, one function, one message shape.
//!
//! ## Why this is a WARNING and not a rename
//! The obvious "fix" is to namespace the marker (`zzop-<pack>-<rule>-ok`) so a collision becomes
//! impossible by construction. We deliberately do NOT do that. The marker SPELLING is a public contract
//! that 1.0 freezes, and it is a contract written into USER SOURCE, not into config we control: every
//! `// zzop-hardcoded-secret-ok` comment already sitting in someone's repo was placed by a human who
//! vetted a finding. Renaming the derivation would silently un-suppress every one of those comments at
//! once — the vetted findings all come back on the next run, and the reader has no diff to read that
//! explains why. That is a strictly worse failure than the one being reported here, and it lands on
//! every user rather than only on the ones who loaded two packs that collide. So the grammar stays
//! frozen (`RuleDef::suppress_marker_for_id`) and the collision is DISCLOSED instead, naming both sides
//! so the pack author who actually owns one of the two ids can rename THEIR rule.
//!
//! ## Why this cannot be answered inside `load_dsl_packs`
//! A collision is a property of the LOADED SET, and that set is assembled from sources that never meet
//! inside one loader call: the bundled packs arrive as inline `packDefs` (compile-time embedded, see
//! `zzop_config::BUNDLED_PACK_SOURCES`), a user's packs arrive from one or more `packsDir` directories,
//! and a direct embedder may fill `EngineConfig::packs` itself. The headline case — a third-party pack
//! whose rule id collides with a BUNDLED rule's — is exactly a collision ACROSS those sources, so a
//! per-directory check inside `load_dsl_packs` would be blind to it. This takes the whole merged list
//! instead, and the callers hand it `EngineConfig::packs` after every source has been folded in.

use std::collections::BTreeMap;

use crate::dsl::RulePackDef;

/// Every suppress marker that more than one rule in `packs` derives, as one issue string each,
/// deterministic in both axes (markers in marker order, ids sorted within a marker).
///
/// The marker is derived as `zzop-<rule id>-ok` from the rule id ALONE (see
/// `RuleDef::suppress_marker`), with no pack namespace in it, so "two rules derive the same marker" is
/// exactly "two rules share a bare id". The consequence is co-suppression the reader never opted into:
/// a `// zzop-x-ok` comment placed to vet one rule's finding also silences every other rule deriving
/// `zzop-x-ok` wherever their lookback windows overlap.
///
/// Ids are reported PACK-QUALIFIED (`<pack>/<rule>`) for `pack_regex_issues`' reason verbatim — a bare
/// id is ambiguous here by construction, since a bare-id clash is the very thing being reported, and
/// the qualified form is the one a reader can act on (it names which pack to go rename the rule in).
///
/// ## Within one pack, too
/// Rule ids being unique inside a single pack is a property of the SHIPPED packs (pinned by
/// `rule_contracts`' `dsl_rule_ids_are_unique_within_each_pack`), NOT something the loader enforces:
/// nothing in `parse_dsl_pack` rejects a third-party pack that lists `hardcoded-secret` twice. So this
/// census deliberately does not dedupe within a pack — a within-pack duplicate surfaces as the same
/// `<pack>/<rule>` string listed twice, which is itself the diagnosis.
pub fn suppress_marker_collisions(packs: &[RulePackDef]) -> Vec<String> {
    let mut by_marker: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for pack in packs {
        for rule in &pack.rules {
            by_marker
                .entry(rule.suppress_marker())
                .or_default()
                .push(format!("{}/{}", pack.id, rule.id));
        }
    }
    by_marker
        .into_iter()
        .filter(|(_, ids)| ids.len() > 1)
        .map(|(marker, mut ids)| {
            // Sorted so the sentence is byte-identical across runs regardless of which source
            // contributed which pack first (directory-iteration order, `packDefs` array order).
            ids.sort();
            let listed = ids
                .iter()
                .map(|id| format!("\"{id}\""))
                .collect::<Vec<String>>()
                .join(", ");
            format!(
                "suppress marker `{marker}` is derived by {} loaded rules: {listed}. The marker is \
                 derived from the RULE id alone and carries no pack namespace, so ONE \
                 `// {marker}` comment suppresses ALL of them wherever their lookback windows \
                 overlap — including findings the reader who wrote that comment never vetted. The \
                 marker spelling is frozen (a 1.0 public contract that user source already carries), \
                 so it will not be namespaced to dodge this; rename the rule id in whichever pack you \
                 control.",
                ids.len()
            )
        })
        .collect()
}
