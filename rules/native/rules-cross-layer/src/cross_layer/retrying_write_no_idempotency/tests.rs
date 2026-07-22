use super::*;
use std::collections::BTreeMap;
use zzop_core::io::{EdgeFrom, EdgeTo};
use zzop_core::{Attribute, AttributeStore, EntityRef};

fn edge(key: &str, from: (&str, &str, u32), to: (&str, &str, u32), cross: bool) -> CrossLayerEdge {
    CrossLayerEdge {
        kind: "http".into(),
        key: key.into(),
        from: EdgeFrom {
            source: from.0.into(),
            file: from.1.into(),
            line: from.2,
        },
        to: EdgeTo {
            source: to.0.into(),
            file: to.1.into(),
            line: to.2,
            symbol: None,
        },
        cross_source: cross,
        low_confidence_reason: None,
    }
}

fn site(s: &str, f: &str, l: u32) -> RetrySite {
    (s.into(), f.into(), l)
}

/// No injected/native evidence anywhere — old callers' default, and every increment-1 test's baseline.
fn no_attrs() -> BTreeMap<String, &'static AttributeStore> {
    BTreeMap::new()
}

#[test]
fn flags_retry_configured_write_edge() {
    let edges = vec![edge(
        "POST /api/orders",
        ("fe", "src/checkout.ts", 42),
        ("be", "src/orders.controller.ts", 10),
        true,
    )];
    let retry: BTreeSet<RetrySite> = [site("fe", "src/checkout.ts", 42)].into();
    let f = retrying_write_no_idempotency_findings(&edges, &retry, &no_attrs());
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].file, "src/checkout.ts");
    assert_eq!(f[0].line, 42);
    assert_eq!(f[0].severity, Severity::Critical);
    assert!(f[0].message.contains("POST /api/orders"));
    assert!(f[0].message.contains("src/orders.controller.ts:10"));
    assert!(f[0].message.contains("across repositories"));
}

#[test]
fn ignores_edge_whose_consume_is_not_retry_configured() {
    let edges = vec![edge(
        "POST /api/orders",
        ("fe", "src/checkout.ts", 42),
        ("be", "src/orders.controller.ts", 10),
        true,
    )];
    // Empty retry-site set: the same call site without a witnessed retry policy is not flagged.
    let f = retrying_write_no_idempotency_findings(&edges, &BTreeSet::new(), &no_attrs());
    assert!(f.is_empty());
}

#[test]
fn skips_non_http_and_read_verbs() {
    let edges = vec![
        edge(
            "GET /api/orders",
            ("fe", "src/list.ts", 5),
            ("be", "src/orders.controller.ts", 3),
            true,
        ),
        {
            let mut e = edge(
                "table:orders",
                ("fe", "src/list.ts", 5),
                ("be", "src/orders.controller.ts", 3),
                true,
            );
            e.kind = "db-table".into();
            e
        },
    ];
    // Both the read edge and the db-table edge share a retry-configured site, yet neither is a
    // write http edge — nothing is flagged.
    let retry: BTreeSet<RetrySite> = [site("fe", "src/list.ts", 5)].into();
    assert!(retrying_write_no_idempotency_findings(&edges, &retry, &no_attrs()).is_empty());
}

#[test]
fn same_source_edge_omits_cross_repo_phrasing() {
    let edges = vec![edge(
        "PUT /api/user",
        ("app", "web/save.ts", 8),
        ("app", "server/user.ts", 20),
        false,
    )];
    let retry: BTreeSet<RetrySite> = [site("app", "web/save.ts", 8)].into();
    let f = retrying_write_no_idempotency_findings(&edges, &retry, &no_attrs());
    assert_eq!(f.len(), 1);
    assert!(!f[0].message.contains("across repositories"));
}

// --- Increment 2: the provider-guard veto channel ---

#[test]
fn veto_via_exact_iokey_attr_on_provider_source_suppresses_the_finding() {
    let edges = vec![edge(
        "POST /api/orders",
        ("fe", "src/checkout.ts", 42),
        ("be", "src/orders.controller.ts", 10),
        true,
    )];
    let retry: BTreeSet<RetrySite> = [site("fe", "src/checkout.ts", 42)].into();
    let store = AttributeStore::from_attrs(vec![Attribute {
        target: EntityRef::IoKey {
            kind: "http".into(),
            key: "POST /api/orders".into(),
        },
        key: IDEMPOTENCY_GUARDED_ATTR.into(),
        value: serde_json::json!(true),
    }]);
    let mut attrs: BTreeMap<String, &AttributeStore> = BTreeMap::new();
    attrs.insert("be".into(), &store);
    let f = retrying_write_no_idempotency_findings(&edges, &retry, &attrs);
    assert!(f.is_empty());
}

