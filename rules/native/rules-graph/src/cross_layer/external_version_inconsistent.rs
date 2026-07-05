//! `cross-layer/external-version-inconsistent` (info) — one external host consumed through BOTH
//! version-shaped paths (`/v1/...`, `/v2/...`) and versionless paths (`/users`, ...). A bare root pins no
//! version, so it's dropped from the versionless side first. Classification uses the shared
//! [`super::VERSION_SEGMENT_PATTERN`] (same pattern `version_skew` uses); findings are anchored at the
//! first versionless consume site, sorted by `(file, line)`.
//!
//! Consume sites in test-path files (`crate::unreachable::is_test_file`) are skipped, including from the
//! per-host counting — a test mocking a vendor/own API is not deployed egress.
//!
//! ## Message framing
//! The path split is a real signal but not a verdict: some vendor hosts genuinely serve versioned and
//! versionless paths as DISTINCT documented endpoint families rather than one API inconsistently pinned.
//! The message presents both readings neutrally rather than asserting drift as the default explanation.

use std::collections::{BTreeMap, BTreeSet};

use regex::Regex;

use zpz_core::io::TaggedConsume;
use zpz_core::{Finding, Severity};

use super::{path_segments, split_external_key, VERSION_SEGMENT_PATTERN};

pub fn external_version_inconsistent_findings(external_consumes: &[TaggedConsume]) -> Vec<Finding> {
    let version_re = Regex::new(VERSION_SEGMENT_PATTERN).unwrap();

    let mut by_host: BTreeMap<String, Vec<(String, &TaggedConsume)>> = BTreeMap::new();
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
        by_host
            .entry(url.host.to_string())
            .or_default()
            .push((url.path.to_string(), c));
    }

    let mut out = Vec::new();
    for (host, entries) in &by_host {
        let mut versioned: BTreeSet<&str> = BTreeSet::new();
        let mut versionless: BTreeSet<&str> = BTreeSet::new();
        for entry in entries {
            let path = entry.0.as_str();
            if path == "/" {
                continue; // a root call pins no version — drop it from the versionless side.
            }
            let is_versioned = path_segments(path)
                .into_iter()
                .any(|seg| version_re.is_match(seg));
            if is_versioned {
                versioned.insert(path);
            } else {
                versionless.insert(path);
            }
        }
        if versioned.is_empty() || versionless.is_empty() {
            continue;
        }

        let mut versionless_sites: Vec<&TaggedConsume> = Vec::new();
        let mut sources: Vec<&str> = Vec::new();
        for entry in entries {
            let path = entry.0.as_str();
            let c = entry.1;
            if versioned.contains(path) || versionless.contains(path) {
                sources.push(c.source.as_str());
            }
            if versionless.contains(path) {
                versionless_sites.push(c);
            }
        }
        sources.sort();
        sources.dedup();
        versionless_sites.sort_by(|a, b| {
            a.consume
                .file
                .cmp(&b.consume.file)
                .then(a.consume.line.cmp(&b.consume.line))
        });
        let anchor = versionless_sites[0];

        let versioned_examples: Vec<&str> = versioned.iter().take(3).copied().collect();
        let versionless_examples: Vec<&str> = versionless.iter().take(3).copied().collect();

        let message = format!(
            "external host `{host}` is called through both version-pinned paths (e.g. `{}`) and versionless \
             paths (e.g. `{}`) — {} versioned path(s) vs {} versionless path(s) across {} caller(s), anchored \
             at {}:{} (source `{}`). This has two equally plausible readings: either the versionless calls \
             were never migrated onto an explicit version and silently ride whatever `{host}` currently \
             treats as default/latest (inconsistent pinning against one API), or the versionless paths are a \
             genuinely distinct endpoint family that `{host}` documents and versions separately from the \
             `/v*` family (not drift at all). Check `{host}`'s API docs for the versionless paths before \
             changing anything, and only unify the calls if the docs confirm they're the same API surface. \
             Disable via rule config `disabled_rules: [\"cross-layer/external-version-inconsistent\"]` if \
             this host legitimately serves both an unversioned default endpoint and explicit versioned \
             endpoints side by side on purpose.",
            versioned_examples.first().copied().unwrap_or(""),
            versionless_examples.first().copied().unwrap_or(""),
            versioned.len(),
            versionless.len(),
            sources.len(),
            anchor.consume.file,
            anchor.consume.line,
            anchor.source,
        );

        out.push(Finding {
            rule_id: "cross-layer/external-version-inconsistent".to_string(),
            severity: Severity::Info,
            file: anchor.consume.file.clone(),
            line: anchor.consume.line,
            message,
            data: Some(serde_json::json!({
                "host": host,
                "versionedPathCount": versioned.len(),
                "versionlessPathCount": versionless.len(),
                "versionedPathExamples": versioned_examples,
                "versionlessPathExamples": versionless_examples,
                "consumeSources": sources,
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
    fn versioned_and_versionless_paths_on_the_same_host_are_flagged_anchored_at_versionless() {
        let external = vec![
            consume(
                "http",
                Some("GET https://api.vendor.com/v1/users"),
                "fe",
                "Users.ts",
                40,
            ),
            consume(
                "http",
                Some("GET https://api.vendor.com/accounts"),
                "fe",
                "Accounts.ts",
                7,
            ),
        ];
        let out = external_version_inconsistent_findings(&external);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "cross-layer/external-version-inconsistent");
        assert_eq!(out[0].severity, Severity::Info);
        assert_eq!(out[0].file, "Accounts.ts");
        assert_eq!(out[0].line, 7);
        assert!(out[0].message.contains("api.vendor.com"));
        assert!(out[0].message.contains("disabled_rules"));
        assert!(out[0].message.contains("equally plausible readings"));
        assert!(out[0]
            .message
            .contains("genuinely distinct endpoint family"));
        assert!(out[0].message.contains("Check `api.vendor.com`'s API docs"));
        let data = out[0].data.as_ref().unwrap();
        assert_eq!(data["versionedPathCount"], 1);
        assert_eq!(data["versionlessPathCount"], 1);
    }

    #[test]
    fn only_versioned_paths_on_a_host_is_not_flagged() {
        let external = vec![
            consume(
                "http",
                Some("GET https://api.vendor.com/v1/users"),
                "fe",
                "Users.ts",
                40,
            ),
            consume(
                "http",
                Some("GET https://api.vendor.com/v2/accounts"),
                "fe",
                "Accounts.ts",
                7,
            ),
        ];
        assert!(external_version_inconsistent_findings(&external).is_empty());
    }

    #[test]
    fn only_versionless_paths_on_a_host_is_not_flagged() {
        let external = vec![
            consume(
                "http",
                Some("GET https://api.vendor.com/users"),
                "fe",
                "Users.ts",
                40,
            ),
            consume(
                "http",
                Some("GET https://api.vendor.com/accounts"),
                "fe",
                "Accounts.ts",
                7,
            ),
        ];
        assert!(external_version_inconsistent_findings(&external).is_empty());
    }

    #[test]
    fn bare_root_path_is_dropped_from_the_versionless_side() {
        // Only a "/" consume plus a versioned one — root pins nothing, so this must not fire.
        let external = vec![
            consume(
                "http",
                Some("GET https://api.vendor.com/v1/users"),
                "fe",
                "Users.ts",
                40,
            ),
            consume(
                "http",
                Some("GET https://api.vendor.com/"),
                "fe",
                "Root.ts",
                1,
            ),
        ];
        assert!(external_version_inconsistent_findings(&external).is_empty());
    }

    #[test]
    fn each_host_is_classified_independently() {
        // host A: versioned only, host B: versionless only — neither alone qualifies.
        let external = vec![
            consume(
                "http",
                Some("GET https://a.vendor.com/v1/users"),
                "fe",
                "A.ts",
                1,
            ),
            consume(
                "http",
                Some("GET https://b.vendor.com/users"),
                "fe",
                "B.ts",
                1,
            ),
        ];
        assert!(external_version_inconsistent_findings(&external).is_empty());
    }

    #[test]
    fn consume_in_a_test_fixture_file_does_not_count_toward_the_host_classification() {
        // Versionless consume is only in a test fixture — it must not count toward "both" territory.
        let external = vec![
            consume(
                "http",
                Some("GET https://api.vendor.com/v1/users"),
                "fe",
                "Users.ts",
                40,
            ),
            consume(
                "http",
                Some("GET https://api.vendor.com/accounts"),
                "fe",
                "src/__tests__/Accounts.test.ts",
                7,
            ),
        ];
        assert!(external_version_inconsistent_findings(&external).is_empty());
    }

    #[test]
    fn non_http_kind_is_ignored() {
        let external = vec![
            consume(
                "queue",
                Some("GET https://api.vendor.com/v1/users"),
                "fe",
                "Users.ts",
                40,
            ),
            consume(
                "queue",
                Some("GET https://api.vendor.com/accounts"),
                "fe",
                "Accounts.ts",
                7,
            ),
        ];
        assert!(external_version_inconsistent_findings(&external).is_empty());
    }

    #[test]
    fn findings_are_sorted_deterministically_by_file_then_line() {
        let external = vec![
            // host b: fires, anchored at B-versionless.ts:9
            consume(
                "http",
                Some("GET https://b.vendor.com/v1/x"),
                "fe",
                "B-versioned.ts",
                2,
            ),
            consume(
                "http",
                Some("GET https://b.vendor.com/x"),
                "fe",
                "B-versionless.ts",
                9,
            ),
            // host a: fires, anchored at A-versionless.ts:3
            consume(
                "http",
                Some("GET https://a.vendor.com/v1/y"),
                "fe",
                "A-versioned.ts",
                50,
            ),
            consume(
                "http",
                Some("GET https://a.vendor.com/y"),
                "fe",
                "A-versionless.ts",
                3,
            ),
        ];
        let out = external_version_inconsistent_findings(&external);
        assert_eq!(out.len(), 2);
        assert_eq!((out[0].file.as_str(), out[0].line), ("A-versionless.ts", 3));
        assert_eq!((out[1].file.as_str(), out[1].line), ("B-versionless.ts", 9));
    }
}
