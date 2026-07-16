//! `cross-layer/external-base-url-drift` (info) — the same PATH is consumed as external egress against 2+
//! hosts that plausibly belong to ONE logical service (see the registrable-domain guard below). Host
//! strings include port (a port-only difference also counts as drift), and is narrowed to paths with 2+
//! non-empty segments — a single-segment path like `/api` is too generic to attribute to one service.
//!
//! ## Registrable-domain guard
//! Grouping by path alone is not enough: vendors converge on conventional path shapes (`/oauth/token` is
//! the OAuth2 spec's own suggested path) regardless of vendor. This only fires when at least two hosts on
//! a path also share an (approximate) registrable domain — see [`registrable_domain`] — dropping any host
//! that shares nothing but the path with the rest of the group.
//!
//! Consume sites in test-path files (`zzop_core::is_test_file`) are skipped, including from the
//! per-path host-count threshold — a test mocking a vendor/own API is not deployed egress.

use std::collections::{BTreeMap, BTreeSet};

use zzop_core::io::TaggedConsume;
use zzop_core::{disable_hint, Finding, Severity};

use super::{path_segments, split_external_key};

struct Site<'a> {
    host: &'a str,
    source: &'a str,
    file: &'a str,
    line: u32,
}

/// Approximates a host's "registrable domain": strips a trailing `:port`, then compares the last two
/// dot-separated labels (`api.zoom.us` and `zoom.us` match; `oauth.pipedrive.com` and `api.dub.co` don't).
/// Hosts with fewer than 2 labels, or IPv4 literals, compare on the whole host instead. Not a
/// public-suffix-list lookup, so it under-strips two-label ccSLDs like `co.uk` — an accepted trade to avoid
/// over-flagging unrelated vendors on a shared path.
fn registrable_domain(host: &str) -> &str {
    let stripped = match host.rsplit_once(':') {
        Some((h, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => h,
        _ => host,
    };
    let labels: Vec<&str> = stripped.split('.').collect();
    let is_ip_literal = labels
        .iter()
        .all(|l| !l.is_empty() && l.bytes().all(|b| b.is_ascii_digit()));
    if labels.len() < 2 || is_ip_literal {
        return stripped;
    }
    let last = labels[labels.len() - 1];
    let second_last = labels[labels.len() - 2];
    let start = stripped.len() - second_last.len() - 1 - last.len();
    &stripped[start..]
}

pub fn external_base_url_drift_findings(external_consumes: &[TaggedConsume]) -> Vec<Finding> {
    let mut by_path: BTreeMap<&str, Vec<Site<'_>>> = BTreeMap::new();
    for c in external_consumes
        .iter()
        .filter(|c| c.consume.kind == "http" && !zzop_core::is_test_file(&c.consume.file))
    {
        let Some(key) = c.consume.key.as_deref() else {
            continue;
        };
        let Some(url) = split_external_key(key) else {
            continue;
        };
        if path_segments(url.path).len() < 2 {
            continue;
        }
        by_path.entry(url.path).or_default().push(Site {
            host: url.host,
            source: c.source.as_str(),
            file: c.consume.file.as_str(),
            line: c.consume.line,
        });
    }

    let mut out = Vec::new();
    for (path, mut sites) in by_path {
        let hosts: BTreeSet<&str> = sites.iter().map(|s| s.host).collect();
        if hosts.len() < 2 {
            continue;
        }
        let mut by_domain: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for host in &hosts {
            by_domain
                .entry(registrable_domain(host))
                .or_default()
                .insert(*host);
        }
        let related_hosts: BTreeSet<&str> = by_domain
            .values()
            .filter(|group| group.len() >= 2)
            .flatten()
            .copied()
            .collect();
        if related_hosts.len() < 2 {
            continue;
        }
        sites.retain(|s| related_hosts.contains(s.host));

        sites.sort_by(|a, b| {
            a.source
                .cmp(b.source)
                .then(a.file.cmp(b.file))
                .then(a.line.cmp(&b.line))
        });
        let first = &sites[0];
        let site_count = sites.len();
        let hosts_sorted: Vec<&str> = related_hosts.iter().copied().collect();

        let mut example_sites = Vec::new();
        for host in related_hosts.iter().take(4) {
            if let Some(site) = sites.iter().find(|s| s.host == *host) {
                example_sites.push(serde_json::json!({
                    "host": site.host,
                    "source": site.source,
                    "file": site.file,
                    "line": site.line,
                }));
            }
        }

        let message = format!(
            "path `{path}` is consumed as external egress against {} distinct hosts ({}) — {site_count} call \
             site(s) total, first at {}:{} (source `{}`, host `{}`). Since host strings here include port, this \
             also fires on a port-only difference between environments. This usually means one caller still \
             points at a different base URL/deployment than another (a config that drifted, or a hardcoded \
             host that was never updated everywhere). Verify whether all these hosts are supposed to be the \
             same logical service, and if so unify the base URL behind one config value. {} if this path \
             intentionally exists on multiple independent hosts (e.g. the same open API path shape offered \
             by unrelated vendors).",
            hosts_sorted.len(),
            hosts_sorted.join(", "),
            first.file,
            first.line,
            first.source,
            first.host,
            disable_hint("cross-layer/external-base-url-drift"),
        );
        out.push(Finding {
            rule_id: "cross-layer/external-base-url-drift".to_string(),
            severity: Severity::Info,
            file: first.file.to_string(),
            line: first.line,
            message,
            data: Some(serde_json::json!({
                "path": path,
                "hosts": hosts_sorted,
                "siteCount": site_count,
                "exampleSites": example_sites,
            })),
        });
    }
    out.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    out
}

#[cfg(test)]
mod tests;
