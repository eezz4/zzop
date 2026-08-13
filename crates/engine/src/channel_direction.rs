//! What an EMPTY io channel does to the rules that read it — the DIRECTION half of the rule→channel
//! fact, and the only half of it that is measured rather than stated.
//!
//! # The sentence nobody could write
//! `zzop_core::rule_channels` binds each rule to the channels its input is drawn from, and its module
//! doc is explicit that a declaration says nothing about direction: `cross-layer/unconsumed-endpoint`
//! goes SILENT with no http provides while `unprovided-consume` FLOODS (every consume then looks
//! unprovided). Both shipped, opposite, undistinguished — so a disclosure saying "channel X is empty,
//! so rule Y prints nothing" would have been half false, and every `framework_silence` tripwire
//! therefore stopped at "cross-layer joins will be near-silent" without naming a rule.
//!
//! # Why this is not a field on the declaration
//! Because a declared direction would be exactly the hardcoding the channel table removed: a rule
//! module's own code evidences its channels (that is what `rule_contracts::rule_channels` binds), and
//! it evidences NOTHING about what an empty channel does — that is a property of the JOIN PIPELINE,
//! visible only by running it. So the value here is produced the one way it can be bound: an
//! experiment. `rule_contracts::channel_direction` runs the same fixture twice per channel, once with
//! the channel supplied and once with it withheld, counts each rule's findings in both arms, and
//! requires [`OBSERVED`] to equal that derivation ROW FOR ROW — plus a row for every declared
//! `(rule, channel)` pair, so a rule cannot be registered without a measured direction.
//!
//! # So why is the table written down at all
//! Because the consumer runs at ANALYSIS time and the experiment cannot. A tripwire fires while one
//! tree is being assembled, long before any cross-layer finding exists; re-deriving the direction
//! there would mean running the whole join twice on every real run to learn a fact that does not
//! depend on the run at all — direction is a property of the rule's code, so it is fixed at build
//! time. This table is therefore a CACHE of a measurement, not a claim: it cannot drift, because the
//! measurement is re-run by the contract on every `cargo test`, and it cannot go missing, because a
//! declared pair with no row is a red.
//!
//! # Why the engine owns it, when the engine owns no other per-rule data
//! [`crate::rule_channels`] states the rule — the engine ENUMERATES per-rule data, never owns it —
//! and this is the one fact that rule cannot cover: a rules crate cannot depend on `analyze_trees`,
//! so it could not run the experiment that produces its own value, and a fact belongs where it can be
//! checked. The channels stay in the owning crate's `NATIVE_ANALYSES` row; only the pipeline's
//! response to their emptiness lives here.

mod table;

use zzop_core::{RuleConfig, RuleIoChannel};

pub use table::OBSERVED;

/// What emptying one channel did to one rule's finding count, as MEASURED by
/// `rule_contracts::channel_direction`'s two-arm probe. Every variant is a comparison between the
/// withheld-channel arm and the supplied-channel arm — no threshold, no judgment call:
/// "flooding" is simply MORE findings from LESS input, which no correct rule can do on purpose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelDirection {
    /// Withheld arm zero, supplied arm non-zero: the rule has nothing left to judge. The only verdict
    /// a "so rule Y prints nothing" sentence may quote.
    Silences,
    /// Withheld arm STRICTLY GREATER than the supplied arm — the opposite reading, and the one that
    /// makes an unqualified silence disclosure wrong.
    Floods,
    /// Fewer findings without the channel, but not none: the rule still speaks, on less evidence.
    Reduces,
    /// The two arms produced the SAME count. Never a claim that the channel is irrelevant — the rule
    /// is named in no disclosure and nothing is said about it.
    ///
    /// Two states share this verdict, and a reader should not assume the first one: usually both arms
    /// were zero (this fixture never made the rule fire, so nothing is known), but a rule can also
    /// fire the same NON-zero number of times with the channel supplied and withheld, which is a real
    /// measurement of indifference on this fixture rather than an absence of one. Both are published
    /// the same way because both are "the experiment did not move this rule", which is the only thing
    /// a consequence disclosure may act on; distinguishing them would require shipping the counts,
    /// and no consumer has a use for them (`enabled_rules_directed` filters on direction alone).
    Unobserved,
}

/// One measured `(rule, channel) -> direction` row. `rule_id` is `&'static str` rather than the owned
/// `String` [`zzop_core::NativeRuleChannels`] carries, because the composed rule ids that forced that
/// choice (`schema/<label>`) all read `NO_IO` and so have no channel to be directed on.
#[derive(Debug, Clone, Copy)]
pub struct RuleChannelDirection {
    pub rule_id: &'static str,
    pub channel: RuleIoChannel,
    pub direction: ChannelDirection,
}

/// Every measured row, sorted by `(rule id, channel label)` — the probe's own emission order, so
/// output derived from it is deterministic and a re-derived block diffs cleanly against the shipped
/// one.
pub fn channel_directions() -> &'static [RuleChannelDirection] {
    OBSERVED
}

