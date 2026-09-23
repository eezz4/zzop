//! The engine aggregator half of the native-rule io-channel mechanism (`zzop_core::rule_channels`'
//! module doc holds the contract): each owning rules crate declares, on the same table row its
//! `register_native_analyses` reads, which cross-layer io channels that rule's input is drawn from, and
//! this module composes those tables into the one list a consumer reads — the exact shape
//! [`crate::rule_sightlines`] and [`crate::framework_recognizers`] already give the sightline and
//! recognizer halves, and for the same reason: the engine may enumerate, never own, per-rule data.
//!
//! # The question this answers that nothing else could
//! `framework_silence`'s tripwires can say "route extraction came up empty, so cross-layer joins will be
//! near-silent". They cannot name the rules, because the rule→channel fact existed only as
//! `kind == "http"` comparisons scattered through rule bodies. [`enabled_native_rules_reading`] is that
//! naming, gated through the same `RuleConfig` a user's `disabledRules` reaches — a disclosure that
//! names a rule the run has switched off is a worse answer than none.
//!
//! # The claim's direction is NOT here (read this before writing a sentence out of it)
//! A declaration says the rule READS the channel. It does not say the rule goes quiet when the channel
//! is empty, and the two classes both ship: `cross-layer/unconsumed-endpoint` iterates http provides and
//! reports nothing without them, while `unprovided-consume` reports EVERY consume without them. See
//! `zzop_core::rule_channels`' doc for why that direction is deliberately not a declared field.

use zzop_core::{NativeRuleChannels, RuleConfig, RuleIoChannel};

/// Every native analysis in this build with its declared io channels, in the crate order
/// [`crate::register_all_native`] composes registration, so output derived from it is deterministic.
///
/// `zzop_metrics` is absent and that is not an omission: the ids it registers gate SCORE computations
/// (`seams`/`criticality`/`scores`/`health`/`recommendations`), not findings-producing rules — they ride
/// the `RuleConfig` id space without reading any rule input at all. The gap is pinned rather than
/// assumed: `rule_contracts::rule_channels` subtracts this list from the real registry and requires the
/// remainder to be EXACTLY what a fresh registry gets from `zzop_metrics` alone, so a rules crate that
/// stopped contributing here could not hide behind the exemption.
pub fn native_rule_channels() -> Vec<NativeRuleChannels> {
    let mut out = zzop_rules_graph::native_rule_channels();
    out.extend(zzop_rules_http::native_rule_channels());
    out.extend(zzop_rules_cross_layer::native_rule_channels());
    out.extend(zzop_rules_schema::native_rule_channels());
    out
}

/// The ids of every native rule that reads `channel` AND is enabled under `gate` — the query a
/// silence disclosure runs to turn "this tree contributed no http provides" into the rule names that
/// consequently have nothing to judge. Registration order (see [`native_rule_channels`]).
pub fn enabled_native_rules_reading(gate: &RuleConfig, channel: RuleIoChannel) -> Vec<String> {
    native_rule_channels()
        .into_iter()
        .filter(|row| row.reads.contains(&channel))
        .filter(|row| zzop_core::is_enabled(gate, &row.rule_id))
        .map(|row| row.rule_id)
        .collect()
}

