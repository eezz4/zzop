//! The CAPABILITY-kind cell of the coverage reply: per-rule sightline blind spots, derived — never
//! hand-restated — by crossing each rule's own compiled-in declaration
//! (`zzop_engine::rule_sightlines`, built by the owning rules crates from the same pinned claim
//! constants their finding prose uses) with the tree's MEASURED extension mix. This is the axis the
//! parent module's doc kept deliberately absent while the only available source was
//! `docs/rules/catalog.md`'s prose: a hand copy would drift the day a recognizer widens, a derived
//! cross cannot.
//!
//! The cross is structural-only and CLASS-AWARE — it branches on each declaration's
//! `assert_when_blind`, because the two blindness classes read in opposite directions and a naive
//! "structural minus trigger" subtraction gets both wrong (measured 2026-07-31: a TS+Prisma tree
//! reported all 8 declared rules blind on `.prisma`, false for all 8 — `.prisma` cannot host the
//! silence-class evidence, and the inverse-class channel WAS fed by the `.ts` files):
//! - SILENCE class (`assert_when_blind == false`): an entry appears when the tree has structural
//!   files outside the rule's witnessed extensions, EXCLUDING declaration-only extensions
//!   (`zzop_engine::declaration_only_extension` — prisma/sql files project no code, so silence-class
//!   evidence can never live there and their presence is not blindness).
//! - INVERSE class (`assert_when_blind == true`): an entry appears only when the whole evidence
//!   channel is UNFED — the tree has structural files but none inside the witnessed extensions.
//!   `structuralOutside` may then honestly include a declaration-only extension: that is where the
//!   rule's SUBJECT (the schema) lives, and disclosing the flood on a schema-only tree is the point.
//!
//! Two scope qualifiers, stated here because the legend ships them: only DECLARED sightline rules
//! are crossed — native analyses; DSL pack rules carry their own `file_pattern` gate and are not
//! represented — and lexical-only files are not crossed: the `lexicalOnly` legend already says
//! everything structural was never evaluated for them, and repeating that per rule would drown the
//! per-rule signal in a restatement.

use std::collections::BTreeSet;

use serde_json::{json, Value};
use zzop_core::RuleSightline;

/// One entry per sightline-restricted rule that is blind on this tree in its OWN class's direction
/// (see the module doc); empty array (a schema position, not an omission) when no declared rule is.
/// Each entry quotes the rule's own `outside_meaning` sentence verbatim — the consequence wording is
/// owned by the rule, because the two blindness classes read opposite ways (a silent-when-blind
/// rule's zero means not-analyzed, never clean; an assert-when-blind rule's findings mean
/// evidence-channel blindness, not a verdict).
pub(super) fn blind_spots(
    sightlines: &[RuleSightline],
    structural_exts: &BTreeSet<String>,
) -> Value {
    let entries: Vec<Value> = sightlines
        .iter()
        .filter_map(|s| {
            let outside: Vec<&str> = structural_exts
                .iter()
                .filter(|e| !s.trigger_extensions.contains(&e.as_str()))
                .map(String::as_str)
                .collect();
            let emit = if s.assert_when_blind {
                // Inverse class: blindness is the UNFED channel, so any structural file inside the
                // witnessed set clears the whole tree — the evidence IS being gathered there.
                !outside.is_empty() && outside.len() == structural_exts.len()
            } else {
                // Silence class: a declaration-only extension cannot host this class's evidence,
                // so its presence outside the witnessed set says nothing about coverage.
                outside
                    .iter()
                    .any(|e| !zzop_engine::declaration_only_extension(e))
            };
            if !emit {
                return None;
            }
            Some(json!({
                "ruleId": s.rule_id,
                "witnessedIn": s.trigger_extensions,
                "structuralOutside": if s.assert_when_blind {
                    // The whole structural mix — the subject (e.g. `.prisma`) lives there and
                    // naming it is honest for a flood-when-blind rule.
                    outside
                } else {
                    outside
                        .into_iter()
                        .filter(|e| !zzop_engine::declaration_only_extension(e))
                        .collect()
                },
                "meaning": &s.outside_meaning,
            }))
        })
        .collect();
    Value::Array(entries)
}

/// The per-tree `blindSpotBasis` sentence — what the (possibly empty) `blindSpots` array was
/// computed FROM, the `joinVisibility` idiom: an empty array must be readable as either "crossed
/// and clean" or "nothing to cross" without guessing, and null/omission would be exactly that
/// guess. Counts are derived at emit time (a derived sentence, not a stored fact).
pub(super) fn basis(sightline_count: usize, structural_ext_count: usize) -> Value {
    if structural_ext_count == 0 {
        return json!(
            "no structural extension in this tree, so no sightline could be crossed — this empty \
             list is absence of input, not a coverage verdict"
        );
    }
    json!(format!(
        "{structural_ext_count} structural extension(s) crossed against {sightline_count} declared \
         rule sightline(s)"
    ))
}

/// The one sentence the vocabulary needs to be self-describing, shipped top-level next to
/// `dispatchMeaning` — same discipline, same reason.
pub(super) fn legend() -> Value {
    json!(
        "CAPABILITY cells, derived from each rule's own compiled-in sightline declaration crossed \
         with this tree's structural extensions — never from this run's findings and never restated \
         by hand. A rule's trigger can only be witnessed in files its evidence extractor covers, so \
         each entry names a tree where that rule's findings count carries no information about the \
         code, in the entry's own direction: a silent-when-blind rule is listed when structural \
         files sit outside its witnessedIn set (declaration-only extensions like prisma/sql \
         excluded — they cannot host code evidence), an assert-when-blind rule only when NO \
         structural file feeds its evidence channel at all. Only DECLARED sightline rules are \
         crossed (native analyses — DSL pack rules carry their own file_pattern gate), and \
         lexical-only files are not crossed. witnessedIn is an upper bound, never a completeness \
         claim — inside it, coverage can still be partial."
    )
}
