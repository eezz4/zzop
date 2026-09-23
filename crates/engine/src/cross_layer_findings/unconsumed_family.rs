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
    externally_fetched_paths: &[&str],
    // The two halves of the zero-contribution blindness gate, ANDed below. Kept as two inputs rather
    // than one precomputed verdict so the reason a source counts as blind stays readable at the seam.
    zero_contribution_sources: &[&str],
    untraced_blind_sources: &BTreeSet<String>,
    mostly_unread_by_source: &BTreeSet<String>,
) -> (Vec<Finding>, Vec<Finding>) {
    let mutation = if zzop_core::is_enabled(gate, "cross-layer/unconsumed-mutation-endpoint") {
        // Same blindness predicate `cross-layer/unresolved-consume-ratio` self-reports with, via the shared
        // helper so the two rules never drift on what counts BLIND (a confident "unconsumed" verdict needs
        // a resolved consume side).
        let blind_sources = zzop_rules_cross_layer::majority_unresolved_http_sources(
            unresolved_consumes,
            http_consume_totals,
        );
        // 🔴 The ratio predicate above CANNOT see the most blind tree of all (review ledger V63).
        // `majority_unresolved_http_sources` is unresolved/total over a `MIN_TOTAL_CONSUMES` floor, so a
        // tree that contributed NOTHING has total 0, falls below the floor, and reads as NOT blind — the
        // band then stayed at `warning` on exactly the run where the caller side was darkest. Measured:
        // `corpus/oss/fe-svelte` + `be-gin`, 16 write endpoints at `warning` while the caller tree's
        // `.svelte` files were never parsed.
        //
        // Zero contribution alone does not qualify — a shared-lib or UI-only tree in a monorepo join is
        // legitimately io-less, and downgrading a real attack-surface warning because of an unrelated
        // tree would be the worse trade. It is ANDed with a POSITIVE measurement that the tree's own
        // files went unread, the same shape `provide_blind_sources` uses on the other side: a zero
        // counts as blindness only when something measured says io was owed.
        //
        // Kept SEPARATE from the ratio set rather than unioned into it, because the finding's own
        // confidence sentence names the mechanism — and "majority-unresolved consumes" is FALSE about a
        // tree that has no consumes at all. One set would have made the band right and the sentence wrong.
        let silent_blind_sources: std::collections::BTreeSet<String> = zero_contribution_sources
            .iter()
            .filter(|s| mostly_unread_by_source.contains(**s))
            .map(|s| (*s).to_string())
            .collect();
        let mut findings = zzop_rules_cross_layer::unconsumed_mutation_endpoint_findings(
            unconsumed_provides,
            unresolved_consumes,
            &blind_sources,
            &silent_blind_sources,
            untraced_blind_sources,
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
            externally_fetched_paths,
        );
        blindness_caveat::append(&mut findings, caveat);
        findings
    } else {
        Vec::new()
    };

    (general, mutation)
}
