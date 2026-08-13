//! `cross-layer/external-ip-literal` (warning) — an external HTTP consume whose host is a raw IP literal
//! (dotted-quad IPv4, or bracketed IPv6) rather than a hostname. A hardcoded IP pins the call to one specific
//! network/environment and silently breaks — or silently points elsewhere — once the target moves; hostnames
//! exist so infra can rotate underneath a stable name.
//!
//! What is measured is therefore precisely *"this literal PINS AN ENVIRONMENT"*, and that is the whole test
//! for what belongs here. Loopback literals (`127.0.0.0/8`, `[::1]`) are excluded because they pin none:
//! every machine's loopback is its own, so the address designates no host that could move and no network
//! that could be rotated out from under the call. Private-range IPs (`10.x`, `192.168.x`, ...) are NOT
//! excluded for exactly the same reason read the other way — an internal IP names one specific network,
//! which is the environment-drift signal this rule exists to surface. Anchored at the consume site.
//!
//! The exclusion is stated in the finding text as this rule's own scope, and names no other rule. Whether a
//! committed loopback URL is a problem at all is a judgment about the project rather than about the line, so
//! it is not a claim this rule can make — and pointing at whoever might make it is only meaningful while that
//! owner is guaranteed to be running, which nothing here can guarantee.

use zzop_core::io::TaggedConsume;
use zzop_core::{disable_hint, Finding, Severity};

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
             are intentionally out of scope: what this rule measures is a literal PINNING AN ENVIRONMENT, and \
             loopback pins none — every machine's loopback is its own, so there is no host that can move and \
             no network to drift away from. Private-range IPs ARE flagged, because those do name one specific \
             network.) \
             {} if this integration legitimately targets a fixed IP on purpose (e.g. a pinned on-prem \
             appliance with no DNS entry).",
            url.method, url.host, url.path, c.source,
            disable_hint("cross-layer/external-ip-literal"),
        );

        out.push(Finding {
            rule_id: "cross-layer/external-ip-literal".to_string(),
            severity: Severity::Warning,
            file: c.consume.file.clone(),
            line: c.consume.line,
            message,
            evidence_paths: Vec::new(),
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
mod tests;
