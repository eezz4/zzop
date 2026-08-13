use super::*;

fn consume(kind: &str, key: Option<&str>, source: &str, file: &str, line: u32) -> TaggedConsume {
    TaggedConsume {
        source: source.to_string(),
        consume: zzop_core::IoConsume {
            client: None,
            body: None,
            kind: kind.to_string(),
            key: key.map(str::to_string),
            file: file.to_string(),
            line,
            raw: None,
            method: None,
            retry_configured: None,
        },
    }
}

#[test]
fn ipv4_literal_host_is_flagged() {
    let external = vec![consume(
        "http",
        Some("GET https://203.0.113.5/v1/users"),
        "fe",
        "Client.ts",
        10,
    )];
    let out = external_ip_literal_findings(&external);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].rule_id, "cross-layer/external-ip-literal");
    assert_eq!(out[0].severity, Severity::Warning);
    assert_eq!(out[0].file, "Client.ts");
    assert_eq!(out[0].line, 10);
    assert!(out[0].message.contains("203.0.113.5"));
    assert!(out[0].message.contains("disabledRules"));
    let data = out[0].data.as_ref().unwrap();
    assert_eq!(data["ip"], "203.0.113.5");
}

#[test]
fn ipv4_literal_with_port_is_flagged_with_port_stripped() {
    let external = vec![consume(
        "http",
        Some("GET https://203.0.113.5:8443/v1/users"),
        "fe",
        "Client.ts",
        10,
    )];
    let out = external_ip_literal_findings(&external);
    assert_eq!(out.len(), 1);
    let data = out[0].data.as_ref().unwrap();
    assert_eq!(data["ip"], "203.0.113.5");
}

#[test]
fn private_range_ipv4_still_fires() {
    let external = vec![consume(
        "http",
        Some("GET https://10.0.4.12/v1/users"),
        "fe",
        "Client.ts",
        10,
    )];
    assert_eq!(external_ip_literal_findings(&external).len(), 1);
}

/// The loopback exclusion must justify itself, and must do it WITHOUT naming another rule.
///
/// The message used to hand the case off by id ("that's the DSL `...-url-literal-committed` rule's turf").
/// A pointer like that is only worth anything while the rule it names is guaranteed to be running, and no
/// finding can guarantee that about a rule in some other pack — the named owner may be unloaded, renamed, or
/// simply absent, and then the sentence sends the reader after nothing. So the contract is inverted here: the
/// exclusion has to stand on this rule's OWN yardstick (a literal that pins an environment; loopback pins
/// none), and NO rule id may appear in the text. This assertion is what refuses a re-planted pointer.
///
/// `disable_hint` puts this rule's own id in the message, which is the one id that is always true here —
/// the check below is scoped to ids OTHER than that one.
#[test]
fn the_loopback_exclusion_is_justified_without_naming_any_other_rule() {
    let external = vec![consume(
        "http",
        Some("GET https://203.0.113.5/v1/users"),
        "fe",
        "Client.ts",
        10,
    )];
    let out = external_ip_literal_findings(&external);
    assert_eq!(out.len(), 1);
    let message = &out[0].message;

    assert!(
        message.contains("loopback pins none"),
        "the exclusion must carry its own reason: {message}"
    );

    let foreign_ids: Vec<&str> = message
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '/'))
        .filter(|t| t.contains('/') && t.contains('-'))
        .filter(|t| *t != "cross-layer/external-ip-literal")
        .collect();
    assert!(
        foreign_ids.is_empty(),
        "the message must not delegate to another rule id, found {foreign_ids:?}: {message}"
    );
}

#[test]
fn loopback_ipv4_is_excluded() {
    let external = vec![consume(
        "http",
        Some("GET https://127.0.0.1:3000/v1/users"),
        "fe",
        "Client.ts",
        10,
    )];
    assert!(external_ip_literal_findings(&external).is_empty());
}

#[test]
fn loopback_ipv6_bracketed_is_excluded() {
    let external = vec![consume(
        "http",
        Some("GET https://[::1]:3000/v1/users"),
        "fe",
        "Client.ts",
        10,
    )];
    assert!(external_ip_literal_findings(&external).is_empty());
}

#[test]
fn non_loopback_ipv6_bracketed_literal_fires() {
    let external = vec![consume(
        "http",
        Some("GET https://[2001:db8::1]/v1/users"),
        "fe",
        "Client.ts",
        10,
    )];
    let out = external_ip_literal_findings(&external);
    assert_eq!(out.len(), 1);
    let data = out[0].data.as_ref().unwrap();
    assert_eq!(data["ip"], "[2001:db8::1]");
}

#[test]
fn hostname_is_not_flagged() {
    let external = vec![consume(
        "http",
        Some("GET https://api.vendor.com/v1/users"),
        "fe",
        "Client.ts",
        10,
    )];
    assert!(external_ip_literal_findings(&external).is_empty());
}

#[test]
fn non_http_kind_is_ignored() {
    let external = vec![consume(
        "queue",
        Some("GET https://203.0.113.5/v1/users"),
        "fe",
        "Client.ts",
        10,
    )];
    assert!(external_ip_literal_findings(&external).is_empty());
}

#[test]
fn findings_are_sorted_deterministically_by_file_then_line() {
    let external = vec![
        consume(
            "http",
            Some("GET https://203.0.113.5/v1/x"),
            "fe",
            "b.ts",
            5,
        ),
        consume(
            "http",
            Some("GET https://198.51.100.9/v1/x"),
            "fe",
            "a.ts",
            20,
        ),
        consume(
            "http",
            Some("GET https://198.51.100.9/v1/y"),
            "fe",
            "a.ts",
            3,
        ),
    ];
    let out = external_ip_literal_findings(&external);
    assert_eq!(out.len(), 3);
    assert_eq!((out[0].file.as_str(), out[0].line), ("a.ts", 3));
    assert_eq!((out[1].file.as_str(), out[1].line), ("a.ts", 20));
    assert_eq!((out[2].file.as_str(), out[2].line), ("b.ts", 5));
}
