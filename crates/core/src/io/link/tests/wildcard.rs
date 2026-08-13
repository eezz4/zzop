//! The fifth gate: ANT wildcard-route partition (`super::super::super::wildcard`). A route path that is
//! a PATTERN (`GET /api/files/**`) is lifted out of the exact join rather than compared as a key — the
//! predicate itself is unit-tested with its subject; what is proven here is what the LINKER does with it,
//! which no predicate test can show: which buckets move, which do not, and that `edges` does not.

use super::{consume, provide};
use crate::io::{link_cross_layer_io, IoFacts, LinkOptions, SourceIo};

/// FE calling a Spring-style catch-all: two calls beneath it, one control call to a route nobody serves,
/// and one control POST beneath the GET-only catch-all.
fn trees() -> Vec<SourceIo> {
    vec![
        SourceIo {
            source: "fe".into(),
            io: IoFacts {
                provides: vec![],
                consumes: vec![
                    consume("http", Some("GET /api/files/a/b/c"), "api.ts", 1, None),
                    consume(
                        "http",
                        Some("GET /api/files/img/logo.png"),
                        "api.ts",
                        2,
                        None,
                    ),
                    consume("http", Some("GET /api/orders"), "api.ts", 3, None),
                    consume("http", Some("GET /api/ghost"), "api.ts", 4, None),
                    consume("http", Some("POST /api/files/new"), "api.ts", 5, None),
                ],
            },
        },
        SourceIo {
            source: "be".into(),
            io: IoFacts {
                provides: vec![
                    provide("http", "GET /api/files/**", "Ctl.java", 4, Some("serve")),
                    provide("http", "GET /api/orders", "Ctl.java", 9, Some("orders")),
                ],
                consumes: vec![],
            },
        },
    ]
}

#[test]
fn the_pattern_leaves_both_residue_buckets_while_the_edge_count_stays_put() {
    let out = link_cross_layer_io(&trees(), &LinkOptions::default());

    // The load-bearing pairing: the win is three removed rows, NOT a new edge. A partition cannot make
    // one — reading success as "more edges" is how the original prescription for this defect went wrong.
    assert_eq!(out.edges.len(), 1, "{:?}", out.edges);
    assert_eq!(out.edges[0].key, "GET /api/orders");

    // Falsehood #1: the catch-all is a live route, never dead code.
    assert!(
        out.unconsumed_provides.is_empty(),
        "a pattern nobody spells back is not an unconsumed route: {:?}",
        out.unconsumed_provides
    );

    // Falsehoods #2 and #3 gone; both controls survive. Losing either control would mean the suppression
    // went blanket — the exact failure this gate must not become.
    let unprovided: Vec<&str> = out
        .unprovided_consumes
        .iter()
        .filter_map(|c| c.consume.key.as_deref())
        .collect();
    assert_eq!(
        unprovided,
        vec!["GET /api/ghost", "POST /api/files/new"],
        "only the calls the pattern does NOT serve may remain"
    );

    // Suppressed consumes go to no bucket at all — deliberately NOT `unresolvedConsumes`, which means
    // "blind about this target" and feeds the disclosed blindness ratio. We are not blind here: the
    // route serves the call. The disclosure is the partition row instead.
    assert!(
        out.unresolved_consumes.is_empty(),
        "{:?}",
        out.unresolved_consumes
    );

    // The substrate that carries the silence out.
    assert_eq!(out.wildcard_route_partitions.len(), 1);
    let p = &out.wildcard_route_partitions[0];
    assert_eq!(
        (p.source.as_str(), p.key.as_str()),
        ("be", "GET /api/files/**")
    );
    assert_eq!((p.file.as_str(), p.line), ("Ctl.java", 4));
    assert_eq!(p.covered_consumes, 2);
}

#[test]
fn an_exact_hit_is_never_reinterpreted_by_a_pattern() {
    // A tree that declares BOTH a catch-all and the exact route beneath it: the exact key must still
    // join into a real edge. The partition is asked only of a MISS — same discipline as the
    // route-identity gate, and for the same reason (a join is not a guess and no pattern may undo one).
    let trees = vec![
        SourceIo {
            source: "fe".into(),
            io: IoFacts {
                provides: vec![],
                consumes: vec![consume(
                    "http",
                    Some("GET /files/report"),
                    "api.ts",
                    1,
                    None,
                )],
            },
        },
        SourceIo {
            source: "be".into(),
            io: IoFacts {
                provides: vec![
                    provide("http", "GET /files/**", "Ctl.java", 4, None),
                    provide("http", "GET /files/report", "Ctl.java", 9, Some("report")),
                ],
                consumes: vec![],
            },
        },
    ];
    let out = link_cross_layer_io(&trees, &LinkOptions::default());
    assert_eq!(out.edges.len(), 1, "{:?}", out.edges);
    assert_eq!(out.edges[0].to.symbol.as_deref(), Some("report"));
    // The catch-all is still partitioned, and it swallowed nothing — a real and reportable value.
    assert_eq!(out.wildcard_route_partitions.len(), 1);
    assert_eq!(out.wildcard_route_partitions[0].covered_consumes, 0);
}

#[test]
fn a_call_under_two_catch_alls_is_charged_to_exactly_one_of_them() {
    // Two overlapping patterns in two trees. Charging both would double-count one call site and make
    // the disclosed totals larger than the population they came from.
    let trees = vec![
        SourceIo {
            source: "fe".into(),
            io: IoFacts {
                provides: vec![],
                consumes: vec![consume("http", Some("GET /files/a/b"), "api.ts", 1, None)],
            },
        },
        SourceIo {
            source: "be-a".into(),
            io: IoFacts {
                provides: vec![provide("http", "GET /files/**", "A.java", 4, None)],
                consumes: vec![],
            },
        },
        SourceIo {
            source: "be-b".into(),
            io: IoFacts {
                provides: vec![provide("http", "GET /**", "B.java", 4, None)],
                consumes: vec![],
            },
        },
    ];
    let out = link_cross_layer_io(&trees, &LinkOptions::default());
    assert!(out.unprovided_consumes.is_empty());
    let charged: usize = out
        .wildcard_route_partitions
        .iter()
        .map(|p| p.covered_consumes)
        .sum();
    assert_eq!(
        charged, 1,
        "one call site, one charge: {:?}",
        out.wildcard_route_partitions
    );
    // Both routes are still disclosed — the reader must see every pattern that left the join, even the
    // one that was charged nothing.
    assert_eq!(out.wildcard_route_partitions.len(), 2);
}

#[test]
fn the_partition_is_http_only_and_never_touches_another_channels_key() {
    // `*` has no ANT meaning in a `db-table`/`queue` key; a non-http key carrying one must join or
    // dangle exactly as before. The `kind` guard is what keeps this gate inside the HTTP vocabulary.
    let trees = vec![SourceIo {
        source: "svc".into(),
        io: IoFacts {
            provides: vec![provide("queue", "topic:events.*", "q.ts", 1, None)],
            consumes: vec![consume(
                "queue",
                Some("topic:events.created"),
                "c.ts",
                2,
                None,
            )],
        },
    }];
    let out = link_cross_layer_io(&trees, &LinkOptions::default());
    assert!(out.wildcard_route_partitions.is_empty());
    assert_eq!(
        out.unconsumed_provides.len(),
        1,
        "{:?}",
        out.unconsumed_provides
    );
    assert_eq!(
        out.unprovided_consumes.len(),
        1,
        "{:?}",
        out.unprovided_consumes
    );
}
