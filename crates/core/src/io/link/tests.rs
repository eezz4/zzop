//! Exercises `link_cross_layer_io`: consume-to-provide joins across trees, dead provides (nothing
//! consumes them), dangling consumes (nothing provides them), unresolved dynamic consumes are never
//! force-matched, provider symbols carry onto the edge, and same-tree matches are not flagged
//! cross-source. The integrity gates (ambiguity/external/low-confidence) live in `gates`; topology-host
//! re-keying in `hosts`; `http_interface_key` normalization tests live with their subject in `super::super::key`.

mod gates;
mod hosts;

use super::{link_cross_layer_io, LinkOptions};
use crate::io::{IoConsume, IoFacts, IoProvide, SourceIo};

fn provide(kind: &str, key: &str, file: &str, line: u32, symbol: Option<&str>) -> IoProvide {
    IoProvide {
        body: None,
        kind: kind.into(),
        key: key.into(),
        file: file.into(),
        line,
        symbol: symbol.map(Into::into),
    }
}
fn consume(kind: &str, key: Option<&str>, file: &str, line: u32, raw: Option<&str>) -> IoConsume {
    IoConsume {
        client: None,
        body: None,
        kind: kind.into(),
        key: key.map(Into::into),
        file: file.into(),
        line,
        raw: raw.map(Into::into),
        method: None,
    }
}

fn fixture() -> Vec<SourceIo> {
    let fe = SourceIo {
        source: "fe".into(),
        io: IoFacts {
            provides: vec![],
            consumes: vec![
                consume("http", Some("GET /authen/getUserInfo"), "Ctx.tsx", 37, None),
                consume("http", Some("GET /authen/getSignout"), "Ctx.tsx", 68, None),
                consume("http", Some("GET /missing/route"), "Ctx.tsx", 99, None), // dangling
                consume("http", None, "Dyn.tsx", 5, Some("axios.get(url)")),      // unresolved
            ],
        },
    };
    let be = SourceIo {
        source: "be".into(),
        io: IoFacts {
            provides: vec![
                provide(
                    "http",
                    "GET /authen/getUserInfo",
                    "CtrlAuthen.java",
                    40,
                    Some("getUserInfo"),
                ),
                provide(
                    "http",
                    "GET /authen/getSignout",
                    "CtrlAuthen.java",
                    56,
                    None,
                ),
                provide(
                    "http",
                    "GET /authen/getGoogleRedirect",
                    "CtrlAuthen.java",
                    25,
                    None,
                ), // dead
            ],
            consumes: vec![consume(
                "db-table",
                Some("table:users"),
                "RepoAuthen.java",
                12,
                None,
            )],
        },
    };
    let db = SourceIo {
        source: "db".into(),
        io: IoFacts {
            provides: vec![provide("db-table", "table:users", "schema.sql", 1, None)],
            consumes: vec![],
        },
    };
    vec![fe, be, db]
}

#[test]
fn joins_consume_to_provide_across_trees() {
    let r = link_cross_layer_io(&fixture(), &LinkOptions::default());
    let http: Vec<_> = r.edges.iter().filter(|e| e.kind == "http").collect();
    assert_eq!(http.len(), 2);
    // sorted by key -> getSignout first
    assert_eq!(http[0].key, "GET /authen/getSignout");
    assert_eq!(http[0].from.source, "fe");
    assert_eq!(http[0].from.line, 68);
    assert_eq!(http[0].to.source, "be");
    assert_eq!(http[0].to.line, 56);
    assert!(http[0].cross_source);
    // BE->DB edge also resolves (different kind / layer)
    let dbe = r.edges.iter().find(|e| e.kind == "db-table").unwrap();
    assert_eq!(dbe.from.source, "be");
    assert_eq!(dbe.to.source, "db");
    assert_eq!(dbe.to.file, "schema.sql");
    assert_eq!(r.edges.len(), 3);
}

#[test]
fn provide_nothing_consumes_is_dead_code() {
    let r = link_cross_layer_io(&fixture(), &LinkOptions::default());
    assert_eq!(r.unconsumed_provides.len(), 1);
    assert_eq!(
        r.unconsumed_provides[0].provide.key,
        "GET /authen/getGoogleRedirect"
    );
    assert_eq!(r.unconsumed_provides[0].source, "be");
}

#[test]
fn consume_nothing_provides_is_dangling() {
    let r = link_cross_layer_io(&fixture(), &LinkOptions::default());
    assert_eq!(r.unprovided_consumes.len(), 1);
    assert_eq!(
        r.unprovided_consumes[0].consume.key.as_deref(),
        Some("GET /missing/route")
    );
    assert_eq!(r.unprovided_consumes[0].source, "fe");
}

#[test]
fn unresolvable_dynamic_consume_never_force_matched() {
    let r = link_cross_layer_io(&fixture(), &LinkOptions::default());
    assert_eq!(r.unresolved_consumes.len(), 1);
    assert_eq!(
        r.unresolved_consumes[0].consume.raw.as_deref(),
        Some("axios.get(url)")
    );
    assert_eq!(r.unresolved_consumes[0].source, "fe");
}

#[test]
fn carries_provider_symbol_onto_edge() {
    let r = link_cross_layer_io(&fixture(), &LinkOptions::default());
    let e = r
        .edges
        .iter()
        .find(|x| x.key == "GET /authen/getUserInfo")
        .unwrap();
    assert_eq!(e.to.symbol.as_deref(), Some("getUserInfo"));
}

#[test]
fn intra_tree_match_is_not_cross_source() {
    let one = SourceIo {
        source: "be".into(),
        io: IoFacts {
            provides: vec![provide("queue", "topic:jobs", "Producer.java", 3, None)],
            consumes: vec![consume(
                "queue",
                Some("topic:jobs"),
                "Consumer.java",
                9,
                None,
            )],
        },
    };
    let r = link_cross_layer_io(&[one], &LinkOptions::default());
    assert_eq!(r.edges.len(), 1);
    assert!(!r.edges[0].cross_source);
    assert_eq!(r.unconsumed_provides.len(), 0);
}
