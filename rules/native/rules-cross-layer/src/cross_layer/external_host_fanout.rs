//! `cross-layer/external-host-fanout` (info) — the same external host is called directly from 3+ distinct
//! files, regardless of how many source trees own those files. Calling a host from many places instead of
//! one shared client duplicates retry/auth/error-handling per call site and removes any single choke point
//! for caching, circuit-breaking, or a base-URL change. Anchored at the first site.
//!
//! Co-fires with `cross-layer/external-host-in-multiple-sources` when a host is both multi-file and
//! multi-source; the remedies differ (extract one client module vs. pick one source to own the call).
//!
//! Test-path consume sites (`zzop_core::is_test_file`) are skipped, including from the file-count
//! threshold — a test mocking a vendor API is not deployed egress. Counts are a lower bound: only consume
//! keys the join extracted directly at the call site are counted, so a host a call site does not spell out
//! can leave calling files uncounted even though they reach the host at runtime — hence "at least N
//! distinct files" rather than an exact count. The TS extractor narrowed that gap but did not close it:
//! `same-file-fn-url-v1` now reads a zero-argument SAME-FILE helper whose whole body is one
//! `return <literal>` (`fetch(chargesUrl())`, `` `${base()}/charges` ``), while a helper that takes an
//! argument, branches, is `async`, or is imported from another module still leaves its callers uncounted —
//! as does every host reached through a client the egress extractor does not recognize at all.
//!
//! ## File identity is `(source, file)`, not `file`
//! `IoConsume::file` is TREE-RELATIVE, so two analyzed trees routinely carry the same relative path
//! (`src/api.ts` in both a frontend and a backend). Counting distinct `file` alone folded those into one,
//! under-counting the fanout and silently vetoing the finding for a host genuinely called from N trees at
//! one shared path shape — the opposite of this rule's own "regardless of how many source trees own those
//! files" premise. The count and the emitted examples both key on `(source, file)`.

use std::collections::{BTreeMap, BTreeSet};

use zzop_core::io::TaggedConsume;
use zzop_core::{disable_hint, Finding, Severity};

use super::split_external_key;

