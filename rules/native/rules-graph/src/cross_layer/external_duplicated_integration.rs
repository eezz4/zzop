//! `cross-layer/external-duplicated-integration` (warning) — the same external host is called directly from
//! 2+ distinct source trees. Each tree likely built its own client for the same third-party integration —
//! duplicated auth/retry/failure-mode handling — so a vendor-side change (base URL, auth scheme) has to be
//! applied in multiple places instead of one. Anchored at the first site; the fix is to centralize behind
//! one client/backend proxy that every tree goes through instead of calling the vendor directly.
//!
//! Consume sites in test-path files (`crate::unreachable::is_test_file`) are skipped, including from the
//! per-host source-tree count — a test mocking a vendor API is not deployed egress.

use std::collections::{BTreeMap, BTreeSet};

use zzop_core::io::TaggedConsume;
use zzop_core::{Finding, Severity};

use super::split_external_key;

struct Site<'a> {
    source: &'a str,
    file: &'a str,
    line: u32,
}

pub fn external_duplicated_integration_findings(
    external_consumes: &[TaggedConsume],
) -> Vec<Finding> {
    let mut by_host: BTreeMap<&str, Vec<Site<'_>>> = BTreeMap::new();
    for c in external_consumes
        .iter()
        .filter(|c| c.consume.kind == "http" && !crate::unreachable::is_test_file(&c.consume.file))
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
        let sources: BTreeSet<&str> = sites.iter().map(|s| s.source).collect();
        if sources.len() < 2 {
            continue;
        }
        sites.sort_by(|a, b| {
            a.source
                .cmp(b.source)
                .then(a.file.cmp(b.file))
                .then(a.line.cmp(&b.line))
        });
        let first = &sites[0];
        let site_count = sites.len();
        let example_sites: Vec<_> = sites
            .iter()
            .take(3)
            .map(|s| serde_json::json!({"source": s.source, "file": s.file, "line": s.line}))
            .collect();
        let sources_sorted: Vec<&str> = sources.into_iter().collect();

        let message = format!(
            "external host `{host}` is called directly from {} distinct sources ({}) — {site_count} \
             call site(s) in total, e.g. {}:{} (source `{}`). Each source likely built its own client for the \
             same third-party integration, duplicating auth/retry/failure-mode handling and multiplying the \
             places a vendor-side change (base URL, auth scheme) has to be applied. Centralize this \
             integration behind one client/backend proxy that every source calls through instead of hitting the \
             vendor directly from each source. Disable via rule config `disabled_rules: \
             [\"cross-layer/external-duplicated-integration\"]` if these sources are intentionally independent \
             deployments that must not share a runtime dependency on one proxy.",
            sources_sorted.len(),
            sources_sorted.join(", "),
            first.file,
            first.line,
            first.source,
        );
        out.push(Finding {
            rule_id: "cross-layer/external-duplicated-integration".to_string(),
            severity: Severity::Warning,
            file: first.file.to_string(),
            line: first.line,
            message,
            data: Some(serde_json::json!({
                "host": host,
                "sources": sources_sorted,
                "siteCount": site_count,
                "exampleSites": example_sites,
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
    fn same_host_from_two_trees_is_flagged() {
        let external = vec![
            consume(
                Some("GET https://api.vendor.com/v1/widgets"),
                "fe",
                "Ctx.tsx",
                10,
            ),
            consume(
                Some("POST https://api.vendor.com/v1/orders"),
                "be",
                "Client.java",
                5,
            ),
        ];
        let out = external_duplicated_integration_findings(&external);
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].rule_id,
            "cross-layer/external-duplicated-integration"
        );
        assert_eq!(out[0].severity, Severity::Warning);
        assert_eq!(out[0].file, "Client.java");
        assert_eq!(out[0].line, 5);
        assert!(out[0].message.contains("api.vendor.com"));
        assert!(out[0].message.contains("disabled_rules"));
        let data = out[0].data.as_ref().unwrap();
        assert_eq!(data["host"], "api.vendor.com");
        assert_eq!(data["sources"], serde_json::json!(["be", "fe"]));
        assert_eq!(data["siteCount"], 2);
    }

    #[test]
    fn test_fixture_consume_does_not_count_toward_the_source_tree_threshold() {
        let external = vec![
            consume(
                Some("GET https://api.vendor.com/v1/widgets"),
                "fe",
                "src/__tests__/Ctx.test.tsx",
                10,
            ),
            consume(
                Some("POST https://api.vendor.com/v1/orders"),
                "be",
                "Client.java",
                5,
            ),
        ];
        assert!(external_duplicated_integration_findings(&external).is_empty());
    }

    #[test]
    fn same_host_from_one_tree_only_is_not_flagged() {
        let external = vec![
            consume(
                Some("GET https://api.vendor.com/v1/widgets"),
                "fe",
                "A.tsx",
                10,
            ),
            consume(
                Some("POST https://api.vendor.com/v1/orders"),
                "fe",
                "B.tsx",
                5,
            ),
        ];
        assert!(external_duplicated_integration_findings(&external).is_empty());
    }

    #[test]
    fn determinism_multiple_hosts_sorted_by_file_then_line() {
        let external = vec![
            consume(Some("GET https://z.vendor.com/a"), "fe", "Z.tsx", 1),
            consume(Some("GET https://z.vendor.com/a"), "be", "A.java", 9),
            consume(Some("GET https://a.vendor.com/a"), "fe", "M.tsx", 1),
            consume(Some("GET https://a.vendor.com/a"), "be", "B.java", 2),
        ];
        let out = external_duplicated_integration_findings(&external);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].file, "A.java");
        assert_eq!(out[1].file, "B.java");
    }
}
