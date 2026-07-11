//! `cross-layer/unconsumed-endpoint` (info) — one finding per `CrossLayerResult::unconsumed_provides` entry of
//! kind `"http"`: an endpoint no source in this `analyzeTrees` run calls. Severity starts at info (not
//! warning) because "no consumer WITHIN this analysis" is weaker evidence than "no consumer at all" — see
//! the message's own caveat.
//!
//! Provider sites in test-path files (`zzop_core::is_test_file`) are skipped — a route registered
//! in a test fixture is not deployed surface. A dead route provided by 2+ trees ALSO fires one warning
//! `cross-layer/duplicate-route` finding for the same key — intentional overlap, different questions.
//!
//! ## Near-miss cross-reference
//! When a provide here is ALSO the chosen near-miss target of an unmatched `cross-layer/route-near-miss`
//! consume (`near_miss_targets`, sourced from `route_near_miss::route_near_miss_results`), the message gains
//! a cross-reference note: dogfood round 8 found this to be the common case, not the exception — a
//! disconnected FE/BE pair with a drifted base prefix produces one `unconsumed-endpoint` finding PER route
//! plus one `route-near-miss` finding per drifted consume, describing the same underlying drift from two
//! sides without ever pointing at each other.
//!
//! ## tRPC mount-route suppression
//! A provide [`super::is_trpc_mount_route_key`] identifies as a tRPC mount route (a literal `trpc` path
//! segment, e.g. `/api/trpc/{}`) is excluded here when ITS OWN source tree is in `trpc_participating_sources`
//! (a tree with 1+ `trpc`-kind edge on either side): dogfood round 9 found a fully-joined tRPC starter's
//! only findings were its own GET/POST mount routes — the mount route IS the transport the `trpc`-kind
//! edges flow through, so "unconsumed" is tone noise, not signal. Per-tree, not run-global: a route in a
//! tree with zero tRPC edges of its own is never suppressed, even when some OTHER tree in the run has
//! tRPC edges. The suppression is never silent — `super::trpc_mount_route_suppression_notes` (called by
//! `zzop_engine::analyze_trees`) discloses it on the owning tree's `warnings` channel instead.

use std::collections::{BTreeMap, BTreeSet};

use zzop_core::io::{TaggedConsume, TaggedProvide};
use zzop_core::{disable_hint, Finding, Severity};

use super::route_near_miss::NearMissTargetRef;

