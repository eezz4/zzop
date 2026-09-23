//! `byRuleMeaning` — the legend for `findings.byRule` / `crossLayerFindings.byRule`, written to the
//! same contract as `painMeaning`/`topRecommendationMeaning`/`nativeAnalysesMeaning`: a machine field
//! ships beside a sentence that says WHAT IT MEASURES, because the number alone is read as measuring
//! something else.
//!
//! # The misread this exists to stop
//! A count in `byRule` is a count of FINDINGS. It is not a count of places, and the ratio between the
//! two is a property of the RULE, not of the tree — so putting two rules' counts in one column
//! silently compares two different quantities. Measured 2026-08-29 on the nine-repo corpus, whole-tree
//! union: `circular` reported **26 findings standing for 719 files** (its `evidencePaths` carry the
//! other 693), while `unimported-export`'s **2,077 findings stood for exactly 2,077 places**. Side by
//! side, unlabelled, that is a 27.65x distortion presented as one column.
//!
//! It has bitten a skilled reader at least twice with the honest number ALREADY on the wire: an
//! external auditor reported `reliability/goroutine-in-loop` as having MISSED a site whose finding had been
//! folded into its method's first trigger line (`data.triggerLines: 2` said so at that very anchor),
//! and a second auditor read a rule leaderboard as a ranking of affected places. The fold is
//! deliberate and documented (`docs/rules/dsl-reference.md`'s finding-shape table: "one finding per
//! method either way, so this is the only way to tell a one-off from a repeated idiom"); what was
//! missing is a reader for it.
//!
//! # Why prose, and why NO derived count beside it
//! The obvious repair — publish a site count next to each `byRule` entry — cannot be derived
//! honestly at this layer, and the corpus says so three separate ways:
//!
//! * `duplicate-route` writes `data.sites` as an INTEGER in the per-tree lane and
//!   `cross-layer/duplicate-route` writes `data.sites` as an ARRAY in the join lane — one key, two
//!   shapes, so a reader keyed on the name gets one of them wrong.
//! * `mutating-route-no-auth`'s `data.unresolvedCallees[]` and
//!   `cross-layer/unconsumed-endpoint`'s `data.unresolvedHttpConsumeCount` are an array and an
//!   integer that look exactly like folds and are diagnostic context — a shape-based predicate
//!   false-positives on both.
//! * `cross-layer/unconsumed-endpoint` folds under `foldedEndpointCount` and
//!   `cross-layer/external-host-fanout` under `siteCount` — names a predicate would have to be TOLD,
//!   so it would silently under-report the day a twelfth fold arrives under a thirteenth name.
//!
//! That is the same reasoning `zzop_core::Finding::evidence_paths` already records for its own
//! existence ("a per-rule table of `data` keys ... a thirteenth rule, or a renamed key, leaves it
//! silently short"). The cardinality is the PRODUCER's to declare, and each producer already declares
//! it — typed in `evidencePaths` for the folds that span files, per-matcher in `data` for the rest.
//! So this legend states the class and names the two channels that hold the answer, rather than
//! stamping a number this layer would have to guess. Nothing here is a rule list: no id is named, and
//! a new folding rule needs no edit to this file.
//!
//! # Why unconditional
//! `nativeAnalysesMeaning`'s rule, for its reason: the reply where every count IS a place count is
//! exactly the reply whose reader most needs to be told that this is not guaranteed. A legend that
//! appeared only when a fold happened would leave the reader to infer its absence means "safe to
//! compare", which is the misread itself.

/// The sentence that ships beside every `byRule` map.
///
/// Deliberately makes NO imperative before its caveat (`rule-quality.md` §27): the "read a count as"
/// instruction comes after the shape it depends on.
pub(super) const BY_RULE_MEANING: &str =
    "Counts FINDINGS, not places — and how far those two diverge is a property of the RULE, so this \
     map is comparable WITHIN one rule (across runs) and NOT across rules within one run. A \
     method-scan rule emits ONE finding per method body however many qualifying lines that body \
     holds; a rule that reports a cycle, a duplicated route key or an N-source collision emits ONE \
     finding for the whole group. Every folding finding declares its own cardinality: the other \
     FILES it names ride its `evidencePaths`, and the group's size rides its `data` (a line count, a \
     cycle, a site list — the shape is the matcher's). Measured on a nine-repo corpus, one rule's 26 \
     findings stood for 719 files while another's 2,077 stood for 2,077 places — a 27x difference \
     between two numbers in this same column. So read an entry as \"how many times this rule had \
     something to say\", never as \"how many places are affected\"; for the second question read the \
     findings themselves. Counts here are over the FULL set even when `shown` is capped.";

#[cfg(test)]
mod tests {
    use super::BY_RULE_MEANING;

    /// Pins the two channels by name. They are the whole point: the legend's job is to hand the
    /// reader the fields that carry the honest cardinality, and a rewrite that drops either one
    /// leaves a caveat with no destination.
    #[test]
    fn names_both_channels_that_carry_the_real_cardinality() {
        assert!(
            BY_RULE_MEANING.contains("evidencePaths"),
            "the legend must name the typed cross-file channel: {BY_RULE_MEANING}"
        );
        assert!(
            BY_RULE_MEANING.contains("`data`"),
            "the legend must name the per-matcher channel: {BY_RULE_MEANING}"
        );
    }

    /// 🔴 POSITION, not presence (`rule-quality.md` §30's invalidation probe): the caveat has to
    /// reach the reader BEFORE the instruction that depends on it. Moving the "read an entry as"
    /// sentence ahead of "Counts FINDINGS, not places" would keep every token and lose the point.
    #[test]
    fn the_caveat_precedes_the_instruction() {
        let caveat = BY_RULE_MEANING
            .find("Counts FINDINGS, not places")
            .expect("caveat clause");
        let instruction = BY_RULE_MEANING
            .find("read an entry as")
            .expect("instruction clause");
        assert!(
            caveat < instruction,
            "the caveat must precede the instruction it qualifies ({caveat} < {instruction}): \
             {BY_RULE_MEANING}"
        );
    }

    /// No rule id, ever. The moment this sentence names one it becomes a list that goes stale the
    /// next time a rule learns to fold — the failure mode `Finding::evidence_paths` documents and
    /// this file's module doc measures three times over.
    #[test]
    fn names_no_rule_id() {
        assert!(
            !BY_RULE_MEANING.contains('/'),
            "a rule id (or anything shaped like one) in this sentence makes it a list that rots: \
             {BY_RULE_MEANING}"
        );
    }
}
