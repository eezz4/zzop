//! The OUTPUT-ID lanes: strings `zzop` prints under a field literally named `id` in an analyze reply
//! that are NOT rule ids, and that `zzop explain` therefore used to answer "unknown rule id" about —
//! the same "the tool has never heard of its own output" failure the `schema/<label>` lane fixes, one
//! namespace over. Two of them, both reached by a reader copying an `id` out of real JSON:
//! - `disclosure[].id` / `disclosure[].group` — the coverage-disclosure registry
//!   (`zzop_engine::blindness_registry`), 17 ids in 4 groups today. Since the 2026-07-29 fold a run
//!   reply carries the registry's COUNTS rather than each class's row, so a reader most often arrives
//!   here holding an id copied out of `zzop contract disclosure-classes` (or an older reply) — which is
//!   why this lane now prints the class's own summary instead of pointing back at the reply for it;
//! - `architecture.topRecommendation.id` (and `recommendations[].id` in the facade output view) — a
//!   `zzop_metrics::roi::RecId`, 8 ids today.
//!
//! Both are lookup FAILURES like every other non-rule lane here (`Err`, stderr, exit 1): there is no DSL
//! rule to render. They exist to say WHAT the id is and where its real answer already lives, instead of
//! denying it exists.
//!
//! Neither set is hand-copied. The disclosure lanes read the live registry, so a class added or regrouped
//! there is picked up with no edit. The recommendation lane never enumerates the variants at all: it asks
//! serde whether the string deserializes into `RecId`, which makes the enum's own
//! `#[serde(rename_all = "kebab-case")]` — the exact spelling the output carries — the single authority.
//!
//! `circular` is both a `RecId` and a registered native analysis id. `explain_over` runs the native lane
//! first, so it answers as the native analysis — the accurate reading, since that recommendation IS the
//! `circular` analysis's output ranked.

use zzop_engine::blindness_registry;
use zzop_metrics::roi::RecId;

/// The registered native analysis id that gates the whole recommendation family — the single thing a
/// caller can actually turn off, mirroring the family-gate framing of the `schema/<label>` lane.
const RECOMMENDATION_GATE: &str = "recommendations";

/// `Some(message)` when `query` is one of the output-ids described in this module's doc; `None` when it
/// is not, leaving the caller's remaining lanes untouched.
pub(super) fn output_id_lane(query: &str) -> Option<String> {
    disclosure_class(query)
        .or_else(|| disclosure_group(query))
        .or_else(|| recommendation_id(query))
}

/// `disclosure[].id` — one coverage-disclosure class. Since the 2026-07-29 fold the reply carries the
/// registry's COUNTS, not each class's paragraph, so this lane prints the class's own `summary` — it is
/// now the shortest path from an id in a reply to what that id means, and pointing back at a reply
/// field that no longer holds the prose would have been a pointer to nothing.
fn disclosure_class(query: &str) -> Option<String> {
    let class = blindness_registry().iter().find(|c| c.id == query)?;
    Some(format!(
        "{query:?} is a coverage-DISCLOSURE class id (group {:?}, status {:?}), not a rule id — it names \
         one way zzop's own output can be silently misread. Every `zzop analyze` reply carries how many \
         such classes there are and how many are NOT fully detected (`disclosure.classes` and the \
         per-status counts); the full text of all of them is one lookup away, at \
         `zzop contract disclosure-classes`. This class reads: {}\nIt is not a finding: it has no \
         severity, no suppression marker, and no `rules: {{ \"<id>\": \"off\" }}` toggle. `zzop explain` \
         only reads the compiled-in DSL pack data — see `zzop contract rule-catalog` for the rule ids.",
        class.group,
        class.status.as_str(),
        class.summary
    ))
}