/// Fanout threshold: 3 distinct files calling the same external host directly. Two files sharing a host is
/// common and unremarkable; three or more is where inlining the call stops scaling and a shared client
/// module starts paying for itself.
///
/// Do not unify or pin (policy inventory T3 — coincidental equality): S1's `MIN_FILES` (engine
/// framework_silence) and `SEAMS_MIN_FILES` (zzop-metrics) are also 3, and so is the pinned
/// `MIN_PREFIX_DRIFT_GROUP`/`MIN_FOREIGN_UNPROVIDED_GROUP` fold-threshold pair — but this 3 is a
/// remedy-economics judgment (the fanout at which a shared client module pays for itself), not a
/// disclosure evidence floor, a metric eligibility floor, or the "2 is coincidence, 3+ is a pattern"
/// same-cause fold policy. It may move for noise tuning (e.g. large monorepos where several files
/// legitimately touch one host) without re-justifying any of those.
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
        // `(source, file)`, not `file` — see the module doc's file-identity note.
        let files: BTreeSet<(&str, &str)> = sites.iter().map(|s| (s.source, s.file)).collect();
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
        // Sibling shape (`external-host-in-multiple-sources`'s `exampleSites`): one object per example
        // carrying its OWN tree, not a bare path, so a consumer can key each as `<source>/<file>:<line>`
        // without guessing. Taken as the FIRST site of each distinct `(source, file)` walking the already
        // `(source, file, line)`-sorted `sites`, which is the same order `files` enumerates in — so
        // `exampleSites[0]` is the finding's anchor by construction, not by coincidence.
        let mut example_sites: Vec<serde_json::Value> = Vec::new();
        let mut example_desc: Vec<String> = Vec::new();
        let mut seen: BTreeSet<(&str, &str)> = BTreeSet::new();
        for s in &sites {
            // Same literal display cap the pre-attribution `take(5)` used — a message-length budget, not
            // a policy threshold, so it stays inline rather than becoming a named/pinned constant.
            if example_sites.len() >= 5 {
                break;
            }
            if !seen.insert((s.source, s.file)) {
                continue;
            }
            example_sites
                .push(serde_json::json!({"source": s.source, "file": s.file, "line": s.line}));
            example_desc.push(format!("{} (source `{}`)", s.file, s.source));
        }
        // Compared on the WHOLE `(source, file, line)` key the sort above orders by, not on `file` alone.
        // A tree-relative path is not an identity (the module doc's file-identity note), so a `file`-only
        // equality is blind to precisely the confusion this assertion exists to catch — `exampleSites[0]`
        // carrying the same path from a DIFFERENT source than the anchor, which is the shape that made
        // `(source, file)` the count key in the first place.
        debug_assert_eq!(
            example_sites.first().map(|e| (
                e["source"].as_str(),
                e["file"].as_str(),
                e["line"].as_u64()
            )),
            Some((
                Some(first.source),
                Some(first.file),
                Some(u64::from(first.line))
            )),
            "exampleSites[0] must be the finding's own anchor"
        );

        let message = format!(
            "external host `{host}` is called directly from at least {file_count} distinct files ({site_count} call \
             site(s) total), e.g. {}, first at {}:{} (source `{}`). Calling a third-party host from this many \
             places instead of one shared client module duplicates retry/auth/error-handling per call site \
             and leaves no single choke point for caching, circuit-breaking, or a base-URL change. Extract \
             one client module for this host and route every call through it. (This can co-fire with \
             `cross-layer/external-host-in-multiple-sources` when the fanout also spans multiple sources — \
             that is a different remedy: pick one source to own the integration.) {} if this host is \
             intentionally called from many independent call sites (e.g. a generic HTTP utility used ad hoc \
             across the codebase with no shared per-vendor logic to extract).",
            example_desc.join(", "),
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
                // The ANCHOR's tree. Without it a consumer keying `<source>/<file>:<line>` folded two
                // trees' identical relative paths onto one key. Additive; the message already said it.
                "source": first.source,
                "fileCount": file_count,
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
        // Seals the anchor attribution: without `source`, `A.tsx:1` is not a unique key across trees.
        assert_eq!(data["source"], "fe");
        assert_eq!(
            data["exampleSites"],
            serde_json::json!([
                {"source": "fe", "file": "A.tsx", "line": 1},
                {"source": "fe", "file": "B.tsx", "line": 2},
                {"source": "fe", "file": "C.tsx", "line": 3},
            ])
        );
    }

    /// Seals the file-identity fix: three trees sharing one relative path are three distinct files, so
    /// the fanout fires. Keyed on `file` alone they folded into one and the rule stayed silent.
    #[test]
    fn the_same_relative_path_in_three_trees_counts_as_three_distinct_files() {
        let external = vec![
            consume(Some("GET https://api.vendor.com/a"), "xfe", "src/api.ts", 7),
            consume(Some("GET https://api.vendor.com/b"), "xbe", "src/api.ts", 7),
            consume(
                Some("GET https://api.vendor.com/c"),
                "xbe2",
                "src/api.ts",
                7,
            ),
        ];
        let out = external_host_fanout_findings(&external);
        assert_eq!(out.len(), 1);
        let data = out[0].data.as_ref().unwrap();
        assert_eq!(data["fileCount"], 3);
        assert_eq!(data["source"], "xbe");
        assert_eq!(data["exampleSites"][0]["source"], "xbe");
    }

    /// The other half of the same identity rule: two call sites in ONE tree's one file are one file, so
    /// the `(source, file)` key must not inflate a within-tree repeat into fanout.
    #[test]
    fn two_call_sites_in_one_file_of_one_tree_stay_one_file() {
        let external = vec![
            consume(Some("GET https://api.vendor.com/a"), "fe", "src/api.ts", 1),
            consume(Some("GET https://api.vendor.com/b"), "fe", "src/api.ts", 2),
            consume(Some("GET https://api.vendor.com/c"), "fe", "src/api.ts", 3),
        ];
        assert!(external_host_fanout_findings(&external).is_empty());
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
