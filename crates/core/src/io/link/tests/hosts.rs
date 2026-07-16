//! Topology hosts: consume-side re-keying of absolute-URL consumes against declared internal
//! hosts (`LinkOptions::internal_hosts`) — see that field's doc for the matching rule.

use super::{consume, provide};
use crate::io::{link_cross_layer_io, IoFacts, LinkOptions, SourceIo};

#[test]
fn absolute_url_consume_matching_a_declared_internal_host_is_rekeyed_and_joins() {
    let fe = SourceIo {
        source: "fe".into(),
        io: IoFacts {
            provides: vec![],
            consumes: vec![consume(
                "http",
                Some("GET https://api.foo.com/users"),
                "Client.tsx",
                1,
                None,
            )],
        },
    };
    let be = SourceIo {
        source: "be".into(),
        io: IoFacts {
            provides: vec![provide("http", "GET /users", "Api.java", 5, None)],
            consumes: vec![],
        },
    };
    let opts = LinkOptions {
        internal_hosts: vec!["api.foo.com".to_string()],
        ..LinkOptions::default()
    };
    let r = link_cross_layer_io(&[fe, be], &opts);

    assert!(r.external_consumes.is_empty());
    assert_eq!(r.edges.len(), 1, "{:?}", r.edges);
    assert_eq!(r.edges[0].key, "GET /users");
    assert!(r.edges[0].cross_source);
    assert_eq!(r.host_rekey_counts, vec![("api.foo.com".to_string(), 1)]);
}

#[test]
fn rekeyed_consume_without_a_provider_lands_in_unprovided_under_its_join_key() {
    // Field-measured seam (mono-hub, 2026-07-14): the join used the re-keyed internal key but
    // the bucket entry kept the original absolute URL — a scheme-carrying key leaking into
    // `unprovided_consumes`, a bucket whose consumers (the near-miss family) must never see
    // one. The bucket must carry the JOIN key; the original spelling moves to `raw`.
    let fe = SourceIo {
        source: "fe".into(),
        io: IoFacts {
            provides: vec![],
            consumes: vec![consume(
                "http",
                Some("GET https://api.foo.com/price/{}"),
                "Client.tsx",
                1,
                None,
            )],
        },
    };
    let be = SourceIo {
        source: "be".into(),
        io: IoFacts {
            provides: vec![provide("http", "GET /users", "Api.java", 5, None)],
            consumes: vec![],
        },
    };
    let opts = LinkOptions {
        internal_hosts: vec!["api.foo.com".to_string()],
        ..LinkOptions::default()
    };
    let r = link_cross_layer_io(&[fe, be], &opts);

    assert!(r.external_consumes.is_empty());
    assert_eq!(r.unprovided_consumes.len(), 1);
    let c = &r.unprovided_consumes[0].consume;
    assert_eq!(
        c.key.as_deref(),
        Some("GET /price/{}"),
        "bucket must carry the re-keyed join key, not the absolute URL"
    );
    assert_eq!(
        c.raw.as_deref(),
        Some("GET https://api.foo.com/price/{}"),
        "original absolute spelling is preserved as raw provenance"
    );
    assert_eq!(r.host_rekey_counts, vec![("api.foo.com".to_string(), 1)]);
}

#[test]
fn internal_host_match_is_case_insensitive() {
    let fe = SourceIo {
        source: "fe".into(),
        io: IoFacts {
            provides: vec![],
            consumes: vec![consume(
                "http",
                Some("GET https://API.FOO.com/users"),
                "Client.tsx",
                1,
                None,
            )],
        },
    };
    let be = SourceIo {
        source: "be".into(),
        io: IoFacts {
            provides: vec![provide("http", "GET /users", "Api.java", 5, None)],
            consumes: vec![],
        },
    };
    let opts = LinkOptions {
        internal_hosts: vec!["api.foo.com".to_string()],
        ..LinkOptions::default()
    };
    let r = link_cross_layer_io(&[fe, be], &opts);
    assert_eq!(r.edges.len(), 1, "{:?}", r.edges);
}

#[test]
fn consume_side_port_is_ignored_when_declared_host_carries_no_port() {
    let fe = SourceIo {
        source: "fe".into(),
        io: IoFacts {
            provides: vec![],
            consumes: vec![consume(
                "http",
                Some("GET https://api.foo.com:8443/users"),
                "Client.tsx",
                1,
                None,
            )],
        },
    };
    let be = SourceIo {
        source: "be".into(),
        io: IoFacts {
            provides: vec![provide("http", "GET /users", "Api.java", 5, None)],
            consumes: vec![],
        },
    };
    let opts = LinkOptions {
        internal_hosts: vec!["api.foo.com".to_string()],
        ..LinkOptions::default()
    };
    let r = link_cross_layer_io(&[fe, be], &opts);
    assert_eq!(r.edges.len(), 1, "{:?}", r.edges);
}