#[test]
fn veto_via_path_scope_covering_the_route_suppresses_the_finding() {
    let edges = vec![edge(
        "POST /api/orders",
        ("fe", "src/checkout.ts", 42),
        ("be", "src/orders.controller.ts", 10),
        true,
    )];
    let retry: BTreeSet<RetrySite> = [site("fe", "src/checkout.ts", 42)].into();
    let store = AttributeStore::from_attrs(vec![Attribute {
        target: EntityRef::PathScope {
            prefix: "/api".into(),
        },
        key: IDEMPOTENCY_GUARDED_ATTR.into(),
        value: serde_json::json!(true),
    }]);
    let mut attrs: BTreeMap<String, &AttributeStore> = BTreeMap::new();
    attrs.insert("be".into(), &store);
    let f = retrying_write_no_idempotency_findings(&edges, &retry, &attrs);
    assert!(f.is_empty());
}

#[test]
fn falsy_attr_value_does_not_veto_finding_still_emitted() {
    let edges = vec![edge(
        "POST /api/orders",
        ("fe", "src/checkout.ts", 42),
        ("be", "src/orders.controller.ts", 10),
        true,
    )];
    let retry: BTreeSet<RetrySite> = [site("fe", "src/checkout.ts", 42)].into();
    let store = AttributeStore::from_attrs(vec![Attribute {
        target: EntityRef::IoKey {
            kind: "http".into(),
            key: "POST /api/orders".into(),
        },
        key: IDEMPOTENCY_GUARDED_ATTR.into(),
        value: serde_json::json!(false),
    }]);
    let mut attrs: BTreeMap<String, &AttributeStore> = BTreeMap::new();
    attrs.insert("be".into(), &store);
    let f = retrying_write_no_idempotency_findings(&edges, &retry, &attrs);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].severity, Severity::Critical);
}

#[test]
fn attr_on_the_wrong_source_id_does_not_veto_finding_still_emitted() {
    let edges = vec![edge(
        "POST /api/orders",
        ("fe", "src/checkout.ts", 42),
        ("be", "src/orders.controller.ts", 10),
        true,
    )];
    let retry: BTreeSet<RetrySite> = [site("fe", "src/checkout.ts", 42)].into();
    // The attribute is real and truthy, but it's keyed under the CONSUMER's source id ("fe"), not the
    // provider's ("be") — the lookup is provider_attrs.get(edge.to.source), so this store is never found.
    let store = AttributeStore::from_attrs(vec![Attribute {
        target: EntityRef::IoKey {
            kind: "http".into(),
            key: "POST /api/orders".into(),
        },
        key: IDEMPOTENCY_GUARDED_ATTR.into(),
        value: serde_json::json!(true),
    }]);
    let mut attrs: BTreeMap<String, &AttributeStore> = BTreeMap::new();
    attrs.insert("fe".into(), &store);
    let f = retrying_write_no_idempotency_findings(&edges, &retry, &attrs);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].severity, Severity::Critical);
}

#[test]
fn severity_message_and_injection_stub_round_trip() {
    let edges = vec![edge(
        "POST /api/orders",
        ("fe", "src/checkout.ts", 42),
        ("be", "src/orders.controller.ts", 10),
        true,
    )];
    let retry: BTreeSet<RetrySite> = [site("fe", "src/checkout.ts", 42)].into();
    let f = retrying_write_no_idempotency_findings(&edges, &retry, &no_attrs());
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].severity, Severity::Critical);
    assert!(f[0].message.contains("idempotency-guarded"));
    let data = f[0].data.as_ref().expect("data present");
    let stub_str = data["injectionStub"]
        .as_str()
        .expect("injectionStub is a string");
    let stub: serde_json::Value =
        serde_json::from_str(stub_str).expect("injectionStub round-trips as JSON");
    assert_eq!(stub["target"]["ioKey"]["key"], "POST /api/orders");
    assert_eq!(stub["target"]["ioKey"]["kind"], "http");
    assert_eq!(stub["key"], IDEMPOTENCY_GUARDED_ATTR);
    assert_eq!(stub["value"], true);
}

#[test]
fn truthy_attr_targeting_a_different_route_key_does_not_veto() {
    let edges = vec![edge(
        "POST /api/orders",
        ("fe", "src/checkout.ts", 42),
        ("be", "src/orders.controller.ts", 10),
        true,
    )];
    let retry: BTreeSet<RetrySite> = [site("fe", "src/checkout.ts", 42)].into();
    // A store exists for the right provider source, and the attribute is truthy, but it targets an
    // unrelated route (exact key mismatch, and the prefix doesn't cover /api/orders either).
    let store = AttributeStore::from_attrs(vec![Attribute {
        target: EntityRef::IoKey {
            kind: "http".into(),
            key: "POST /api/other".into(),
        },
        key: IDEMPOTENCY_GUARDED_ATTR.into(),
        value: serde_json::json!(true),
    }]);
    let mut attrs: BTreeMap<String, &AttributeStore> = BTreeMap::new();
    attrs.insert("be".into(), &store);
    let f = retrying_write_no_idempotency_findings(&edges, &retry, &attrs);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].severity, Severity::Critical);
}
