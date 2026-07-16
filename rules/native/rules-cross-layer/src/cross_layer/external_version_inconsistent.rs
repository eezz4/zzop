//! `cross-layer/external-version-inconsistent` (info) — one external host consumed through BOTH
//! version-shaped paths (`/v1/...`, `/v2/...`) and versionless paths (`/users`, ...). A bare root pins no
//! version, so it's dropped from the versionless side first. Classification uses the shared
//! [`super::VERSION_SEGMENT_PATTERN`] (same pattern `version_skew` uses); findings are anchored at the
//! first versionless consume site, sorted by `(file, line)`.
//!
//! Consume sites in test-path files (`zzop_core::is_test_file`) are skipped, including from the
//! per-host counting — a test mocking a vendor/own API is not deployed egress.
//!
//! ## Message framing
//! The path split is a real signal but not a verdict: some vendor hosts genuinely serve versioned and
//! versionless paths as DISTINCT documented endpoint families rather than one API inconsistently pinned.
//! The message presents both readings neutrally rather than asserting drift as the default explanation.

use std::collections::{BTreeMap, BTreeSet};

use regex::Regex;

use zzop_core::io::TaggedConsume;
use zzop_core::{disable_hint, Finding, Severity};

use super::{path_segments, split_external_key, VERSION_SEGMENT_PATTERN};

pub fn external_version_inconsistent_findings(external_consumes: &[TaggedConsume]) -> Vec<Finding> {
    let version_re = Regex::new(VERSION_SEGMENT_PATTERN).unwrap();

    let mut by_host: BTreeMap<String, Vec<(String, &TaggedConsume)>> = BTreeMap::new();
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
             {} if this host legitimately serves both an unversioned default endpoint and explicit versioned \
             endpoints side by side on purpose.",
            versioned_examples.first().copied().unwrap_or(""),
            versionless_examples.first().copied().unwrap_or(""),
            versioned.len(),
            versionless.len(),
            sources.len(),
            anchor.consume.file,
            anchor.consume.line,
            anchor.source,
            disable_hint("cross-layer/external-version-inconsistent"),
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
mod tests;