#[test]
fn declared_host_with_a_port_requires_an_exact_match() {
    let fe = SourceIo {
        source: "fe".into(),
        io: IoFacts {
            provides: vec![],
            consumes: vec![
                // Exact host:port match -> internal.
                consume(
                    "http",
                    Some("GET https://api.foo.com:8443/users"),
                    "A.tsx",
                    1,
                    None,
                ),
                // Same host, no port on the consume side -> declared host demanded a port, no match ->
                // stays external.
                consume(
                    "http",
                    Some("GET https://api.foo.com/orders"),
                    "B.tsx",
                    2,
                    None,
                ),
            ],
        },
    };
    let be = SourceIo {
        source: "be".into(),
        io: IoFacts {
            provides: vec![
                provide("http", "GET /users", "Api.java", 5, None),
                provide("http", "GET /orders", "Api.java", 9, None),
            ],
            consumes: vec![],
        },
    };
    let opts = LinkOptions {
        internal_hosts: vec!["api.foo.com:8443".to_string()],
        ..LinkOptions::default()
    };
    let r = link_cross_layer_io(&[fe, be], &opts);
    assert_eq!(r.edges.len(), 1, "{:?}", r.edges);
    assert_eq!(r.edges[0].key, "GET /users");
    assert_eq!(r.external_consumes.len(), 1);
    assert_eq!(
        r.external_consumes[0].consume.key.as_deref(),
        Some("GET https://api.foo.com/orders")
    );
    assert_eq!(
        r.host_rekey_counts,
        vec![("api.foo.com:8443".to_string(), 1)]
    );
}

#[test]
fn rekeyed_host_consume_drops_the_query_string_like_any_other_consume_key() {
    let fe = SourceIo {
        source: "fe".into(),
        io: IoFacts {
            provides: vec![],
            consumes: vec![consume(
                "http",
                Some("GET https://api.foo.com/users?limit=10"),
                "Client.tsx",
                1,
                None,
            )],
        },
    };
    let be = SourceIo {
        source: "be".into(),
        io: IoFacts {
            provides: vec![provide("http", "GET /users", "Api.java", 5, None)],
            consumes: vec![],
        },
    };
    let opts = LinkOptions {
        internal_hosts: vec!["api.foo.com".to_string()],
        ..LinkOptions::default()
    };
    let r = link_cross_layer_io(&[fe, be], &opts);
    assert_eq!(r.edges.len(), 1, "{:?}", r.edges);
    assert_eq!(r.edges[0].key, "GET /users");
}

#[test]
fn undeclared_host_stays_external_even_with_internal_hosts_configured() {
    let fe = SourceIo {
        source: "fe".into(),
        io: IoFacts {
            provides: vec![],
            consumes: vec![consume(
                "http",
                Some("GET https://other.com/x"),
                "Client.tsx",
                1,
                None,
            )],
        },
    };
    let opts = LinkOptions {
        internal_hosts: vec!["api.foo.com".to_string()],
        ..LinkOptions::default()
    };
    let r = link_cross_layer_io(&[fe], &opts);
    assert_eq!(r.external_consumes.len(), 1);
    assert!(r.edges.is_empty());
    assert_eq!(r.host_rekey_counts, vec![("api.foo.com".to_string(), 0)]);
}

#[test]
fn ws_scheme_stays_external_even_when_the_host_is_declared_internal() {
    let fe = SourceIo {
        source: "fe".into(),
        io: IoFacts {
            provides: vec![],
            consumes: vec![consume(
                "http",
                Some("GET ws://api.foo.com/socket"),
                "Client.tsx",
                1,
                None,
            )],
        },
    };
    let opts = LinkOptions {
        internal_hosts: vec!["api.foo.com".to_string()],
        ..LinkOptions::default()
    };
    let r = link_cross_layer_io(&[fe], &opts);
    assert_eq!(r.external_consumes.len(), 1);
    assert_eq!(
        r.external_consumes[0].consume.key.as_deref(),
        Some("GET ws://api.foo.com/socket")
    );
    assert!(r.edges.is_empty());
}
