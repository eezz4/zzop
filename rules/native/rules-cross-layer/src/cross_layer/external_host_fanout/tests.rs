//! Unit tests for `cross-layer/external-host-fanout`. Moved out of the rule file on 2026-07-29 (line
//! cap) — same tests, same order, no behaviour change.

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
    assert!(out[0].message.contains("disabledRules"));
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
