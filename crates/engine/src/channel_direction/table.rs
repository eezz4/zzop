//! The measured rows themselves — see [`super`] for what they mean and why they are written down.
//!
//! DO NOT hand-edit a verdict here. Every row is the output of `rule_contracts::channel_direction`'s
//! two-arm probe, which re-derives the whole table on every `cargo test` and prints the corrected
//! block when it disagrees. Editing a value to make the contract green inverts the direction of the
//! binding: the table would go back to being a claim, which is the state this module exists to leave.
//!
//! Sorted by `(rule id, channel label)` because that is the order the probe emits — pasting its output
//! verbatim is the intended edit, and a hand-chosen order would make the diff between two runs
//! unreadable. Split from `super` purely for the file-length ratchet; the row count is
//! `zzop_core::rule_channels`' business, not a number written here.

use zzop_core::rule_channels::reads::{
    DB_TABLE_CONSUMES, DB_TABLE_PROVIDES, HTTP_CONSUMES, HTTP_PROVIDES, TRPC_CONSUMES,
    TRPC_PROVIDES,
};
use zzop_core::RuleIoChannel;

use super::ChannelDirection::{Floods, Silences, Unobserved};
use super::{ChannelDirection, RuleChannelDirection};

/// Compact row constructor — one line per declared pair, so the struct literal lives here once.
const fn row(
    rule_id: &'static str,
    channel: RuleIoChannel,
    direction: ChannelDirection,
) -> RuleChannelDirection {
    RuleChannelDirection {
        rule_id,
        channel,
        direction,
    }
}

pub const OBSERVED: &[RuleChannelDirection] = &[
    row("cross-layer/all-consumes-unjoined", HTTP_CONSUMES, Silences),
    row("cross-layer/all-consumes-unjoined", HTTP_PROVIDES, Silences),
    row("cross-layer/ambiguous-consume", HTTP_CONSUMES, Unobserved),
    row("cross-layer/ambiguous-consume", TRPC_CONSUMES, Unobserved),
    row("cross-layer/ambiguous-consume", HTTP_PROVIDES, Unobserved),
    row("cross-layer/ambiguous-consume", TRPC_PROVIDES, Unobserved),
    row("cross-layer/body-field-drift", HTTP_CONSUMES, Unobserved),
    row("cross-layer/body-field-drift", HTTP_PROVIDES, Unobserved),
    row(
        "cross-layer/db-table-name-in-multiple-sources",
        DB_TABLE_CONSUMES,
        Silences,
    ),
    row(
        "cross-layer/db-table-name-in-multiple-sources",
        DB_TABLE_PROVIDES,
        Unobserved,
    ),
    row("cross-layer/duplicate-route", HTTP_CONSUMES, Unobserved),
    row("cross-layer/duplicate-route", HTTP_PROVIDES, Silences),
    row(
        "cross-layer/external-base-url-drift",
        HTTP_CONSUMES,
        Silences,
    ),
    row(
        "cross-layer/external-host-fanout",
        HTTP_CONSUMES,
        Unobserved,
    ),
    row(
        "cross-layer/external-host-in-multiple-sources",
        HTTP_CONSUMES,
        Silences,
    ),
    row("cross-layer/external-ip-literal", HTTP_CONSUMES, Silences),
    row(
        "cross-layer/external-secret-in-url",
        HTTP_CONSUMES,
        Silences,
    ),
    row(
        "cross-layer/external-shadow-internal",
        HTTP_CONSUMES,
        Silences,
    ),
    row(
        "cross-layer/external-shadow-internal",
        HTTP_PROVIDES,
        Silences,
    ),
    row(
        "cross-layer/external-version-inconsistent",
        HTTP_CONSUMES,
        Silences,
    ),
    row("cross-layer/method-mismatch", HTTP_CONSUMES, Silences),
    row("cross-layer/method-mismatch", HTTP_PROVIDES, Silences),
    row("cross-layer/path-near-miss", HTTP_CONSUMES, Silences),
    row("cross-layer/path-near-miss", HTTP_PROVIDES, Silences),
    row("cross-layer/prefix-drift", HTTP_CONSUMES, Unobserved),
    row("cross-layer/prefix-drift", HTTP_PROVIDES, Unobserved),
    row(
        "cross-layer/retrying-write-no-idempotency",
        HTTP_CONSUMES,
        Unobserved,
    ),
    row(
        "cross-layer/retrying-write-no-idempotency",
        HTTP_PROVIDES,
        Unobserved,
    ),
    row("cross-layer/route-near-miss", HTTP_CONSUMES, Unobserved),
    row("cross-layer/route-near-miss", HTTP_PROVIDES, Unobserved),
    row("cross-layer/route-shadowing", HTTP_PROVIDES, Silences),
    row(
        "cross-layer/sensitive-response-field",
        HTTP_CONSUMES,
        Unobserved,
    ),
    row(
        "cross-layer/sensitive-response-field",
        HTTP_PROVIDES,
        Silences,
    ),
    row("cross-layer/unconsumed-endpoint", HTTP_CONSUMES, Floods),
    row("cross-layer/unconsumed-endpoint", HTTP_PROVIDES, Silences),
    row(
        "cross-layer/unconsumed-mutation-endpoint",
        HTTP_CONSUMES,
        Unobserved,
    ),
    row(
        "cross-layer/unconsumed-mutation-endpoint",
        HTTP_PROVIDES,
        Silences,
    ),
    row("cross-layer/unconsumed-procedure", TRPC_PROVIDES, Silences),
    row("cross-layer/unknown-verb-route", HTTP_PROVIDES, Unobserved),
    row(
        "cross-layer/unprovided-mutation-call",
        HTTP_CONSUMES,
        Unobserved,
    ),
    row(
        "cross-layer/unprovided-mutation-call",
        HTTP_PROVIDES,
        Floods,
    ),
    row(
        "cross-layer/unresolved-consume-ratio",
        HTTP_CONSUMES,
        Unobserved,
    ),
    row(
        "cross-layer/untraced-client-import-no-visible-consume",
        HTTP_CONSUMES,
        Unobserved,
    ),
    row("cross-layer/version-skew", HTTP_CONSUMES, Silences),
    row("cross-layer/version-skew", HTTP_PROVIDES, Silences),
    row("duplicate-route", HTTP_PROVIDES, Unobserved),
    row("mutating-route-no-auth", HTTP_PROVIDES, Silences),
    row("non-idempotent-write", HTTP_PROVIDES, Silences),
    row("route-shadowing", HTTP_PROVIDES, Unobserved),
    row("unprovided-consume", HTTP_CONSUMES, Silences),
    row("unprovided-consume", HTTP_PROVIDES, Silences),
    row("unsafe-read-endpoint", HTTP_PROVIDES, Silences),
];
