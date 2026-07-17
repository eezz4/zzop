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
    let signal_count = cross_layer_findings
        .iter()
        .filter(|f| {
            f.rule_id == "cross-layer/duplicate-route"
                || f.rule_id == "cross-layer/ambiguous-consume"
        })
        .count();
    if signal_count < MIN_PARALLEL_IMPL_SIGNALS {
        return None;
    }
    Some(format!(
        "this join produced 0 cross-source edges but {signal_count} duplicate-route/ambiguous-consume \
         findings — the trees may be parallel implementations of the same API surface rather than one \
         system; if so, analyze them separately (or trim the config's trees)."
    ))
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
