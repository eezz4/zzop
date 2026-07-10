//! `cross-layer/external-host-fanout` (info) — the same external host is called directly from 3+ distinct
//! files, regardless of how many source trees own those files. Calling a host from many places instead of
//! one shared client duplicates retry/auth/error-handling per call site and removes any single choke point
//! for caching, circuit-breaking, or a base-URL change. Anchored at the first site.
//!
//! Co-fires with `cross-layer/external-duplicated-integration` when a host is both multi-file and
//! multi-source; the remedies differ (extract one client module vs. pick one source to own the call).
//!
//! Test-path consume sites (`zzop_core::is_test_file`) are skipped, including from the file-count
//! threshold — a test mocking a vendor API is not deployed egress. Counts are a lower bound: only consume
//! keys the join extracted directly at the call site are counted, so a host literal returned from a helper
//! function can leave calling files uncounted even though they reach the host at runtime — hence "at least N
//! distinct files" rather than an exact count.

use std::collections::{BTreeMap, BTreeSet};

use zzop_core::io::TaggedConsume;
use zzop_core::{disable_hint, Finding, Severity};

use super::split_external_key;

/// Fanout threshold: 3 distinct files calling the same external host directly. Two files sharing a host is
/// common and unremarkable; three or more is where inlining the call stops scaling and a shared client
/// module starts paying for itself.
const FANOUT_MIN_FILES: usize = 3;

struct Site<'a> {
    source: &'a str,
    file: &'a str,
    line: u32,
}

pub fn external_host_fanout_findings(external_consumes: &[TaggedConsume]) -> Vec<Finding> {
    let mut by_host: BTreeMap<&str, Vec<Site<'_>>> = BTreeMap::new();
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
        by_host.entry(url.host).or_default().push(Site {
            source: c.source.as_str(),
            file: c.consume.file.as_str(),
            line: c.consume.line,
        });
    }

    let mut out = Vec::new();
    for (host, mut sites) in by_host {
        let files: BTreeSet<&str> = sites.iter().map(|s| s.file).collect();
        if files.len() < FANOUT_MIN_FILES {
            continue;
        }
        sites.sort_by(|a, b| {
            a.source
                .cmp(b.source)
                .then(a.file.cmp(b.file))
                .then(a.line.cmp(&b.line))
        });
        let first = &sites[0];
        let file_count = files.len();
        let site_count = sites.len();
        let example_files: Vec<&str> = files.iter().copied().take(5).collect();

        let message = format!(
            "external host `{host}` is called directly from at least {file_count} distinct files ({site_count} call \
             site(s) total), e.g. {}, first at {}:{} (source `{}`). Calling a third-party host from this many \
             places instead of one shared client module duplicates retry/auth/error-handling per call site \
             and leaves no single choke point for caching, circuit-breaking, or a base-URL change. Extract \
             one client module for this host and route every call through it. (This can co-fire with \
             `cross-layer/external-duplicated-integration` when the fanout also spans multiple sources — \
             that is a different remedy: pick one source to own the integration.) {} if this host is \
             intentionally called from many independent call sites (e.g. a generic HTTP utility used ad hoc \
             across the codebase with no shared per-vendor logic to extract).",
            example_files.join(", "),
            first.file,
            first.line,
            first.source,
            disable_hint("cross-layer/external-host-fanout"),
        );
        out.push(Finding {
            rule_id: "cross-layer/external-host-fanout".to_string(),
            severity: Severity::Info,
            file: first.file.to_string(),
            line: first.line,
            message,
            data: Some(serde_json::json!({
                "host": host,
                "fileCount": file_count,
                "siteCount": site_count,
                "exampleFiles": example_files,
            })),
        });
    }
    out.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn consume(key: Option<&str>, source: &str, file: &str, line: u32) -> TaggedConsume {
        TaggedConsume {
            source: source.to_string(),
            consume: zzop_core::IoConsume {
                kind: "http".to_string(),
                key: key.map(str::to_string),
                file: file.to_string(),
                line,
                raw: None,
                method: None,
            },
        }
    }

    #[test]
    fn host_called_from_three_files_is_flagged() {
        let external = vec![
            consume(Some("GET https://api.vendor.com/a"), "fe", "A.tsx", 1),
            consume(Some("GET https://api.vendor.com/b"), "fe", "B.tsx", 2),
            consume(Some("GET https://api.vendor.com/c"), "fe", "C.tsx", 3),
        ];
        let out = external_host_fanout_findings(&external);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "cross-layer/external-host-fanout");
        assert_eq!(out[0].severity, Severity::Info);
        assert_eq!(out[0].file, "A.tsx");
        assert_eq!(out[0].line, 1);
        assert!(out[0].message.contains("api.vendor.com"));
        assert!(out[0].message.contains("disabled_rules"));
        let data = out[0].data.as_ref().unwrap();
        assert_eq!(data["fileCount"], 3);
        assert_eq!(data["siteCount"], 3);
    }

    #[test]
    fn test_fixture_file_does_not_count_toward_the_fanout_threshold() {
        let external = vec![
            consume(Some("GET https://api.vendor.com/a"), "fe", "A.tsx", 1),
            consume(Some("GET https://api.vendor.com/b"), "fe", "B.tsx", 2),
            consume(
                Some("GET https://api.vendor.com/c"),
                "fe",
                "src/__tests__/C.test.tsx",
                3,
            ),
        ];
        assert!(external_host_fanout_findings(&external).is_empty());
    }

    #[test]
    fn host_called_from_exactly_two_files_is_not_flagged() {
        let external = vec![
            consume(Some("GET https://api.vendor.com/a"), "fe", "A.tsx", 1),
            consume(Some("GET https://api.vendor.com/b"), "fe", "B.tsx", 2),
        ];
        assert!(external_host_fanout_findings(&external).is_empty());
    }

    #[test]
    fn determinism_multiple_findings_sorted_by_file_then_line() {
        let external = vec![
            consume(Some("GET https://z.vendor.com/a"), "fe", "Z1.tsx", 1),
            consume(Some("GET https://z.vendor.com/b"), "fe", "Z2.tsx", 2),
            consume(Some("GET https://z.vendor.com/c"), "fe", "Z3.tsx", 3),
            consume(Some("GET https://a.vendor.com/a"), "fe", "M1.tsx", 1),
            consume(Some("GET https://a.vendor.com/b"), "fe", "M2.tsx", 2),
            consume(Some("GET https://a.vendor.com/c"), "fe", "M3.tsx", 3),
        ];
        let out = external_host_fanout_findings(&external);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].file, "M1.tsx");
        assert_eq!(out[1].file, "Z1.tsx");
    }
}
