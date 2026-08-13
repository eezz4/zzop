//! The wildcard-route partition's disclosure half — see [`disclose`].
//!
//! `zzop_core::link_cross_layer_io` lifts an ANT-pattern route (`GET /api/files/**`) out of the exact
//! join (`zzop_core::io::wildcard`'s module doc has the three false findings that motivated it). That
//! partition is correct AND silent: the route stops being an edge candidate, stops being reportable as a
//! dead route, and swallows the consumes it serves — and not one bucket count says any of it happened. A
//! reader diffing `unprovidedConsumes` across the change would see a number drop with no cause on the
//! wire.
//!
//! So the linker's `wildcard_route_partitions` substrate is turned into a self-report on the DECLARING
//! tree's own `AnalyzeOutput::warnings` — the same per-tree channel `join_io_filter`'s test-io drop and
//! the topology-host zero-effect tripwire already use, chosen over a new channel for exactly that reason.
//! That channel is also the one with a proven carrier all the way out: `zzop cross` copies it verbatim
//! into `sources[].warnings` (`zzop_summary::cross`), which is where the pin at the outermost layer sits.

use zzop_core::WildcardRoutePartition;

use crate::AnalyzeOutput;

/// Pushes ONE warning per declaring tree (never one per route — a static-resource controller with a
/// dozen catch-alls must not bury the rest of that tree's warnings channel), naming every partitioned
/// route with the number of consume call sites it swallowed. A partition list that names no tree present
/// in `outputs` is dropped rather than misfiled: the warning's whole claim is "THIS tree's route did
/// this".
pub(super) fn disclose(
    outputs: &mut [(std::path::PathBuf, String, AnalyzeOutput)],
    partitions: &[WildcardRoutePartition],
) {
    for (source, warning) in messages(partitions) {
        if let Some((_, _, output)) = outputs.iter_mut().find(|(_, s, _)| **s == source) {
            output.warnings.push(warning);
        }
    }
}

/// `(source, warning)` per DECLARING tree, in the linker's own `(source, key, file, line)` order — the
/// pure half, so the wording is testable without an `AnalyzeOutput`. Two declarations of the SAME key in
/// one tree (an overload, or the same catch-all in two controllers) stay two entries: they are two sites,
/// and collapsing them would under-report the surface.
fn messages(partitions: &[WildcardRoutePartition]) -> Vec<(String, String)> {
    let mut grouped: Vec<(String, Vec<String>)> = Vec::new();
    for p in partitions {
        let rendered = format!(
            "`{}` (covered {} consume call site(s))",
            p.key, p.covered_consumes
        );
        match grouped.iter_mut().find(|(s, _)| *s == p.source) {
            Some((_, routes)) => routes.push(rendered),
            None => grouped.push((p.source.clone(), vec![rendered])),
        }
    }
    grouped
        .into_iter()
        .map(|(source, routes)| {
            let warning = format!(
                "{} wildcard route(s) partitioned OUT of the cross-layer join: {}. Their paths are ANT \
                 patterns, not exact keys, and the join is an exact `(kind, key)` match that never \
                 prefix-guesses — so each is excluded from `unconsumedProvides` (a pattern nobody \
                 spells back is not a dead route) and the calls it serves are taken out of \
                 `unprovidedConsumes` (the count beside each key). Those calls ARE served, so no \
                 cross-layer edge is emitted for them and no verb-level check runs against the pattern; \
                 declare the concrete routes (this tree's `routes` field) if you want exact-key \
                 coverage on them.",
                routes.len(),
                routes.join(", "),
            );
            (source, warning)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn partition(source: &str, key: &str, covered: usize) -> WildcardRoutePartition {
        WildcardRoutePartition {
            source: source.to_string(),
            key: key.to_string(),
            file: "Ctl.java".to_string(),
            line: 7,
            covered_consumes: covered,
        }
    }

    #[test]
    fn one_warning_per_tree_names_every_route_and_its_covered_count() {
        let out = messages(&[
            partition("be", "GET /api/files/**", 2),
            partition("be", "GET /api/static/*", 0),
        ]);
        assert_eq!(out.len(), 1, "one warning per tree, not per route: {out:?}");
        let (source, w) = &out[0];
        assert_eq!(source, "be");
        assert!(w.starts_with("2 wildcard route(s) partitioned OUT"), "{w}");
        assert!(
            w.contains("`GET /api/files/**` (covered 2 consume call site(s))"),
            "{w}"
        );
        // A zero-effect partition is still named — "declared but swallowed nothing" is a distinct fact.
        assert!(
            w.contains("`GET /api/static/*` (covered 0 consume call site(s))"),
            "{w}"
        );
    }

    #[test]
    fn two_declaring_trees_get_one_warning_each_and_a_silent_tree_gets_none() {
        let out = messages(&[
            partition("be-a", "GET /a/**", 1),
            partition("be-b", "GET /b/**", 0),
        ]);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].0, "be-a");
        assert!(out[0].1.starts_with("1 wildcard route(s)"), "{}", out[0].1);
        assert_eq!(out[1].0, "be-b");
        assert!(out[1].1.contains("`GET /b/**`"), "{}", out[1].1);
        assert!(messages(&[]).is_empty(), "no partitions, no warning");
    }
}
