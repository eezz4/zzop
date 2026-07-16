//! Integrity gates on the raw join: cross-tree ambiguity (never auto-linked), external egress
//! (`"://"` keys never join), and low-confidence key patterns (edge emitted, but tagged).

use super::{consume, provide};
use crate::io::{link_cross_layer_io, IoFacts, LinkOptions, SourceIo};

#[test]
fn key_provided_by_two_distinct_trees_is_ambiguous_not_edged() {
    // Same key ("GET /health") provided by TWO different source trees — a many-to-many join across
    // trees would silently pick one; instead this must land in `ambiguousConsumes` with both candidates,
    // emit no edges for it, and NOT appear in `unconsumedProvides` either (it IS referenced, just
    // ambiguously).
    let a = SourceIo {
        source: "svc-a".into(),
        io: IoFacts {
            provides: vec![provide("http", "GET /health", "svc-a/health.ts", 3, None)],
            consumes: vec![],
        },
    };
    let b = SourceIo {
        source: "svc-b".into(),
        io: IoFacts {
            provides: vec![provide("http", "GET /health", "svc-b/health.ts", 7, None)],
            consumes: vec![],
        },
    };
    let caller = SourceIo {
        source: "gateway".into(),
        io: IoFacts {
            provides: vec![],
            consumes: vec![consume("http", Some("GET /health"), "gw.ts", 1, None)],
        },
    };
    let r = link_cross_layer_io(&[a, b, caller], &LinkOptions::default());

    assert!(
        r.edges.iter().all(|e| e.key != "GET /health"),
        "ambiguous key must not produce edges: {:?}",
        r.edges
    );
    assert_eq!(r.ambiguous_consumes.len(), 1);
    assert_eq!(r.ambiguous_consumes[0].source, "gateway");
    assert_eq!(
        r.ambiguous_consumes[0].consume.key.as_deref(),
        Some("GET /health")
    );
    assert_eq!(r.ambiguous_consumes[0].candidates.len(), 2);
    // deterministically sorted by (source, file, line)
    assert_eq!(r.ambiguous_consumes[0].candidates[0].source, "svc-a");
    assert_eq!(r.ambiguous_consumes[0].candidates[1].source, "svc-b");

    assert!(
        r.unconsumed_provides
            .iter()
            .all(|p| p.provide.key != "GET /health"),
        "ambiguous-candidate provides must not be counted dead: {:?}",
        r.unconsumed_provides
    );
}

#[test]
fn multi_tree_provided_key_nobody_consumes_is_still_dead() {
    // Two trees provide the same key and NO consume references it at all — the provider-set being
    // multi-tree must not exempt it from `unconsumedProvides` (that exemption is only for keys an
    // actual consume referenced ambiguously).
    let a = SourceIo {
        source: "svc-a".into(),
        io: IoFacts {
            provides: vec![provide("http", "DELETE /api/me", "svc-a/me.ts", 3, None)],
            consumes: vec![],
        },
    };
    let b = SourceIo {
        source: "svc-b".into(),
        io: IoFacts {
            provides: vec![provide("http", "DELETE /api/me", "svc-b/me.ts", 9, None)],
            consumes: vec![],
        },
    };
    let r = link_cross_layer_io(&[a, b], &LinkOptions::default());
    assert!(r.edges.is_empty());
    assert!(r.ambiguous_consumes.is_empty());
    assert_eq!(
        r.unconsumed_provides.len(),
        2,
        "both unconsumed provider entries must be reported dead: {:?}",
        r.unconsumed_provides
    );
}

#[test]
fn multi_provider_within_one_tree_is_unaffected_by_ambiguity_gate() {
    // Two providers for the same key, but BOTH from the same source tree — legal multi-provider case
    // (e.g. a tree exposing a topic twice), unaffected by the cross-tree ambiguity gate: edges to each.
    let one = SourceIo {
        source: "be".into(),
        io: IoFacts {
            provides: vec![
                provide("http", "GET /ping", "a.ts", 1, None),
                provide("http", "GET /ping", "b.ts", 2, None),
            ],
            consumes: vec![consume("http", Some("GET /ping"), "c.ts", 3, None)],
        },
    };
    let r = link_cross_layer_io(&[one], &LinkOptions::default());
    assert_eq!(r.edges.len(), 2);
    assert!(r.ambiguous_consumes.is_empty());
}

#[test]
fn host_carrying_consume_key_is_external_never_dangling_even_with_a_matching_internal_provide() {
    // "GET https://vendor.com/api/users" must route to `external`, never join even though an
    // internal "GET /api/users" provide exists in the same analysis — the host makes it egress, not
    // an internal route reference.
    let fe = SourceIo {
        source: "fe".into(),
        io: IoFacts {
            provides: vec![],
            consumes: vec![consume(
                "http",
                Some("GET https://vendor.com/api/users"),
                "Client.tsx",
                10,
                None,
            )],
        },
    };
    let be = SourceIo {
        source: "be".into(),
        io: IoFacts {
            provides: vec![provide("http", "GET /api/users", "Api.java", 5, None)],
            consumes: vec![],
        },
    };
    let r = link_cross_layer_io(&[fe, be], &LinkOptions::default());

    assert_eq!(r.external_consumes.len(), 1);
    assert_eq!(
        r.external_consumes[0].consume.key.as_deref(),
        Some("GET https://vendor.com/api/users")
    );
    assert_eq!(r.external_consumes[0].source, "fe");
    assert!(r.unprovided_consumes.is_empty());
    assert!(r.edges.is_empty());
    // The internal BE provide is untouched — nothing consumed it, so it's dead, unrelated to external.
    assert_eq!(r.unconsumed_provides.len(), 1);
    assert_eq!(r.unconsumed_provides[0].provide.key, "GET /api/users");
}

#[test]
fn edge_key_matching_a_low_confidence_pattern_carries_the_reason() {
    let fe = SourceIo {
        source: "fe".into(),
        io: IoFacts {
            provides: vec![],
            consumes: vec![
                consume("http", Some("GET /health"), "Client.tsx", 1, None),
                consume("http", Some("GET /orders"), "Client.tsx", 2, None),
            ],
        },
    };
    let be = SourceIo {
        source: "be".into(),
        io: IoFacts {
            provides: vec![
                provide("http", "GET /health", "Api.java", 1, None),
                provide("http", "GET /orders", "Api.java", 9, None),
            ],
            consumes: vec![],
        },
    };
    let opts = LinkOptions {
        low_confidence_key_patterns: vec![(
            regex::Regex::new(r"^GET /health$").unwrap(),
            "generic path shared by many services".to_string(),
        )],
        ..LinkOptions::default()
    };
    let r = link_cross_layer_io(&[fe, be], &opts);

    let health = r.edges.iter().find(|e| e.key == "GET /health").unwrap();
    assert_eq!(
        health.low_confidence_reason.as_deref(),
        Some("generic path shared by many services")
    );
    let orders = r.edges.iter().find(|e| e.key == "GET /orders").unwrap();
    assert_eq!(orders.low_confidence_reason, None);
}
