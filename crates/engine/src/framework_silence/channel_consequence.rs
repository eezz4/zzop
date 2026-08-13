//! S15: empty-channel CONSEQUENCE self-report — the sentence every sibling tripwire above could not
//! write, which is WHICH rules an empty io channel actually changes.
//!
//! ## Why this is a separate tripwire and not a paragraph inside S1/S2/S4
//! Because the same consequence belongs to several of them at once. S1 and S2 both mean "no http
//! provides were extracted"; S4, S5 and S7 all mean "no http consumes were". Pasting the rule roster
//! into each message would put four copies of one fact in one `warnings` array, and the copies would
//! drift the first time one of them was edited. This rides them instead: it fires only when a sibling
//! already raised the alarm for that channel, and it says the one thing they cannot.
//!
//! ## Why it needs a measurement and not a list
//! The obvious sentence — "channel X is empty, so rule Y prints nothing" — is FALSE for half the
//! rules. `cross-layer/unconsumed-endpoint` goes silent without http provides; `unprovided-mutation-call`
//! reports MORE, because with no provider anywhere every mutating call looks unprovided. Both ship,
//! and no rule module's code states which it is. [`crate::channel_direction`] holds that fact as a
//! measurement re-derived by `rule_contracts::channel_direction` on every test run, so this warning
//! quotes an experiment rather than an opinion — and quotes NOTHING for a rule the experiment never
//! moved, which is why a channel with no measured mover produces no warning at all instead of an
//! empty-sounding one.
//!
//! ## Direction: over-disclosure is safe
//! Like every sibling this is a `warnings: Vec<String>` self-report, not a `Finding` — it suppresses
//! nothing and changes no verdict.

use zzop_core::{RuleConfig, RuleIoChannel};

use crate::channel_direction::{enabled_rules_directed, ChannelDirection};

/// `Some(warning)` naming the rules an EMPTY `channel` measurably silences and the rules it measurably
/// inflates, under `gate`. `None` when the measurement has nothing to say about this channel (every
/// reader `Unobserved`, or every mover disabled) — an "affects 0 rules" sentence reads as an all-clear,
/// which is the failure mode this whole module exists against.
///
/// The CALLER owns the gate on the channel actually being empty: this function states a consequence,
/// it does not detect a condition. See `analyze::assemble::warnings` for where it is attached, and to
/// which sibling tripwires.
pub fn channel_consequence_warning(channel: RuleIoChannel, gate: &RuleConfig) -> Option<String> {
    let silenced = enabled_rules_directed(gate, channel, ChannelDirection::Silences);
    let flooded = enabled_rules_directed(gate, channel, ChannelDirection::Floods);
    if silenced.is_empty() && flooded.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    if !silenced.is_empty() {
        parts.push(format!(
            "{} rule(s) can produce NO finding at all from this tree — their zero here is out of \
             range, not clean: {}",
            silenced.len(),
            silenced.join(", ")
        ));
    }
    if !flooded.is_empty() {
        parts.push(format!(
            "{} rule(s) go the OPPOSITE way and report MORE than they would on a fully extracted \
             tree, because with the channel empty every counterpart looks unmatched: {} — read those \
             findings as a symptom of the extraction gap above, not as evidence about this code",
            flooded.len(),
            flooded.join(", ")
        ));
    }
    Some(format!(
        "Empty-channel consequence ({}): {}. Both lists are MEASURED, not declared — \
         `rule_contracts::channel_direction` runs one fixture with this channel supplied and once with \
         it withheld and pins each rule's response, so a rule whose response that experiment never \
         moved is named in neither list rather than guessed at.",
        channel.label(),
        parts.join("; and ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zzop_core::rule_channels::reads;

    /// Seals the disclosure itself: the http provide channel must name BOTH directions, because the
    /// half-true sentence ("empty channel, so these rules print nothing") is exactly what this
    /// warning exists to stop being written.
    #[test]
    fn an_empty_http_provide_channel_names_both_directions() {
        let w = channel_consequence_warning(reads::HTTP_PROVIDES, &RuleConfig::default())
            .expect("http provides has measured movers in both directions");
        assert!(w.contains("io.provides:http"), "{w}");
        assert!(w.contains("cross-layer/unconsumed-endpoint"), "{w}");
        assert!(w.contains("cross-layer/unprovided-mutation-call"), "{w}");
        assert!(w.contains("OPPOSITE"), "{w}");
        assert!(w.contains("MEASURED"), "{w}");
    }

    /// Seals the silence side twice over. A channel whose every reader the probe never moved says
    /// nothing at all — naming zero rules would read as "nothing is affected" when it means "nothing
    /// was measured". And a gate that disabled every mover collapses to the same answer.
    #[test]
    fn nothing_measured_and_nothing_enabled_both_produce_no_warning() {
        assert!(
            channel_consequence_warning(reads::DB_TABLE_PROVIDES, &RuleConfig::default()).is_none(),
            "db-table provides has no measured mover, so it must stay silent"
        );
        let movers: Vec<String> = crate::channel_direction::OBSERVED
            .iter()
            .filter(|r| {
                r.channel == reads::HTTP_PROVIDES
                    && matches!(
                        r.direction,
                        ChannelDirection::Silences | ChannelDirection::Floods
                    )
            })
            .map(|r| r.rule_id.to_string())
            .collect();
        let gate = RuleConfig {
            disabled_rules: movers,
            ..RuleConfig::default()
        };
        assert!(
            channel_consequence_warning(reads::HTTP_PROVIDES, &gate).is_none(),
            "every mover disabled must silence the disclosure, not print an empty roster"
        );
    }
}
