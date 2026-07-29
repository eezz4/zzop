//! `cross-layer/external-host-in-multiple-sources` (warning) — the same external host is called directly from
//! 2+ distinct source trees. Each tree likely built its own client for the same third-party integration —
//! duplicated auth/retry/failure-mode handling — so a vendor-side change (base URL, auth scheme) has to be
//! applied in multiple places instead of one. Anchored at the first site; the fix is to centralize behind
//! one client/backend proxy that every tree goes through instead of calling the vendor directly.
//!
//! **ONE FINDING PER CALLING SOURCE, each anchored in that source's own tree** (2026-07-29), following
//! `shared_db_table`'s precedent — read its module doc for the full argument. `exclude` applies to the
//! ANCHOR, so a single representative made WHICH tree could silence an N-tree fact an accident of sorting.
//! The gate here is literally "2+ distinct SOURCES", so the source IS the unit of the fact.
//!
//! Its co-firing sibling `external-host-fanout` deliberately does NOT do this: that rule counts distinct
//! FILES ("regardless of how many source trees own those files"), so splitting it per source would divide
//! one fanout below its own 3-file floor and delete the finding instead of re-anchoring it.
//!
//! Consume sites in test-path files (`zzop_core::is_test_file`) are skipped, including from the
//! per-host source-tree count — a test mocking a vendor API is not deployed egress.
//!
//! The id names the observation (one external host, 2+ sources), not the conclusion: this matcher never
//! inspects a client, so "each tree built its own client" stays a hypothesis the message states as one.
//! Renamed from `cross-layer/external-duplicated-integration`; the old id is recorded in `VERSIONING.md`.

use std::collections::{BTreeMap, BTreeSet};

use zzop_core::io::TaggedConsume;
use zzop_core::{disable_hint, Finding, Severity};

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
        let site_count = sites.len();
        let example_sites: Vec<_> = sites
            .iter()
            .take(3)
            .map(|s| serde_json::json!({"source": s.source, "file": s.file, "line": s.line}))
            .collect();
        let sources_sorted: Vec<&str> = sources.into_iter().collect();

        // ONE COPY PER CALLING SOURCE, each anchored in its own tree — see the module doc. `sites` is
        // already `(source, file, line)`-sorted, so each source's first match is deterministic.
        for source in &sources_sorted {
            let Some(first) = sites.iter().find(|s| &s.source == source) else {
                continue;
            };
            let others: Vec<&str> = sources_sorted
                .iter()
                .copied()
                .filter(|s| s != source)
                .collect();
            let message = format!(
                "external host `{host}` is called directly from this source (`{source}`, first at {}:{}) \
                 and from {} other analyzed source(s) ({}) — {site_count} call site(s) in total. Each \
                 source likely built its own client for the same third-party integration, duplicating \
                 auth/retry/failure-mode handling and multiplying the places a vendor-side change (base \
                 URL, auth scheme) has to be applied. Centralize this integration behind one \
                 client/backend proxy that every source calls through instead of hitting the vendor \
                 directly from each source. Each calling source gets its own copy of this finding, \
                 anchored in its own tree, so excluding one source's paths never silences the others. {} \
                 if these sources are intentionally independent deployments that must not share a runtime \
                 dependency on one proxy.",
                first.file,
                first.line,
                others.len(),
                others.join(", "),
                disable_hint("cross-layer/external-host-in-multiple-sources"),
            );
            out.push(Finding {
                rule_id: "cross-layer/external-host-in-multiple-sources".to_string(),
                severity: Severity::Warning,
                file: first.file.to_string(),
                line: first.line,
                message,
                // Every other call site this copy prints (via `exampleSites`). Deduped and sorted.
                evidence_paths: sites
                    .iter()
                    .map(|s| s.file)
                    .filter(|f| *f != first.file)
                    .map(str::to_string)
                    .collect::<std::collections::BTreeSet<String>>()
                    .into_iter()
                    .collect(),
                data: Some(serde_json::json!({
                    "host": host,
                    "consumeSource": source,
                    "sources": sources_sorted,
                    "siteCount": site_count,
                    "exampleSites": example_sites,
                })),
            });
        }
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
                client: None,
                body: None,
                kind: "http".to_string(),
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
        // One copy PER CALLING SOURCE, each anchored in its own tree (2026-07-29) — see the module doc.
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].file, "Client.java");
        assert_eq!(out[0].line, 5);
        for f in &out {
            assert_eq!(f.rule_id, "cross-layer/external-host-in-multiple-sources");
            assert_eq!(f.severity, Severity::Warning);
            assert!(f.message.contains("api.vendor.com"), "{}", f.message);
            assert!(f.message.contains("disabled_rules"), "{}", f.message);
            let data = f.data.as_ref().unwrap();
            assert_eq!(data["host"], "api.vendor.com");
            assert_eq!(data["sources"], serde_json::json!(["be", "fe"]));
            assert_eq!(data["siteCount"], 2);
        }
        // Each copy names its own tree — that is the whole point of the change.
        assert_eq!(out[0].data.as_ref().unwrap()["consumeSource"], "be");
        assert_eq!(out[1].data.as_ref().unwrap()["consumeSource"], "fe");
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
        // Two hosts x two calling sources each = four copies, still ordered by (file, line).
        assert_eq!(out.len(), 4);
        let files: Vec<&str> = out.iter().map(|f| f.file.as_str()).collect();
        assert_eq!(files, vec!["A.java", "B.java", "M.tsx", "Z.tsx"]);
    }
}
