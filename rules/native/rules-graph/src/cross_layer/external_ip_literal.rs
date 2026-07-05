//! `cross-layer/external-ip-literal` (warning) — an external HTTP consume whose host is a raw IP literal
//! (dotted-quad IPv4, or bracketed IPv6) rather than a hostname. A hardcoded IP pins the call to one specific
//! network/environment and silently breaks — or silently points elsewhere — once the target moves; hostnames
//! exist so infra can rotate underneath a stable name. Loopback literals (`127.0.0.0/8`, `[::1]`) are excluded
//! as the separate "committed dev config" smell owned by the DSL `localhost-egress-committed` rule.
//! Private-range IPs (`10.x`, `192.168.x`, ...) are NOT excluded: a hardcoded internal IP is still the
//! environment-drift signal this rule exists to surface. Anchored at the consume site.

use zpz_core::io::TaggedConsume;
use zpz_core::{Finding, Severity};

use super::split_external_key;

/// Classifies `host` (scheme already stripped) as an IP literal. Returns `Some((ip_literal, is_loopback))`
/// after stripping an optional trailing `:port` (or the bracket-aware equivalent for IPv6); `None` for
/// anything else — hostnames, malformed input — never panics.
fn classify_ip(host: &str) -> Option<(String, bool)> {
    if let Some(rest) = host.strip_prefix('[') {
        let end = rest.find(']')?;
        let inner = &rest[..end];
        if !looks_like_ipv6(inner) {
            return None;
        }
        let is_loopback = inner == "::1";
        return Some((format!("[{inner}]"), is_loopback));
    }

    let host_no_port = strip_ipv4_port(host);
    let octets = parse_ipv4(host_no_port)?;
    let is_loopback = octets[0] == 127;
    Some((host_no_port.to_string(), is_loopback))
}

/// Strips a trailing `:port` from a non-bracketed host, only when the suffix after the last `:` is entirely
/// digits (so a bare hostname/IPv4 with no port is left untouched).
fn strip_ipv4_port(host: &str) -> &str {
    let Some(idx) = host.rfind(':') else {
        return host;
    };
    let port = &host[idx + 1..];
    if !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) {
        &host[..idx]
    } else {
        host
    }
}

/// Parses a dotted-quad IPv4 literal: exactly 4 digit-only segments, each 0-255. `None` for anything else.
fn parse_ipv4(s: &str) -> Option<[u16; 4]> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    let mut out = [0u16; 4];
    for (i, p) in parts.iter().enumerate() {
        if p.is_empty() || !p.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let n: u16 = p.parse().ok()?;
        if n > 255 {
            return None;
        }
        out[i] = n;
    }
    Some(out)
}

/// Loose IPv6-literal shape check (hex digits, `:`, `.` for IPv4-mapped forms, at least one `:`) — not a
/// full RFC-4291 validator, just enough to distinguish an IPv6 literal from an arbitrary bracketed hostname.
fn looks_like_ipv6(s: &str) -> bool {
    !s.is_empty()
        && s.contains(':')
        && s.chars()
            .all(|c| c.is_ascii_hexdigit() || c == ':' || c == '.')
}

pub fn external_ip_literal_findings(external_consumes: &[TaggedConsume]) -> Vec<Finding> {
    let mut out = Vec::new();
    for c in external_consumes
        .iter()
        .filter(|c| c.consume.kind == "http")
    {
        let Some(key) = c.consume.key.as_deref() else {
            continue;
        };
        let Some(url) = split_external_key(key) else {
            continue;
        };
        let Some((ip, is_loopback)) = classify_ip(url.host) else {
            continue;
        };
        if is_loopback {
            continue;
        }

        let message = format!(
            "external call `{} {}{}` (source `{}`) targets the raw IP literal `{ip}` instead of a hostname. \
             A literal IP baked into source hardcodes one specific network/environment and will silently \
             break — or silently point at the wrong place — if the target ever moves; DNS-backed hostnames \
             are what let infra rotate underneath a stable name. Verify whether this should be a real hostname \
             (config/DNS drift) and replace the literal with one. (Loopback literals like `127.0.0.1`/`[::1]` \
             are intentionally not flagged here — that's the DSL `localhost-egress-committed` rule's turf.) \
             Disable via rule config `disabled_rules: [\"cross-layer/external-ip-literal\"]` if this \
             integration legitimately targets a fixed IP on purpose (e.g. a pinned on-prem appliance with no \
             DNS entry).",
            url.method, url.host, url.path, c.source,
        );

        out.push(Finding {
            rule_id: "cross-layer/external-ip-literal".to_string(),
            severity: Severity::Warning,
            file: c.consume.file.clone(),
            line: c.consume.line,
            message,
            data: Some(serde_json::json!({
                "key": key,
                "host": url.host,
                "ip": ip,
                "consumeSource": c.source,
            })),
        });
    }
    out.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn consume(
        kind: &str,
        key: Option<&str>,
        source: &str,
        file: &str,
        line: u32,
    ) -> TaggedConsume {
        TaggedConsume {
            source: source.to_string(),
            consume: zpz_core::IoConsume {
                kind: kind.to_string(),
                key: key.map(str::to_string),
                file: file.to_string(),
                line,
                raw: None,
                method: None,
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
        assert!(out[0].message.contains("disabled_rules"));
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
}
