//! Tests for the http-provide shape — the fact that decides whether the rule fires and what it blames.

use zzop_core::io::{CrossLayerResult, EdgeFrom, EdgeTo, TaggedProvide};
use zzop_core::{CrossLayerEdge, IoProvide};

use super::*;

fn provide(kind: &str, key: &str) -> TaggedProvide {
    TaggedProvide {
        source: "be".to_string(),
        provide: IoProvide {
            response: None,
            body: None,
            kind: kind.to_string(),
            key: key.to_string(),
            file: "be/urls.py".to_string(),
            line: 1,
            symbol: None,
            ..Default::default()
        },
    }
}

fn empty() -> CrossLayerResult {
    CrossLayerResult {
        edges: Vec::new(),
        unconsumed_provides: Vec::new(),
        unprovided_consumes: Vec::new(),
        unresolved_consumes: Vec::new(),
        external_consumes: Vec::new(),
        ambiguous_consumes: Vec::new(),
        host_rekey_counts: Vec::new(),
        wildcard_route_partitions: Vec::new(),
    }
}

#[test]
fn a_run_with_no_http_provide_at_all_is_none() {
    assert_eq!(classify(&empty()), HttpProvideShape::None);
    let mut cl = empty();
    cl.unconsumed_provides = vec![provide("db-table", "users")];
    assert_eq!(
        classify(&cl),
        HttpProvideShape::None,
        "a db-table provide says nothing about route topology"
    );
}

/// The core of V97: a `?`-verb provide is a real http provide that can NEVER join, and the
/// old boolean counted it as "routes to join against". This is the `be-django` shape.
#[test]
fn a_run_whose_every_route_is_verb_unknown_is_not_joinable() {
    let mut cl = empty();
    cl.unconsumed_provides = vec![
        provide("http", "? /api/articles/feed"),
        provide("http", "? /api/profiles/{username}"),
    ];
    assert_eq!(classify(&cl), HttpProvideShape::VerbUnknownOnly);
}

#[test]
fn one_real_verb_makes_the_run_joinable() {
    let mut cl = empty();
    cl.unconsumed_provides = vec![provide("http", "GET /api/articles")];
    assert_eq!(
        classify(&cl),
        HttpProvideShape::Joinable {
            with_verb_unknown: false
        }
    );
}

/// The mixed case is real and gets its own answer — both causes are live, so both remedies are named.
#[test]
fn verb_unknown_routes_beside_joinable_ones_are_reported_as_both() {
    let mut cl = empty();
    cl.unconsumed_provides = vec![
        provide("http", "GET /api/articles"),
        provide("http", "? /api/articles/feed"),
    ];
    assert_eq!(
        classify(&cl),
        HttpProvideShape::Joinable {
            with_verb_unknown: true
        }
    );
}

/// An http EDGE landing is proof a real verb joined, even with no unconsumed provide to read.
#[test]
fn a_landed_http_edge_alone_is_joinable() {
    let mut cl = empty();
    cl.edges = vec![CrossLayerEdge {
        kind: "http".to_string(),
        key: "GET /api/articles".to_string(),
        from: EdgeFrom {
            source: "fe".to_string(),
            file: "fe/api.ts".to_string(),
            line: 1,
        },
        to: EdgeTo {
            source: "be".to_string(),
            file: "be/routes.ts".to_string(),
            line: 1,
            symbol: None,
        },
        cross_source: true,
        low_confidence_reason: None,
    }];
    assert_eq!(
        classify(&cl),
        HttpProvideShape::Joinable {
            with_verb_unknown: false
        }
    );
}

/// The sentence must not send a verb-unknown run to the base-path knobs, and must say the paths may
/// already match — that is the whole misdiagnosis V97 recorded.
#[test]
fn the_verb_unknown_sentence_does_not_blame_the_base_path() {
    let s = cause_sentence(HttpProvideShape::VerbUnknownOnly, 19);
    assert!(s.contains("verb-unknown"), "{s}");
    assert!(s.contains("already line up"), "{s}");
    assert!(
        !s.contains("the likely cause is ONE unresolved base path"),
        "still blaming the base: {s}"
    );
}

#[test]
fn the_joinable_sentence_is_unchanged() {
    let s = cause_sentence(
        HttpProvideShape::Joinable {
            with_verb_unknown: false,
        },
        19,
    );
    assert!(s.contains("ONE unresolved base path"), "{s}");
    assert!(s.contains("not 19 independent problems"), "{s}");
}

/// `routes` is offered exactly when there is a verb-unknown route for it to fix, and never otherwise —
/// a repair that cannot apply is noise in a message this rule already fills.
#[test]
fn the_routes_repair_appears_only_when_a_verb_unknown_route_exists() {
    assert!(routes_repair(HttpProvideShape::VerbUnknownOnly).contains("trees[].routes"));
    assert!(routes_repair(HttpProvideShape::Joinable {
        with_verb_unknown: true
    })
    .contains("trees[].routes"));
    assert_eq!(
        routes_repair(HttpProvideShape::Joinable {
            with_verb_unknown: false
        }),
        ""
    );
    assert_eq!(routes_repair(HttpProvideShape::None), "");
}

/// The two rules that speak about the same routes in one reply must name the same knob first.
/// `cross-layer/unknown-verb-route` leads with `routes`; until 2026-09-07 this rule never mentioned it.
#[test]
fn the_routes_repair_names_the_sibling_rule_that_lists_the_routes() {
    let r = routes_repair(HttpProvideShape::VerbUnknownOnly);
    assert!(r.contains("cross-layer/unknown-verb-route"), "{r}");
    assert!(r.contains("FIRST"), "{r}");
}