/// Every registered native id that no finding can carry, sorted — the set the registry deliberately
/// does NOT separate, published so a consumer can stop treating "registered" as "reportable".
///
/// # The two classes, and why the id space they sit in is the wrong question to ask
/// `RuleRegistry` holds the ids the CONFIG gates. That is a strictly larger set than the ids a finding
/// can carry, in two ways, and `NativeAnalyses::registered`'s doc has named both since 2026-08-29:
/// `zzop_metrics` registers ids that gate SCORE computations and emit no finding at all, and
/// `zzop_rules_schema` registers two FAMILY gates whose passes report under the finer `schema/<label>`
/// ids. Both classes are read from their owning crate here — the metrics half from a fresh registry
/// (the same source `every_registered_rule_id_is_declared_except_the_metrics_score_gates` reads, so
/// the two cannot be edited into disagreement), the schema half from that crate's own
/// [`zzop_rules_schema::SCHEMA_FAMILY_GATES`] — rather than spelled again in this file.
///
/// # What this is FOR, stated because the name does not say it
/// A caller asking "could `--rule <id>` ever have matched?" must test membership in the reportable
/// ids, and the registry was the set within reach. So the answer was yes for these seven, the refusal
/// stayed silent, and the reply came back `shown: 0` with a full census behind it — a filter reading
/// as a clean result, which is the one outcome `unmatchable_rule_filter` was built to prevent.
///
/// It is NOT a claim that these ids do nothing: they gate real work, they belong in `RuleConfig`, and
/// disabling one switches its whole pass off. The claim is narrower and is the only one a caller may
/// draw from it — no `findings` entry is ever keyed by them.
pub fn ids_that_carry_no_finding() -> Vec<String> {
    let mut metrics = zzop_core::RuleRegistry::new();
    zzop_metrics::register_native_analyses(&mut metrics);
    let mut out: Vec<String> = metrics.ids().to_vec();
    out.extend(
        zzop_rules_schema::SCHEMA_FAMILY_GATES
            .iter()
            .map(|id| (*id).to_string()),
    );
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use zzop_core::rule_channels::reads;

    /// The floor under [`ids_that_carry_no_finding`], and it has to bite from BOTH sides or it is a
    /// list that agrees with itself.
    ///
    /// Above: every id it names is really registered — a renamed gate cannot leave a ghost here.
    /// Below: the set is non-empty and a PROPER subset, so a derivation that collapsed to "everything"
    /// (which would silence the `--rule` refusal for every id, the failure direction that costs a
    /// reader the most) fails rather than passes.
    #[test]
    fn ids_that_carry_no_finding_are_registered_and_a_proper_subset() {
        let all: BTreeSet<String> = registry_ids().into_iter().collect();
        let none: Vec<String> = ids_that_carry_no_finding();
        assert!(
            !none.is_empty(),
            "empty means the two classes stopped being readable from their owning crates — the \
             refusal this feeds would go silent again with nothing to show for it"
        );
        for id in &none {
            assert!(
                all.contains(id),
                "`{id}` is named as carrying no finding but is not registered at all — a rename left \
                 a ghost, and the refusal would then reject an id this build never had"
            );
        }
        assert!(
            none.len() < all.len(),
            "every registered id was judged unreportable — the `--rule` refusal would fire on \
             everything"
        );
    }

    /// The claim itself, checked against the ids findings ACTUALLY carry rather than against another
    /// list: no schema issue id (the finer ids the umbrella passes report under) may appear in the
    /// set, or the refusal would reject a filter that does match. This is the direction that would
    /// break a working command line, so it is pinned separately from the membership test above.
    #[test]
    fn no_reportable_schema_issue_id_is_called_unreportable() {
        let none: BTreeSet<String> = ids_that_carry_no_finding().into_iter().collect();
        let reportable = zzop_rules_schema::SCHEMA_STRUCTURAL_ISSUE_LABELS
            .iter()
            .chain(zzop_rules_schema::SCHEMA_USAGE_ISSUE_LABELS.iter())
            .map(|label| zzop_rules_schema::schema_issue_rule_id(label));
        for id in reportable {
            assert!(
                !none.contains(&id),
                "`{id}` is a rule id findings really carry, yet it was judged unreportable — \
                 `--rule {id}` would be refused on a run that can match it"
            );
        }
    }

    fn registry_ids() -> Vec<String> {
        let mut registry = zzop_core::RuleRegistry::new();
        crate::register_all_native(&mut registry);
        registry.ids().to_vec()
    }

    /// The declaration IS the registration — same table, same order — so the aggregate must reproduce
    /// the registry exactly, minus the score-gate ids `zzop_metrics` contributes. Read off a real
    /// registry and a real `zzop_metrics` registration rather than any hand list, so neither side can
    /// be edited into agreement.
    #[test]
    fn every_registered_rule_id_is_declared_except_the_metrics_score_gates() {
        let declared: Vec<String> = native_rule_channels()
            .into_iter()
            .map(|r| r.rule_id)
            .collect();
        let declared_set: BTreeSet<&String> = declared.iter().collect();
        assert_eq!(
            declared.len(),
            declared_set.len(),
            "a rule id is declared twice — two rows for one rule is the drift this table removes"
        );

        let mut metrics_only = zzop_core::RuleRegistry::new();
        zzop_metrics::register_native_analyses(&mut metrics_only);
        let score_gates: BTreeSet<&String> = metrics_only.ids().iter().collect();
        assert!(
            !score_gates.is_empty(),
            "zzop_metrics registered nothing — the exemption below would then absorb any rules crate \
             that silently stopped declaring"
        );

        let all: BTreeSet<String> = registry_ids().into_iter().collect();
        let undeclared: Vec<&String> = all
            .iter()
            .filter(|id| !declared_set.contains(id) && !score_gates.contains(id))
            .collect();
        assert!(
            undeclared.is_empty(),
            "registered native analysis id(s) with no io-channel declaration: {undeclared:?} — add \
             the id to its owning crate's NATIVE_ANALYSES table (the same table registration reads), \
             never to a second list"
        );
        let phantom: Vec<&&String> = declared_set
            .iter()
            .filter(|id| !all.contains(**id))
            .collect();
        assert!(
            phantom.is_empty(),
            "declared io channels for id(s) the registry does not know: {phantom:?}"
        );
    }

    /// Every declared channel must be one of the named constants. A hand-built `RuleIoChannel` with a
    /// misspelled kind would sit in a bucket no consumer ever queries — the same silent shape as not
    /// declaring at all.
    #[test]
    fn every_declared_channel_is_one_of_the_named_constants() {
        for row in native_rule_channels() {
            let mut seen = BTreeSet::new();
            for c in row.reads {
                assert!(
                    reads::ALL.contains(c),
                    "{} declares unknown channel {c:?} — use zzop_core::rule_channels::reads::*",
                    row.rule_id
                );
                assert!(seen.insert(c), "{} declares {c:?} twice", row.rule_id);
            }
        }
    }

    /// The disclosure this table exists to feed must have something to say on every channel. A channel
    /// no rule reads would make a self-report name zero rules, which reads to a user as "nothing is
    /// affected" when it actually means "we never bound this channel to anything".
    #[test]
    fn every_named_channel_is_read_by_at_least_one_rule() {
        let rows = native_rule_channels();
        for c in reads::ALL {
            let readers: Vec<&str> = rows
                .iter()
                .filter(|r| r.reads.contains(c))
                .map(|r| r.rule_id.as_str())
                .collect();
            assert!(
                !readers.is_empty(),
                "no native rule declares {c:?} ({}), so a disclosure about that channel would name \
                 nothing — either a rule stopped declaring it or the channel is not actually joined",
                c.label()
            );
        }
    }

    /// The gate leg of [`enabled_native_rules_reading`] — a disclosure must not name a rule the user
    /// has switched off.
    #[test]
    fn the_query_drops_rules_the_config_disabled() {
        let open = RuleConfig::default();
        let all = enabled_native_rules_reading(&open, reads::HTTP_PROVIDES);
        assert!(
            all.len() > 5,
            "only {} rule(s) read http provides — the aggregate is not seeing the rules crates",
            all.len()
        );
        let victim = all[0].clone();
        let gate = RuleConfig {
            disabled_rules: vec![victim.clone()],
            ..RuleConfig::default()
        };
        let filtered = enabled_native_rules_reading(&gate, reads::HTTP_PROVIDES);
        assert!(
            !filtered.contains(&victim),
            "{victim} stayed in the answer after being disabled"
        );
        assert_eq!(filtered.len(), all.len() - 1);
    }
}
