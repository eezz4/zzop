//! The parallel-implementation tripwire: a run-level self-report when a multi-tree join produced
//! zero cross-source edges alongside a pile of duplicate-route/ambiguous-consume findings — the
//! shape `trees: "auto"` (or any multi-tree config) hits when it wires several competing
//! reimplementations of the SAME API surface into one join instead of one real system split across
//! layers. Blind field test that motivated this: `trees: "auto"` wired 5 competing frontend
//! reimplementations + 2 backends of the same API into one join, producing 0 clean cross-source
//! edges and 86 pure ambiguity/duplicate findings — presented with no run-level context, that reads
//! as "one system with 86 drift problems" rather than "5+2 systems that all happen to expose the
//! same route shapes".

use zzop_core::{CrossLayerResult, Finding};

/// Threshold for [`maybe_warn`]: how many combined `cross-layer/duplicate-route` +
/// `cross-layer/ambiguous-consume` findings, alongside ZERO cross-source edges, count as signal (not
/// an ordinary small-scale overlap one real multi-service system can produce on its own — e.g. two
/// services both happening to expose a `/health` route) that the joined trees are parallel
/// reimplementations rather than one system. Picked well below the field test's observed 86 (any run
/// that far past "a few incidental route-shape collisions" is unambiguous) while still high enough
/// that a handful of coincidental overlaps never trips it. Census-tracked
/// (`scripts/check-policy-census.sh`) — triage this as a T-tier disclosure-gate threshold (a run-level
/// honesty gate), not a finding-severity policy value, in the project's internal policy-value review.
pub const MIN_PARALLEL_IMPL_SIGNALS: usize = 5;

/// Returns the ONE run-level warning when the gate above fires, else `None`. Never per-tree — no
/// single tree in the join is "at fault" for this shape, so it does not belong on any one tree's own
/// `AnalyzeOutput::warnings` (unlike the topology-host/tRPC-suppression tripwires in `mod.rs`, which
/// each blame one config-declaring tree).
///
/// Cross-source edges are the join actually connecting two DIFFERENT trees cleanly (consume in one,
/// provide in another, same normalized key). Duplicate-route/ambiguous-consume firing in their
/// TOTAL absence, past the threshold, means every candidate cross-tree match this run found
/// collapsed into "which of N near-identical providers is the real one" instead of a single clean
/// edge anywhere — the signature of parallel implementations of one API surface, not ordinary drift
/// inside one system (which would still produce SOME clean edges alongside the noise).
pub fn maybe_warn(
    cross_layer: &CrossLayerResult,
    cross_layer_findings: &[Finding],
) -> Option<String> {
    let cross_source_edges = cross_layer.edges.iter().filter(|e| e.cross_source).count();
    if cross_source_edges > 0 {
        return None;
    }
    let signal_count = count_signals(cross_layer_findings);
    if signal_count < MIN_PARALLEL_IMPL_SIGNALS {
        return None;
    }
    Some(format!(
        "this join produced 0 cross-source edges but {signal_count} duplicate-route/ambiguous-consume \
         findings — the trees may be parallel implementations of the same API surface rather than one \
         system; if so, analyze them separately (or trim the config's trees)."
    ))
}

/// How many DISTINCT overlap signals this run carries — the quantity [`MIN_PARALLEL_IMPL_SIGNALS`] is
/// calibrated against.
///
/// `cross-layer/duplicate-route` is deduped by its route key, because since 2026-07-29 that rule emits
/// ONE COPY PER PROVIDING SOURCE, each anchored in its own tree. Counting copies would make the threshold
/// mean something different for every run: two trees sharing 3 routes would produce 6 "signals" and five
/// trees sharing 1 route would produce 5, so a fixed threshold would stop measuring "how much of the API
/// surface overlaps" and start measuring "how many trees are in the config". One colliding route is one
/// signal regardless of how many sources serve it. `ambiguous-consume` is already one per consume site,
/// which is the per-call-site quantity the threshold wants.
fn count_signals(cross_layer_findings: &[Finding]) -> usize {
    let mut duplicate_keys: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    let mut per_finding = 0usize;
    for f in cross_layer_findings {
        match f.rule_id.as_str() {
            "cross-layer/duplicate-route" => {
                match f
                    .data
                    .as_ref()
                    .and_then(|d| d.get("key"))
                    .and_then(serde_json::Value::as_str)
                {
                    Some(key) => {
                        duplicate_keys.insert(key);
                    }
                    // Defensive: a payload with no `key` cannot be deduped, so it counts on its own
                    // rather than being dropped — an unrecognized shape must not shrink the signal.
                    None => per_finding += 1,
                }
            }
            "cross-layer/ambiguous-consume" => per_finding += 1,
            _ => {}
        }
    }
    duplicate_keys.len() + per_finding
}