/// The ids of every rule that `direction` was measured for on `channel` AND that `gate` leaves
/// enabled — the query a silence disclosure runs. Gated for the same reason
/// [`crate::enabled_native_rules_reading`] is: naming a rule the run switched off is a worse answer
/// than naming none.
pub fn enabled_rules_directed(
    gate: &RuleConfig,
    channel: RuleIoChannel,
    direction: ChannelDirection,
) -> Vec<String> {
    OBSERVED
        .iter()
        .filter(|row| row.channel == channel && row.direction == direction)
        .filter(|row| zzop_core::is_enabled(gate, row.rule_id))
        .map(|row| row.rule_id.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use zzop_core::rule_channels::reads;

    /// The table must cover the DECLARED pairs exactly — every one, once, and nothing else. This is
    /// the leg that makes a new rule impossible to register without a measured direction: its
    /// `NATIVE_ANALYSES` row adds a pair here, and a pair with no row is a red. (What each row's
    /// VALUE must be is the contract test's job — it is the half that needs a running engine.)
    #[test]
    fn the_table_covers_every_declared_rule_channel_pair_exactly_once() {
        let mut declared: BTreeSet<(String, RuleIoChannel)> = BTreeSet::new();
        for row in crate::native_rule_channels() {
            for c in row.reads {
                assert!(
                    declared.insert((row.rule_id.clone(), *c)),
                    "{} declares {c:?} twice",
                    row.rule_id
                );
            }
        }
        let mut measured: BTreeSet<(String, RuleIoChannel)> = BTreeSet::new();
        for row in OBSERVED {
            assert!(
                measured.insert((row.rule_id.to_string(), row.channel)),
                "{} has two direction rows for {:?}",
                row.rule_id,
                row.channel
            );
        }
        let undirected: Vec<&(String, RuleIoChannel)> =
            declared.difference(&measured).collect::<Vec<_>>();
        assert!(
            undirected.is_empty(),
            "declared rule/channel pair(s) with no measured direction: {undirected:?} — run \
             `rule_contracts::channel_direction` and paste the row it derives; a pair with no row \
             means a disclosure about that channel silently omits the rule"
        );
        let phantom: Vec<&(String, RuleIoChannel)> =
            measured.difference(&declared).collect::<Vec<_>>();
        assert!(
            phantom.is_empty(),
            "direction row(s) for pair(s) nothing declares: {phantom:?}"
        );
    }

    /// Vacuity floor: a table that measured nothing would answer "no rule is affected" to every
    /// question and read as an all-clear. BOTH directions must be present — the whole reason this
    /// module exists is that they coexist.
    #[test]
    fn both_directions_are_actually_present_in_the_measurement() {
        let silences = enabled_rules_directed(
            &RuleConfig::default(),
            reads::HTTP_PROVIDES,
            ChannelDirection::Silences,
        );
        let floods = enabled_rules_directed(
            &RuleConfig::default(),
            reads::HTTP_PROVIDES,
            ChannelDirection::Floods,
        );
        assert!(
            silences.len() >= 5,
            "only {} rule(s) measured as silenced by an empty http provide channel {silences:?} — \
             the probe stopped moving the rules, not the rules stopped reading io",
            silences.len()
        );
        assert!(
            !floods.is_empty(),
            "no rule measured as FLOODING on an empty http provide channel — that direction shipping \
             alongside the silent one is this module's entire premise, so its disappearance is a \
             finding, not a cleanup"
        );
    }

    /// The call-graph-BFS family must stay MEASURED, not drift back to ignorance.
    ///
    /// These three read the http provide channel through a second input no overlay carries — the call
    /// graph, which `analyze::native_rules::callgraph` builds by re-parsing the tree's own sources.
    /// The probe reaches them only because one of its hosts is a real fixture
    /// (`cases/trees/callgraph-handlers`, the `callgraph` tree with its route registrations lifted
    /// out) instead of an empty directory. That arrangement is quiet when it breaks: if the handlers
    /// stop reaching the store write, or the donor's `IoProvide`s stop carrying the handler symbol,
    /// all three fall back to equal-and-zero arms and the contract test's only complaint is that the
    /// table disagrees — which can be "fixed" by pasting `Unobserved` back in, silently deleting the
    /// measurement and dropping three security rules out of the S15 disclosure. This says so instead.
    #[test]
    fn the_call_graph_family_stays_measured_on_an_empty_http_provide_channel() {
        let silenced = enabled_rules_directed(
            &RuleConfig::default(),
            reads::HTTP_PROVIDES,
            ChannelDirection::Silences,
        );
        for rule in [
            "mutating-route-no-auth",
            "unsafe-read-endpoint",
            "non-idempotent-write",
        ] {
            assert!(
                silenced.iter().any(|r| r == rule),
                "{rule} is no longer measured as silenced by an empty http provide channel. It reads \
                 that channel and structurally cannot fire without it (`run_callgraph_rules` returns \
                 early on an empty endpoint set), so a drop to `Unobserved` means the PROBE stopped \
                 reaching it — repair `cases/trees/callgraph-handlers` and the `probe-cg` donor \
                 rather than pasting the regression into the table."
            );
        }
    }

    /// The gate leg — a disclosure must not name a rule the user switched off.
    #[test]
    fn the_query_drops_rules_the_config_disabled() {
        let open = RuleConfig::default();
        let all = enabled_rules_directed(&open, reads::HTTP_PROVIDES, ChannelDirection::Silences);
        let victim = all[0].clone();
        let gate = RuleConfig {
            disabled_rules: vec![victim.clone()],
            ..RuleConfig::default()
        };
        let filtered =
            enabled_rules_directed(&gate, reads::HTTP_PROVIDES, ChannelDirection::Silences);
        assert!(
            !filtered.contains(&victim),
            "{victim} stayed in the answer after being disabled"
        );
        assert_eq!(filtered.len(), all.len() - 1);
    }
}
