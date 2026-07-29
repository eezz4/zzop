//! The ONE definition of "the cross-layer join is blind to this tree", shared by every rule that has to
//! reason about it. Split out of `cross_layer/mod.rs` on 2026-07-29 when a third consumer
//! (`all_consumes_unjoined`) arrived and pushed that file over the line-count ratchet — the split is along
//! the seam these three items already formed, not an arbitrary cut.
//!
//! Three rule families read it, and the whole point is that they read the SAME predicate:
//! - `unresolved_consume_ratio` DISCLOSES the blindness per source.
//! - the `unconsumed-*` family must not claim a confident "unconsumed" verdict when a blind tree could be
//!   the unseen caller.
//! - `all_consumes_unjoined` stands down entirely on a blind tree, so the blind-spot self-reports keep
//!   partitioning instead of co-firing.
//!
//! Integer math only throughout — no float reaches output, so a count is byte-stable across platforms.

/// Trees below this many total `http` consumes are too small for a ratio claim — shared floor between
/// `unresolved_consume_ratio` (fires at/above it) and `sdk_import_no_visible_consume` (fires below it),
/// so the two blind-spot self-reports partition the space and never co-fire on one tree. Also the floor
/// [`majority_unresolved_http_sources`] uses to decide which sources are eligible to count as BLIND at all.
pub(crate) const MIN_TOTAL_CONSUMES: usize = 5;

/// Majority threshold, integer math only (no floats — output must be byte-stable across platforms):
/// `unresolved * 2 >= total` is equivalent to `unresolved / total >= 0.5` without any floating-point
/// division. Single definition, shared by [`majority_unresolved_http_sources`] and `unresolved_consume_ratio`
/// so the two can never drift apart on what "majority" means.
pub(crate) fn is_majority_unresolved(unresolved: usize, total: usize) -> bool {
    unresolved * 2 >= total
}

/// Sources whose `http` consumes are majority-unresolved (key extraction failed for most call sites) AND
/// above the small-sample floor ([`MIN_TOTAL_CONSUMES`]) — i.e. sources the cross-layer join is effectively
/// BLIND to. Single definition shared by `unresolved-consume-ratio` (which discloses the blindness per
/// source), the `unconsumed-*` rules (which must not over-claim a confident "unconsumed" verdict when a
/// blind source could be the unseen caller), and `all-consumes-unjoined` (which stands down on a blind
/// tree). Integer math only — no floats reach output.
///
/// This helper is the shared predicate that lets the disclosure rule and the confidence-gated rules reason
/// about blindness identically, so they can never silently drift apart on the definition again.
pub fn majority_unresolved_http_sources(
    unresolved_consumes: &[zzop_core::io::TaggedConsume],
    http_consume_totals: &[(String, usize)],
) -> std::collections::BTreeSet<String> {
    let mut unresolved_by_source: std::collections::BTreeMap<&str, usize> =
        std::collections::BTreeMap::new();
    for c in unresolved_consumes {
        if c.consume.kind == "http" {
            *unresolved_by_source.entry(c.source.as_str()).or_insert(0) += 1;
        }
    }

    http_consume_totals
        .iter()
        .filter_map(|(source, total)| {
            let total = *total;
            if total < MIN_TOTAL_CONSUMES {
                return None;
            }
            let unresolved_count = *unresolved_by_source.get(source.as_str())?;
            is_majority_unresolved(unresolved_count, total).then(|| source.clone())
        })
        .collect()
}
