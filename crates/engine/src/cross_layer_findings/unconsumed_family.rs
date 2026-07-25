//! Wiring for the `cross-layer/unconsumed-endpoint` + `cross-layer/unconsumed-mutation-endpoint` pair —
//! split out of the flat rule dispatch (like `blindness_caveat`) because this pair is the one place there
//! where two rules are joined by a DATA dependency rather than merely listed side by side: the general rule
//! needs the specialized rule's output before it can run. Keeping that ordering constraint in its own module
//! stops it from looking like a reorderable line in the dispatch list.
//!
//! A write route IS an endpoint, so `unconsumed-mutation-endpoint` is a strict specialization of
//! `unconsumed-endpoint` — and both used to fire at the identical `file:line` (dogfood measured
//! `POST /api/ledger/{}/verify` billed as two problems). [`compute`] runs the specialization first and lets
//! the general rule stand down at exactly the sites it REPORTED (`reported_provide_sites`). Two properties
//! that must survive any edit here:
//! 1. Keyed on produced findings — never on a second copy of the write-verb predicate, which would drift
//!    from the specialization's own exclusions, and never on the enable flag alone, which says nothing about
//!    which sites actually fired.
//! 2. Gate-dependent by construction: with the specialization disabled nothing is reported, so nothing is
//!    suppressed and the general rule covers write routes itself. Disabling one rule must never punch a
//!    silent hole in a rule the user did not disable.

use std::collections::{BTreeMap, BTreeSet};

use zzop_core::io::{TaggedConsume, TaggedProvide};
use zzop_core::{Finding, RuleConfig};
use zzop_rules_cross_layer::cross_layer::route_near_miss::NearMissTargetRef;
use zzop_rules_cross_layer::cross_layer::unconsumed_mutation_endpoint::reported_provide_sites;

use super::blindness_caveat;

/// `(unconsumed-endpoint findings, unconsumed-mutation-endpoint findings)`, each empty when that rule is
/// gated off. Returned rather than pushed so the caller keeps both at their established `sources` positions.
#[allow(clippy::too_many_arguments)]
pub(super) fn compute(
    gate: &RuleConfig,
    unconsumed_provides: &[TaggedProvide],
    unresolved_consumes: &[TaggedConsume],
    http_consume_totals: &[(String, usize)],
    near_miss_targets: &BTreeMap<(String, String, u32), NearMissTargetRef>,
    trpc_participating_sources: &BTreeSet<String>,
    caveat: &Option<String>,
) -> (Vec<Finding>, Vec<Finding>) {
    let mutation = if zzop_core::is_enabled(gate, "cross-layer/unconsumed-mutation-endpoint") {
        // Same blindness predicate `cross-layer/unresolved-consume-ratio` self-reports with, via the shared
        // helper so the two rules never drift on what counts BLIND (a confident "unconsumed" verdict needs
        // a resolved consume side).
        let blind_sources = zzop_rules_cross_layer::majority_unresolved_http_sources(
            unresolved_consumes,
            http_consume_totals,
        );
        let mut findings = zzop_rules_cross_layer::unconsumed_mutation_endpoint_findings(
            unconsumed_provides,
            unresolved_consumes,
            &blind_sources,
            near_miss_targets,
            trpc_participating_sources,
        );
        blindness_caveat::append(&mut findings, caveat);
        findings
    } else {
        Vec::new()
    };

    let general = if zzop_core::is_enabled(gate, "cross-layer/unconsumed-endpoint") {
        let mut findings = zzop_rules_cross_layer::unconsumed_endpoint_findings(
            unconsumed_provides,
            unresolved_consumes,
            near_miss_targets,
            trpc_participating_sources,
            &reported_provide_sites(&mutation),
        );
        blindness_caveat::append(&mut findings, caveat);
        findings
    } else {
        Vec::new()
    };

    (general, mutation)
}
