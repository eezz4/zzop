//! Integrity gates on the raw join: cross-tree ambiguity (never auto-linked), external egress
//! (`"://"` keys never join), route identity (an all-`{}` key's miss is unresolved, not unprovided),
//! and low-confidence key patterns (edge emitted, but tagged).

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
fn all_placeholder_consume_key_misses_into_unresolved_not_unprovided() {
    // mono-hub `joke-generator/fetchJoke.ts` (2026-07-25): `const BASE = "https://v2.jokeapi.dev/joke"`
    // is unresolved, the host is lost, and the consume keys as `GET /{}`. That key names no route, so
    // calling it `unprovidedConsumes` asserts "this app calls its own route and that route is missing"
    // — a contract nobody ever claimed. It must land in `unresolvedConsumes` (a disclosed blind spot,
    // counted by `cross-layer/unresolved-consume-ratio`) instead.
    let fe = SourceIo {
        source: "fe".into(),
        io: IoFacts {
            provides: vec![],
            consumes: vec![
                consume("http", Some("GET /{}"), "fetchJoke.ts", 10, None),
                consume("http", Some("POST /{}/{}"), "fetchJoke.ts", 20, None),
                // One literal segment is enough evidence to keep asserting drift.
                consume("http", Some("GET /api/{}"), "client.ts", 3, None),
                // A root consume IS resolved — zero segments, but the path is fully known.
                consume("http", Some("GET /"), "client.ts", 4, None),
            ],
        },
    };
    let be = SourceIo {
        source: "be".into(),
        io: IoFacts {
            provides: vec![provide("http", "GET /health", "Api.java", 5, None)],
            consumes: vec![],
        },
    };
    let r = link_cross_layer_io(&[fe, be], &LinkOptions::default());

    let unresolved: Vec<Option<&str>> = r
        .unresolved_consumes
        .iter()
        .map(|c| c.consume.key.as_deref())
        .collect();
    assert_eq!(unresolved, vec![Some("GET /{}"), Some("POST /{}/{}")]);
    // The demoted records keep their key (locatable in `distinctBucketKeys`), unlike a `key: None` entry.
    assert!(r.unresolved_consumes.iter().all(|c| c.source == "fe"));

    let unprovided: Vec<&str> = r
        .unprovided_consumes
        .iter()
        .filter_map(|c| c.consume.key.as_deref())
        .collect();
    assert_eq!(unprovided, vec!["GET /api/{}", "GET /"]);
    assert!(r.edges.is_empty());
}

#[test]
fn an_all_placeholder_key_that_actually_joins_a_catch_all_provide_still_edges() {
    // The gate redirects a MISS only. A declared catch-all route (`app.get('/:page')` -> `GET /{}`) is a
    // real provide, so a consume that lands on it is a join, not a guess — demoting it would delete a
    // fact the analysis actually has.
    let fe = SourceIo {
        source: "fe".into(),
        io: IoFacts {
            provides: vec![],
            consumes: vec![consume("http", Some("GET /{}"), "client.ts", 1, None)],
        },
    };
    let be = SourceIo {
        source: "be".into(),
        io: IoFacts {
            provides: vec![provide("http", "GET /{}", "routes.ts", 8, None)],
            consumes: vec![],
        },
    };
    let r = link_cross_layer_io(&[fe, be], &LinkOptions::default());
    assert_eq!(r.edges.len(), 1);
    assert_eq!(r.edges[0].key, "GET /{}");
    assert!(r.unresolved_consumes.is_empty());
    assert!(r.unprovided_consumes.is_empty());
}

#[test]
fn an_all_placeholder_key_under_a_declared_host_is_still_external_not_unresolved() {
    // Gate ORDER: the `://` egress gate fires first, so a vendor absolute URL whose path happens to be
    // all-placeholder stays third-party egress — the host IS route identity of a kind, and reclassifying
    // it as "we are blind" would lose the one fact the key carries.
    let fe = SourceIo {
        source: "fe".into(),
        io: IoFacts {
            provides: vec![],
            consumes: vec![consume(
                "http",
                Some("GET https://v2.jokeapi.dev/{}"),
                "fetchJoke.ts",
                10,
                None,
            )],
        },
    };
    let r = link_cross_layer_io(&[fe], &LinkOptions::default());
    assert_eq!(r.external_consumes.len(), 1);
    assert!(r.unresolved_consumes.is_empty());
    assert!(r.unprovided_consumes.is_empty());
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