#[cfg(test)]
mod tests {
    use super::*;
    use zzop_core::Severity;

    fn finding(rule_id: &str) -> Finding {
        Finding {
            rule_id: rule_id.to_string(),
            severity: Severity::Warning,
            file: "a.ts".to_string(),
            line: 1,
            message: String::new(),
            evidence_paths: Vec::new(),
            data: None,
        }
    }

    #[test]
    fn fires_when_edges_are_zero_and_signals_meet_the_threshold() {
        let cl = CrossLayerResult::default();
        let findings: Vec<Finding> = (0..MIN_PARALLEL_IMPL_SIGNALS)
            .map(|_| finding("cross-layer/duplicate-route"))
            .collect();
        let warning = maybe_warn(&cl, &findings).expect("expected the tripwire to fire");
        assert!(warning.contains("0 cross-source edges"));
        assert!(warning.contains(&MIN_PARALLEL_IMPL_SIGNALS.to_string()));
        assert!(warning.contains("parallel implementations"));
    }

    fn duplicate_route(key: &str) -> Finding {
        Finding {
            data: Some(serde_json::json!({ "key": key })),
            ..finding("cross-layer/duplicate-route")
        }
    }

    /// The threshold measures how much of the API SURFACE overlaps. `duplicate-route` emits one copy per
    /// providing source, so counting copies would instead measure how many trees are in the config —
    /// three trees serving one shared route would trip a threshold two trees serving three could not.
    #[test]
    fn duplicate_route_copies_of_one_route_are_one_signal() {
        let cl = CrossLayerResult::default();
        let copies: Vec<Finding> = (0..MIN_PARALLEL_IMPL_SIGNALS + 3)
            .map(|_| duplicate_route("DELETE /api/legacy/purge"))
            .collect();
        assert!(maybe_warn(&cl, &copies).is_none());

        // ...and distinct routes still each count, so the gate is untouched for the shape it was
        // calibrated on: one signal per colliding route, at the same threshold.
        let distinct: Vec<Finding> = (0..MIN_PARALLEL_IMPL_SIGNALS)
            .map(|i| duplicate_route(&format!("GET /api/r{i}")))
            .collect();
        assert!(maybe_warn(&cl, &distinct).is_some());
    }

    #[test]
    fn silent_below_the_threshold() {
        let cl = CrossLayerResult::default();
        let findings: Vec<Finding> = (0..MIN_PARALLEL_IMPL_SIGNALS - 1)
            .map(|_| finding("cross-layer/ambiguous-consume"))
            .collect();
        assert!(maybe_warn(&cl, &findings).is_none());
    }

    #[test]
    fn silent_when_any_cross_source_edge_exists_even_with_enough_signals() {
        use zzop_core::io::{CrossLayerEdge, EdgeFrom, EdgeTo};
        let cl = CrossLayerResult {
            edges: vec![CrossLayerEdge {
                kind: "http".to_string(),
                key: "GET /x".to_string(),
                from: EdgeFrom {
                    source: "fe".to_string(),
                    file: "a.ts".to_string(),
                    line: 1,
                },
                to: EdgeTo {
                    source: "be".to_string(),
                    file: "b.ts".to_string(),
                    line: 1,
                    symbol: None,
                },
                cross_source: true,
                low_confidence_reason: None,
            }],
            ..Default::default()
        };
        let findings: Vec<Finding> = (0..MIN_PARALLEL_IMPL_SIGNALS)
            .map(|_| finding("cross-layer/duplicate-route"))
            .collect();
        assert!(maybe_warn(&cl, &findings).is_none());
    }

    #[test]
    fn silent_with_zero_edges_but_unrelated_findings() {
        let cl = CrossLayerResult::default();
        let findings: Vec<Finding> = (0..10)
            .map(|_| finding("cross-layer/route-shadowing"))
            .collect();
        assert!(maybe_warn(&cl, &findings).is_none());
    }
}
