//! Native-rule io-channel declarations — the vocabulary-free MECHANISM a rules crate uses to state,
//! ON THE SAME ROW as the analysis id it registers, which cross-layer io channels that rule's body
//! actually reads.
//!
//! # Why this exists
//! Every `framework_silence` tripwire can tell a user that route extraction came up empty and that
//! cross-layer joins will therefore be near-silent. None of them can name WHICH rules go quiet,
//! because until this module no rule→channel binding existed anywhere in production code: the
//! registry carries ids only, `RuleSightline` maps a rule to file EXTENSIONS (where its evidence can
//! be witnessed at all, not which io channel it reads), and the real fact lived as scattered
//! `kind == "http"` / `"db-table"` / `"trpc"` comparisons inside individual rule modules —
//! `rule_contracts::io_kind_readers` greps exactly those literals and deliberately throws the rule
//! attribution away.
//!
//! # The same deal as [`crate::sightline`] and [`crate::recognizer`]
//! Mechanism only, zero rule vocabulary: no rule id lives in this module. Each declaration's data
//! lives in the rules crate that owns the rule, on the very array its `register_native_analyses`
//! iterates — so a new rule cannot be registered without stating its channels (the row would not
//! compile), and the statement cannot drift from the id because there is only one row.
//! `zzop_engine::native_rule_channels` composes every crate's table, the same aggregator shape
//! `rule_sightlines` and `framework_recognizers` already use.
//!
//! # What a declaration claims, and what it does NOT
//! It claims the rule's inputs are drawn from that channel — nothing about the DIRECTION of the
//! consequence when the channel is empty. Both directions ship today and they are opposites:
//! `cross-layer/unconsumed-endpoint` goes SILENT with no http provides (it iterates them), while
//! `unprovided-consume` FLOODS (every consume looks unprovided). A consumer writing "channel X is
//! empty, so rule Y prints nothing" must establish that direction itself; this declaration only
//! narrows the candidate set to the rules the channel reaches at all. The direction is deliberately
//! not a field here because — unlike the channel set — no code evidence derives it, and an
//! unbindable field is hardcoding with a nicer address (see `2.backlog` if that changes).

use crate::recognizer::channel;

/// One (side, kind) io channel a rule reads.
///
/// Two axes rather than one string, because a side is not a channel: `db-table` io is built from
/// BOTH sides and `http` from both, so grouping by side alone puts a db rule in the same bucket as a
/// route rule. Both spellings are quoted from vocabulary that already exists —
/// [`channel::PROVIDES`]/[`channel::CONSUMES`] for the side and [`crate::RULE_READ_IO_KINDS`] for the
/// kind — never restated as literals here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct RuleIoChannel {
    /// [`channel::PROVIDES`] or [`channel::CONSUMES`].
    pub side: &'static str,
    /// One of [`crate::RULE_READ_IO_KINDS`].
    pub kind: &'static str,
}

impl RuleIoChannel {
    pub const fn provides(kind: &'static str) -> Self {
        Self {
            side: channel::PROVIDES,
            kind,
        }
    }

    pub const fn consumes(kind: &'static str) -> Self {
        Self {
            side: channel::CONSUMES,
            kind,
        }
    }

    /// The `"{side}:{kind}"` spelling — the SAME composite [`channel::DB`] already fixes for the db
    /// provide side, recomputed rather than re-spelled so the two cannot diverge (pinned by
    /// [`tests::the_db_channel_constant_is_exactly_the_composite_this_type_computes`]).
    pub fn label(&self) -> String {
        format!("{}:{}", self.side, self.kind)
    }
}

/// The closed cross product of the two sides and the three read kinds, named once so a declaration
/// cannot invent a channel by typo — the [`channel`] module's job, one layer up where rules live.
pub mod reads {
    use super::RuleIoChannel;