pub fn unconsumed_endpoint_findings(
    unconsumed_provides: &[TaggedProvide],
    unresolved_consumes: &[TaggedConsume],
    near_miss_targets: &BTreeMap<(String, String, u32), NearMissTargetRef>,
    trpc_participating_sources: &BTreeSet<String>,
) -> Vec<Finding> {
    let unresolved_http = unresolved_consumes
        .iter()
        .filter(|c| c.consume.kind == "http")
        .count();

    let mut out: Vec<Finding> = unconsumed_provides
        .iter()
        .filter(|p| p.provide.kind == "http" && !zzop_core::is_test_file(&p.provide.file))
        .filter(|p| {
            !(trpc_participating_sources.contains(&p.source)
                && super::is_trpc_mount_route_key(&p.provide.key))
        })
        .map(|p| {
            let key = &p.provide.key;
            let near_miss = near_miss_targets.get(&(
                p.source.clone(),
                p.provide.file.clone(),
                p.provide.line,
            ));
            let near_miss_note = if let Some(t) = near_miss {
                format!(
                    " However, {} unmatched http consume(s) in this run name this route as their closest \
                     near-miss candidate (see the `cross-layer/route-near-miss` finding at {}:{}) — the route \
                     may actually be called through a drifted or base-relative path rather than being dead.",
                    t.count, t.consume_file, t.consume_line
                )
            } else {
                String::new()
            };
            let message = format!(
                "endpoint `{key}` (source `{}`) is not called by any source in this analysis. This may be \
                 genuinely dead route code, or it may be consumed by a caller this analysis cannot see — a \
                 repo not included in this `analyzeTrees` run, a mobile/native/third-party client, or one of \
                 the {unresolved_http} unresolved dynamic-URL http consume(s) this run could not statically \
                 match to a key (see `crossLayer.unresolvedConsumes`). Confirm with real traffic/access logs before \
                 removing the route.{near_miss_note} {} if provider-only endpoints (webhook targets, health probes, \
                 endpoints consumed only outside this analysis) are expected in your stack.",
                p.source,
                disable_hint("cross-layer/unconsumed-endpoint")
            );
            let mut data = serde_json::json!({
                "key": key,
                "source": p.source,
                "unresolvedHttpConsumeCount": unresolved_http,
            });
            if let Some(t) = near_miss {
                data["nearMissConsumeCount"] = serde_json::json!(t.count);
                data["nearMissConsumeExample"] =
                    serde_json::json!(format!("{}:{}", t.consume_file, t.consume_line));
            }
            Finding {
                rule_id: "cross-layer/unconsumed-endpoint".to_string(),
                severity: Severity::Info,
                file: p.provide.file.clone(),
                line: p.provide.line,
                message,
                data: Some(data),
            }
        })
        .collect();
    out.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dead(key: &str, source: &str, file: &str, line: u32) -> TaggedProvide {
        TaggedProvide {
            source: source.to_string(),
            provide: zzop_core::IoProvide {
                kind: "http".to_string(),
                key: key.to_string(),
                file: file.to_string(),
                line,
                symbol: None,
            },
        }
    }

    fn dead_kind(kind: &str, key: &str, source: &str, file: &str, line: u32) -> TaggedProvide {
        TaggedProvide {
            source: source.to_string(),
            provide: zzop_core::IoProvide {
                kind: kind.to_string(),
                key: key.to_string(),
                file: file.to_string(),
                line,
                symbol: None,
            },
        }
    }

    fn unresolved(kind: &str, source: &str) -> TaggedConsume {
        TaggedConsume {
            source: source.to_string(),
            consume: zzop_core::IoConsume {
                kind: kind.to_string(),
                key: None,
                file: "dyn.ts".to_string(),
                line: 1,
                raw: Some("dyn".to_string()),
                method: None,
            },
        }
    }

    fn no_near_miss() -> BTreeMap<(String, String, u32), NearMissTargetRef> {
        BTreeMap::new()
    }

    fn no_trpc() -> BTreeSet<String> {
        BTreeSet::new()
    }

    fn trpc_sources(sources: &[&str]) -> BTreeSet<String> {
        sources.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn dead_http_provide_is_flagged_with_source_and_anchor() {
        let out = unconsumed_endpoint_findings(
            &[dead("GET /orphan", "be", "Api.java", 12)],
            &[],
            &no_near_miss(),
            &no_trpc(),
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "cross-layer/unconsumed-endpoint");
        assert_eq!(out[0].severity, Severity::Info);
        assert_eq!(out[0].file, "Api.java");
        assert_eq!(out[0].line, 12);
        assert!(out[0].message.contains("GET /orphan"));
        assert!(out[0].message.contains("source `be`"));
        assert!(out[0].message.contains("disabled_rules"));
        assert!(!out[0].message.contains("near-miss"));
    }

    #[test]
    fn dead_provide_registered_in_a_test_fixture_file_is_skipped() {
        let out = unconsumed_endpoint_findings(
            &[dead(
                "GET /fixture",
                "be",
                "src/api/__test__/handlers.test.ts",
                125,
            )],
            &[],
            &no_near_miss(),
            &no_trpc(),
        );
        assert!(out.is_empty());
    }

    #[test]
    fn non_http_dead_provide_is_ignored() {
        let out = unconsumed_endpoint_findings(
            &[dead_kind("db-table", "table:users", "db", "schema.sql", 1)],
            &[],
            &no_near_miss(),
            &no_trpc(),
        );
        assert!(out.is_empty());
    }

    #[test]
    fn no_unconsumed_provides_is_empty() {
        assert!(unconsumed_endpoint_findings(&[], &[], &no_near_miss(), &no_trpc()).is_empty());
    }

    #[test]
    fn message_states_the_unresolved_http_count_honestly() {
        let out = unconsumed_endpoint_findings(
            &[dead("GET /orphan", "be", "Api.java", 12)],
            &[
                unresolved("http", "fe"),
                unresolved("http", "fe"),
                unresolved("queue", "fe"), // not http — must not inflate the count
            ],
            &no_near_miss(),
            &no_trpc(),
        );
        assert_eq!(out.len(), 1);
        assert!(out[0].message.contains("2 unresolved"));
    }

    #[test]
    fn multiple_unconsumed_provides_are_sorted_by_file_then_line() {
        let out = unconsumed_endpoint_findings(
            &[
                dead("GET /b", "be", "z.java", 1),
                dead("GET /a", "be", "a.java", 9),
                dead("GET /c", "be", "a.java", 2),
            ],
            &[],
            &no_near_miss(),
            &no_trpc(),
        );
        let sites: Vec<(&str, u32)> = out.iter().map(|f| (f.file.as_str(), f.line)).collect();
        assert_eq!(sites, vec![("a.java", 2), ("a.java", 9), ("z.java", 1)]);
    }

    #[test]
    fn near_miss_cross_reference_note_fires_when_the_provide_is_a_near_miss_target() {
        let mut targets = BTreeMap::new();
        targets.insert(
            ("be".to_string(), "Api.java".to_string(), 12),
            NearMissTargetRef {
                consume_file: "Api.tsx".to_string(),
                consume_line: 7,
                count: 3,
            },
        );
        let out = unconsumed_endpoint_findings(
            &[dead("GET /orphan", "be", "Api.java", 12)],
            &[],
            &targets,
            &no_trpc(),
        );
        assert_eq!(out.len(), 1);
        assert!(out[0].message.contains("3 unmatched http consume(s)"));
        assert!(out[0]
            .message
            .contains("cross-layer/route-near-miss` finding at Api.tsx:7"));
        let data = out[0].data.as_ref().unwrap();
        assert_eq!(data["nearMissConsumeCount"], 3);
        assert_eq!(data["nearMissConsumeExample"], "Api.tsx:7");
    }

    #[test]
    fn near_miss_cross_reference_note_is_absent_when_the_provide_is_not_a_near_miss_target() {
        let out = unconsumed_endpoint_findings(
            &[dead("GET /orphan", "be", "Api.java", 12)],
            &[],
            &no_near_miss(),
            &no_trpc(),
        );
        assert_eq!(out.len(), 1);
        assert!(!out[0].message.contains("near-miss"));
        assert!(out[0]
            .data
            .as_ref()
            .unwrap()
            .get("nearMissConsumeCount")
            .is_none());
    }

    #[test]
    fn trpc_mount_route_is_suppressed_when_its_own_tree_has_a_trpc_edge() {
        let out = unconsumed_endpoint_findings(
            &[dead(
                "GET /api/trpc/{}",
                "web",
                "pages/api/trpc/[trpc].ts",
                3,
            )],
            &[],
            &no_near_miss(),
            &trpc_sources(&["web"]),
        );
        assert!(out.is_empty());
    }

    #[test]
    fn trpc_mount_route_is_still_reported_when_no_tree_has_a_trpc_edge() {
        let out = unconsumed_endpoint_findings(
            &[dead(
                "GET /api/trpc/{}",
                "web",
                "pages/api/trpc/[trpc].ts",
                3,
            )],
            &[],
            &no_near_miss(),
            &no_trpc(),
        );
        assert_eq!(out.len(), 1);
        assert!(out[0].message.contains("GET /api/trpc/{}"));
    }

    #[test]
    fn trpc_mount_route_is_still_reported_when_only_a_different_tree_has_trpc_edges() {
        // Class A regression: a run-global `trpc_edge_count` gate would suppress tree "web"'s literal
        // trpc-segment route purely because tree "api" has trpc edges — the mount-IS-transport
        // justification only holds for the tree whose OWN edges flow through the route.
        let out = unconsumed_endpoint_findings(
            &[dead(
                "GET /api/trpc/{}",
                "web",
                "pages/api/trpc/[trpc].ts",
                3,
            )],
            &[],
            &no_near_miss(),
            &trpc_sources(&["api"]),
        );
        assert_eq!(out.len(), 1);
        assert!(out[0].message.contains("GET /api/trpc/{}"));
    }

    #[test]
    fn a_route_that_merely_contains_but_does_not_carry_a_trpc_segment_is_not_suppressed() {
        // "trpcish" is not the literal segment `trpc` — must not false-positive on substring match.
        let out = unconsumed_endpoint_findings(
            &[dead(
                "GET /api/trpcish/status",
                "web",
                "pages/api/trpcish/status.ts",
                3,
            )],
            &[],
            &no_near_miss(),
            &trpc_sources(&["web"]),
        );
        assert_eq!(out.len(), 1);
    }
}
