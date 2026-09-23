//! ROI scoring. Pure scalar functions: the caller extracts risk/loc/fanIn from a FileNode (or passes 0
//! when absent). (`hotspot_score` — the other half of the original roi/hotspot module — moved to
//! `zzop_core::file_nodes` in the R3 crate-boundary batch: `build_file_nodes` is a core mechanism and
//! must stay reachable from core without an upward dependency on this crate — per the crate-boundary
//! split.)

use serde::{Deserialize, Serialize};

use zzop_core::Severity;

const MIN_COST: f64 = 10.0;
const FANIN_COST_WEIGHT: f64 = 3.0;

/// Declares [`RecId`] and its [`RecId::ALL`] array from ONE token list, so the array cannot omit,
/// duplicate or reorder a variant.
///
/// A hand-written `ALL` beside an enum is the exact shape this repo keeps finding broken: the list is a
/// second population, a new variant reaches the wire without reaching the list, and every test whose
/// subjects come from the list goes on passing while saying nothing about it. `meaning`'s exhaustive
/// match already forces a new variant to HAVE a sentence; this forces that sentence to be CHECKED.
/// Same device and same argument as `crates/core/tests/envelope_schema_parity`'s `mirror_enum!`.
macro_rules! rec_ids {
    ($(#[$meta:meta])* $name:ident { $($(#[$vmeta:meta])* $variant:ident),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(rename_all = "kebab-case")]
        pub enum $name {
            $($(#[$vmeta])* $variant,)+
        }

        impl $name {
            /// Every variant, generated from this enum's own declaration — see [`rec_ids!`].
            pub const ALL: &'static [Self] = &[$(Self::$variant,)+];
        }
    };
}

rec_ids! {
    /// Recommendation rule id.
    ///
    /// `UrgentBugRisk` is not a rule — it is the synthetic escalation group id
    /// (`recommendations::escalate_critical_bug_evidence`) that critical-finding-confirmed items are moved
    /// into so they sort to the top without inflating their ROI. `compute_roi`/`derive_action_hint_key` are
    /// only ever called with an item's ORIGINAL rule id, before escalation moves it — so `reduction_ratio`
    /// below never actually receives `UrgentBugRisk` at runtime; its match arm exists only to keep the
    /// match exhaustive and panics loudly if that invariant is ever broken.
    RecId {
        BugProne,
        Circular,
        HotChurn,
        FatFanout,
        HiddenCoupling,
        VersioningCandidate,
        /// Synthetic escalation-only group id — see the enum doc above.
        UrgentBugRisk,
    }
}

impl RecId {
    /// What THIS id means, as one sentence, for the reader holding it.
    ///
    /// # Why the meaning ships with the value (2026-09-14, review ledger V214)
    ///
    /// The reply used to say `id` names "one of a CLOSED SET the rule-catalog defines" — an invitation
    /// to hard-code the list, and a promise this project does not make: `VERSIONING.md` puts field
    /// VALUES outside the freeze, so the set may gain a member in a MINOR release. A consumer who took
    /// the invitation (`else throw new Error("unknown recommendation kind")`) breaks on a change that
    /// is not a compatibility break, and the reply is what told them it was safe.
    ///
    /// The repair is not a bigger promise, it is removing the need for one: this workspace already
    /// ships that design on another lane — `zzop_facade`'s io `verdict` carries `verdictMeaning` FOR
    /// THE TOKEN IT RETURNED, so nothing downstream has to know the vocabulary. A value that explains
    /// itself needs no list to be memorized, and an id added later arrives explaining itself too.
    ///
    /// One owner: the `serde` spelling and this sentence sit on the same variant, so a renamed id
    /// cannot keep an explanation about the old one.
    pub fn meaning(self) -> &'static str {
        match self {
            Self::BugProne => "this file took repeated FIX commits in the analyzed git window — history says defects land here, so it is ranked as the place a change is most likely to be needed",
            Self::Circular => "this file sits in an import cycle, so it cannot be understood, tested or replaced without the rest of the cycle coming with it",
            Self::HotChurn => "this file changes far more per line than the tree's norm — churn concentrated in one place, which is where review attention buys the most",
            Self::FatFanout => "this file imports many modules, so it is a structural junction: a change here reaches widely and it is hard to hold in one's head",
            Self::HiddenCoupling => "this file changes together with another file that does NOT import it — a dependency the import graph cannot see, so nothing warns when only one of the pair is edited",
            Self::VersioningCandidate => "this file has many importers AND a fix history, so a breaking change to it is both expensive and likely — the shape that earns an explicit version boundary",
            Self::UrgentBugRisk => "SYNTHETIC GROUP, produced by no rule: an item lands here when its file ALSO carries a critical rule finding, so the structural signal and a real finding agree on the same file",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoiResult {
    pub roi: f64,
    pub estimated_reduction: f64,
    pub estimated_cost: f64,
}

/// ROI scalar (D1 formula):
/// reduction = base_risk * reductionRatio(rule) * severityMultiplier(sev)
/// cost      = max(10, loc + fanIn*3)
/// roi       = reduction / cost
///
/// The formula once carried two more cost multipliers — an untested-path doubling and a
/// change-amplification factor — driven by `BuildRecInput` fields that NO production caller ever
/// populated (the engine passed an empty set and an empty map at the sole call site, so both multipliers
/// were constant 1.0 in every real run). They were deleted rather than wired because nothing in the
/// config surface could feed them: neither "which paths have tests" nor "co-change amplification per
/// path" is a quantity this pipeline computes. Reinstating either means first building its input.
pub fn compute_roi(
    rule_id: RecId,
    severity: Severity,
    base_risk: f64,
    loc: u32,
    fan_in: u32,
) -> RoiResult {
    let reduction = base_risk * reduction_ratio(rule_id) * severity_multiplier(severity);
    let cost = (loc as f64 + fan_in as f64 * FANIN_COST_WEIGHT).max(MIN_COST);
    RoiResult {
        roi: reduction / cost,
        estimated_reduction: reduction,
        estimated_cost: cost,
    }
}

fn reduction_ratio(rule_id: RecId) -> f64 {
    match rule_id {
        RecId::BugProne | RecId::Circular | RecId::FatFanout => 0.7,
        RecId::HotChurn | RecId::HiddenCoupling | RecId::VersioningCandidate => 0.5,
        RecId::UrgentBugRisk => {
            unreachable!("UrgentBugRisk is a post-escalation synthetic group id — compute_roi is only ever called with an item's original rule id, before escalation")
        }
    }
}

fn severity_multiplier(severity: Severity) -> f64 {
    match severity {
        Severity::Critical => 3.0,
        Severity::Warning => 2.0,
        Severity::Info => 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roi_basic_formula() {
        // risk 100, loc 50, fanIn 10, circular(0.7) x critical(3); cost = max(10, 50+30)=80.
        let r = compute_roi(RecId::Circular, Severity::Critical, 100.0, 50, 10);
        assert!((r.estimated_reduction - 210.0).abs() < 1e-9);
        assert!((r.estimated_cost - 80.0).abs() < 1e-9);
        assert!((r.roi - 2.625).abs() < 1e-9);
    }

    #[test]
    fn roi_cost_floor_applies_to_tiny_files() {
        // loc 1, fanIn 0 -> raw cost 1, floored to MIN_COST.
        let r = compute_roi(RecId::Circular, Severity::Critical, 100.0, 1, 0);
        assert!((r.estimated_cost - 10.0).abs() < 1e-9);
    }

    /// THE VALUES of [`RecId::meaning`], which nothing checked until 2026-09-14 (review ledger V234).
    ///
    /// # What was missing, precisely
    /// The mechanism was already sound — the sentence lives on the enum, so the exhaustive `match`
    /// makes a new variant fail to COMPILE without one. What had no owner was the sentence itself:
    /// `grep -rn 'idMeaning' crates/ --include='*.rs' | grep -i test` returned nothing, so an arm that
    /// was emptied, truncated, or pasted from its neighbour shipped green. The id SPELLINGS are frozen
    /// by `VERSIONING.md` and the meanings deliberately are not — which left the explanation sitting in
    /// the same unchecked layer as the thing it explains.
    ///
    /// # Why these properties and not the exact text
    /// Pinning the bytes would make this a second copy of the prose and would freeze wording that is
    /// meant to improve. What is pinned instead is what a READER is promised:
    ///
    /// * it is a sentence, not a token — `architecture.topRecommendationMeaning` tells consumers to
    ///   "read `idMeaning`, never a hard-coded list of spellings", and a two-word gloss does not
    ///   discharge that instruction;
    /// * the seven are pairwise DISTINCT, which is the realistic silent failure here (a pasted arm
    ///   explains the wrong kind, and nothing about that is visible at the call site);
    /// * none of them is merely the id spelled out — a meaning that only restates `hot-churn` as "hot
    ///   churn" teaches a consumer nothing they did not already hold;
    /// * every variant is covered, because [`RecId::ALL`] is generated from the enum declaration rather
    ///   than written beside it.
    #[test]
    fn every_recommendation_id_carries_a_distinct_sentence_that_is_not_its_own_spelling() {
        // FLOOR: the generated list is the population of everything below, so a macro that stopped
        // expanding would make all three checks vacuously true.
        assert!(
            RecId::ALL.len() >= 7,
            "RecId::ALL holds {} variants — the generator has stopped covering the enum, and every \
             assertion below is then about nothing",
            RecId::ALL.len()
        );

        let mut seen: Vec<(String, &'static str)> = Vec::new();
        for id in RecId::ALL {
            let meaning = id.meaning();
            // The wire spelling, from serde rather than from a hand-written table here.
            let spelled = serde_json::to_string(id).expect("a unit variant serializes");
            let spelled = spelled.trim_matches('"').to_string();

            assert!(
                meaning.len() > 80,
                "`{spelled}`'s meaning is {} bytes. The reply instructs consumers to read this field \
                 INSTEAD of memorizing the id set, and a fragment cannot carry that: {meaning:?}",
                meaning.len()
            );
            assert!(
                meaning.contains(' '),
                "`{spelled}`'s meaning is a single token, not a sentence: {meaning:?}"
            );
            let words: Vec<String> = spelled.split('-').map(str::to_string).collect();
            assert!(
                !words.iter().all(|w| meaning.to_lowercase().contains(w.as_str()))
                    || meaning.len() > 120,
                "`{spelled}`'s meaning reads as the id spelled out and nothing more, which leaves a \
                 consumer exactly where they started: {meaning:?}"
            );
            if let Some((other, _)) = seen.iter().find(|(_, m)| *m == meaning) {
                panic!(
                    "`{spelled}` and `{other}` ship the SAME sentence. One of them is explaining the \
                     other's kind, and a pasted arm is invisible at every call site: {meaning:?}"
                );
            }
            seen.push((spelled, meaning));
        }
    }

    #[test]
    fn roi_zero_risk_is_zero() {
        let r = compute_roi(RecId::VersioningCandidate, Severity::Info, 0.0, 0, 0);
        assert_eq!(r.estimated_reduction, 0.0);
        assert_eq!(r.roi, 0.0);
    }
}