    pub const HTTP_PROVIDES: RuleIoChannel = RuleIoChannel::provides("http");
    pub const HTTP_CONSUMES: RuleIoChannel = RuleIoChannel::consumes("http");
    pub const DB_TABLE_PROVIDES: RuleIoChannel = RuleIoChannel::provides("db-table");
    pub const DB_TABLE_CONSUMES: RuleIoChannel = RuleIoChannel::consumes("db-table");
    pub const TRPC_PROVIDES: RuleIoChannel = RuleIoChannel::provides("trpc");
    pub const TRPC_CONSUMES: RuleIoChannel = RuleIoChannel::consumes("trpc");

    /// Every named channel, so a consumer can enumerate the vocabulary without owning a second copy
    /// of it (and so a contract can assert the set is closed).
    pub const ALL: &[RuleIoChannel] = &[
        HTTP_PROVIDES,
        HTTP_CONSUMES,
        DB_TABLE_PROVIDES,
        DB_TABLE_CONSUMES,
        TRPC_PROVIDES,
        TRPC_CONSUMES,
    ];

    /// The declaration a rule whose evidence is not io at all makes — the dep graph, a Prisma model,
    /// a symbol call graph. An EMPTY set is a real answer here (unlike
    /// `FrameworkRecognizer::emits`, where empty means undeclared), and it is checked as one: the
    /// rule's module must name no io kind either.
    pub const NO_IO: &[RuleIoChannel] = &[];
}

/// One registered native analysis and the io channels it reads.
///
/// `rule_id` is owned rather than `&'static str` because one family of ids is COMPOSED
/// (`zzop_rules_schema` registers `schema/<label>` per issue label, from the same label lists its
/// message module reads) — forcing a static string there would mean spelling those ids a second
/// time, which is exactly the drift this type exists to remove.
#[derive(Debug, Clone)]
pub struct NativeRuleChannels {
    pub rule_id: String,
    pub reads: &'static [RuleIoChannel],
}

/// One crate's `(id, channels)` table, turned into declarations. Every rules crate's
/// `register_native_analyses` and its `native_rule_channels` read the SAME table through this — one
/// array, two readers, so an id cannot be registered without a channel statement or carry one
/// without being registered.
pub fn declare_native_rule_channels(
    table: &'static [(&'static str, &'static [RuleIoChannel])],
) -> Vec<NativeRuleChannels> {
    table
        .iter()
        .map(|(rule_id, reads)| NativeRuleChannels {
            rule_id: (*rule_id).to_string(),
            reads,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RULE_READ_IO_KINDS;

    /// The composite spelling is REUSED, not reinvented. `channel::DB` is the one place this repo had
    /// already written a `{side}:{kind}` channel name down; if [`RuleIoChannel::label`] ever stops
    /// reproducing it, two vocabularies exist for one fact and every consumer joining rules to
    /// recognizers silently splits into two buckets.
    #[test]
    fn the_db_channel_constant_is_exactly_the_composite_this_type_computes() {
        assert_eq!(reads::DB_TABLE_PROVIDES.label(), channel::DB);
    }

    /// The named channels must span the kind vocabulary exactly — a kind no named channel covers is a
    /// channel no rule can declare (silently unsayable), and a named channel outside the vocabulary
    /// is one no io fact can ever fill.
    #[test]
    fn the_named_channels_are_the_side_by_kind_cross_product() {
        let mut expected = Vec::new();
        for kind in RULE_READ_IO_KINDS {
            expected.push(RuleIoChannel::provides(kind));
            expected.push(RuleIoChannel::consumes(kind));
        }
        expected.sort();
        let mut named = reads::ALL.to_vec();
        named.sort();
        assert_eq!(
            named, expected,
            "reads::ALL and RULE_READ_IO_KINDS disagree — a kind gained or lost a reader without \
             this vocabulary following it"
        );
        for c in reads::ALL {
            assert!(
                c.side == channel::PROVIDES || c.side == channel::CONSUMES,
                "{c:?} names a side that is not one of the recognizer channel constants"
            );
        }
    }
}