/// `disclosure[].group` — the taxonomy bucket above a class. Lists its members, the same hint shape the
/// PACK lane uses for a pack id.
fn disclosure_group(query: &str) -> Option<String> {
    let ids: Vec<&str> = blindness_registry()
        .iter()
        .filter(|c| c.group == query)
        .map(|c| c.id)
        .collect();
    if ids.is_empty() {
        return None;
    }
    Some(format!(
        "{query:?} is a coverage-disclosure GROUP, not a rule id — it is the taxonomy bucket over {} \
         disclosure classes, counted in every `zzop analyze` reply's `disclosure` block and spelled out \
         in full by `zzop contract disclosure-classes`: {}. Explain one of those \
         for what it means; `zzop contract rule-catalog` has the rule ids.",
        ids.len(),
        ids.join(", ")
    ))
}

/// `architecture.topRecommendation.id` — a ranked architecture recommendation, computed by
/// `zzop_metrics::recommendations` rather than declared by any rule pack.
fn recommendation_id(query: &str) -> Option<String> {
    serde_json::from_value::<RecId>(serde_json::Value::String(query.to_string())).ok()?;
    Some(format!(
        "{query:?} is a RECOMMENDATION id, not a rule id — it names one of the ranked architecture \
         recommendations zzop computes (`architecture.topRecommendation.id`, and `recommendations[].id` \
         in the full output view), not anything a rule pack declares. The family is gated as a whole by \
         the registered native analysis id {RECOMMENDATION_GATE:?} (that is what `disabledRules` / \
         `rules: {{ \"<id>\": \"off\" }}` takes); there is no per-recommendation toggle. `zzop explain` \
         only reads the compiled-in DSL pack data — see `zzop contract rule-catalog`."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disclosure_class_id_is_answered_with_its_group_and_status() {
        let message = output_id_lane("stale-cache").expect("a registered disclosure class id");
        assert!(
            message.contains("coverage-DISCLOSURE class id"),
            "{message}"
        );
        assert!(message.contains("trust-calibration"), "{message}");
        // Names the reply field that really holds the channel today (the folded counts) AND the lane
        // that holds the prose — the pin used to demand `disclosure[]`, a shape the reply stopped
        // emitting on 2026-07-29.
        assert!(message.contains("disclosure.classes"), "{message}");
        assert!(
            message.contains("zzop contract disclosure-classes"),
            "{message}"
        );
        // The class's own summary is now IN the answer, so an id from any source resolves to meaning
        // without a second command.
        assert!(
            message.contains("Stale results from an un-bumped fingerprint"),
            "{message}"
        );
    }

    #[test]
    fn every_registered_disclosure_id_and_group_is_answered() {
        for class in blindness_registry() {
            assert!(output_id_lane(class.id).is_some(), "class id {}", class.id);
            assert!(
                output_id_lane(class.group).is_some(),
                "group {}",
                class.group
            );
        }
    }

    #[test]
    fn disclosure_group_lists_its_member_classes() {
        let message = output_id_lane("extraction-blind").expect("a registered disclosure group");
        assert!(message.contains("coverage-disclosure GROUP"), "{message}");
        assert!(message.contains("consume-side-unextracted"), "{message}");
    }

    /// The point of the serde round-trip: the kebab-case spelling the output carries is the ONLY thing
    /// this lane accepts, and it is the enum's own rename attribute that decides it.
    #[test]
    fn recommendation_id_is_answered_in_its_wire_spelling_only() {
        let message = output_id_lane("hot-churn").expect("a RecId in its wire spelling");
        assert!(message.contains("RECOMMENDATION id"), "{message}");
        assert!(message.contains(RECOMMENDATION_GATE), "{message}");
        assert!(
            output_id_lane("HotChurn").is_none(),
            "rust variant spelling is not wire"
        );
        assert!(
            output_id_lane("hotChurn").is_none(),
            "camelCase is not wire"
        );
    }

    #[test]
    fn a_string_that_is_none_of_the_three_falls_through() {
        assert!(output_id_lane("no-such-identifier-anywhere").is_none());
        assert!(output_id_lane("").is_none());
    }
}
