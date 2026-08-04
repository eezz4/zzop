//! Unit tests for `cross-layer/db-table-name-in-multiple-sources`. Split from the rule body 2026-07-29
//! when the per-source emission change pushed that file over the line-count ratchet (test files are exempt).

use super::*;
use zzop_core::io::{
    AmbiguousConsume, CrossLayerEdge, EdgeFrom, EdgeTo, TaggedConsume, TaggedProvide,
};
use zzop_core::IoConsume;

fn edge(kind: &str, key: &str, from_source: &str, file: &str, line: u32) -> CrossLayerEdge {
    CrossLayerEdge {
        kind: kind.to_string(),
        key: key.to_string(),
        from: EdgeFrom {
            source: from_source.to_string(),
            file: file.to_string(),
            line,
        },
        to: EdgeTo {
            source: "db".to_string(),
            file: "schema.sql".to_string(),
            line: 1,
            symbol: None,
        },
        cross_source: true,
        low_confidence_reason: None,
    }
}

fn unprovided_consume(kind: &str, key: &str, source: &str, file: &str, line: u32) -> TaggedConsume {
    TaggedConsume {
        source: source.to_string(),
        consume: IoConsume {
            client: None,
            body: None,
            kind: kind.to_string(),
            key: Some(key.to_string()),
            file: file.to_string(),
            line,
            raw: None,
            method: None,
            retry_configured: None,
        },
    }
}

/// ONE finding PER PARTICIPATING SOURCE, each anchored in that source's own first site — see the
/// module doc for why a single representative was wrong.
#[test]
fn same_table_consumed_by_two_edge_sources_is_flagged_in_each_of_them() {
    let cl = CrossLayerResult {
        edges: vec![
            edge("db-table", "table:users", "svc-a", "a.ts", 3),
            edge("db-table", "table:users", "svc-b", "b.ts", 9),
        ],
        ..Default::default()
    };
    let out = shared_db_table_findings(&cl);
    assert_eq!(out.len(), 2, "one per source, not one per table");
    let sites: Vec<(&str, u32)> = out.iter().map(|f| (f.file.as_str(), f.line)).collect();
    assert_eq!(sites, vec![("a.ts", 3), ("b.ts", 9)]);
    for f in &out {
        assert_eq!(f.rule_id, "cross-layer/db-table-name-in-multiple-sources");
        assert_eq!(f.severity, Severity::Warning);
        assert!(f.message.contains("svc-a"), "{}", f.message);
        assert!(f.message.contains("svc-b"), "{}", f.message);
        assert!(f.message.contains("Verify"), "{}", f.message);
        assert!(f.message.contains("disabledRules"), "{}", f.message);
    }
    // Each copy names ITS OWN tree as the subject, so a reader knows which side they are on.
    assert_eq!(out[0].data.as_ref().unwrap()["consumeSource"], "svc-a");
    assert_eq!(out[1].data.as_ref().unwrap()["consumeSource"], "svc-b");
    // …and still lists the whole set, so nothing is lost relative to the single-finding form.
    for f in &out {
        let sources = f.data.as_ref().unwrap()["sources"].as_array().unwrap();
        assert_eq!(sources.len(), 2);
    }
}

/// THE DEFECT THIS SHAPE EXISTS TO REMOVE: one tree excluding its own path used to delete the whole
/// finding, including the part that was about the OTHER trees. Modeled here the way `exclude` reaches
/// this rule — by dropping the findings anchored in one source and asserting the rest survive.
#[test]
fn excluding_one_source_leaves_the_other_sources_finding_standing() {
    let cl = CrossLayerResult {
        edges: vec![
            edge("db-table", "table:users", "svc-a", "a.ts", 3),
            edge("db-table", "table:users", "svc-b", "b.ts", 9),
            edge("db-table", "table:users", "svc-c", "c.ts", 1),
        ],
        ..Default::default()
    };
    let out = shared_db_table_findings(&cl);
    let survivors: Vec<&Finding> = out.iter().filter(|f| f.file != "a.ts").collect();
    assert_eq!(
        survivors.len(),
        2,
        "excluding svc-a must not silence svc-b and svc-c: {:?}",
        out.iter().map(|f| f.file.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn same_table_consumed_by_only_one_source_is_not_flagged() {
    let cl = CrossLayerResult {
        edges: vec![
            edge("db-table", "table:users", "svc-a", "a.ts", 3),
            edge("db-table", "table:users", "svc-a", "a2.ts", 5),
        ],
        ..Default::default()
    };
    assert!(shared_db_table_findings(&cl).is_empty());
}

#[test]
fn signal_combines_edges_ambiguous_and_dangling_consumes() {
    let cl = CrossLayerResult {
        edges: vec![edge("db-table", "table:orders", "svc-a", "a.ts", 1)],
        unprovided_consumes: vec![unprovided_consume(
            "db-table",
            "table:orders",
            "svc-b",
            "b.ts",
            2,
        )],
        ..Default::default()
    };
    let out = shared_db_table_findings(&cl);
    assert_eq!(out.len(), 2, "one per participating source");
    // The point of this test is the SIGNAL SET (an edge and an unprovided consume both count), so it
    // asserts both sources were seen — now via the per-source copies.
    let anchored: Vec<&str> = out.iter().map(|f| f.file.as_str()).collect();
    assert_eq!(anchored, vec!["a.ts", "b.ts"]);
    for f in &out {
        assert!(f.message.contains("svc-a"), "{}", f.message);
        assert!(f.message.contains("svc-b"), "{}", f.message);
    }
}

#[test]
fn ambiguous_consume_of_a_db_table_counts_toward_the_signal() {
    let cl = CrossLayerResult {
        edges: vec![edge("db-table", "table:orders", "svc-a", "a.ts", 1)],
        ambiguous_consumes: vec![AmbiguousConsume {
            source: "svc-c".to_string(),
            consume: IoConsume {
                client: None,
                body: None,
                kind: "db-table".to_string(),
                key: Some("table:orders".to_string()),
                file: "c.ts".to_string(),
                line: 4,
                raw: None,
                method: None,
                retry_configured: None,
            },
            candidates: vec![TaggedProvide {
                source: "db1".to_string(),
                provide: zzop_core::IoProvide {
                    response: None,
                    body: None,
                    kind: "db-table".to_string(),
                    key: "table:orders".to_string(),
                    file: "s1.sql".to_string(),
                    line: 1,
                    symbol: None,
                },
            }],
        }],
        ..Default::default()
    };
    let out = shared_db_table_findings(&cl);
    assert_eq!(out.len(), 2, "one per participating source");
    for f in &out {
        assert!(f.message.contains("svc-a"), "{}", f.message);
        assert!(f.message.contains("svc-c"), "{}", f.message);
    }
}

#[test]
fn non_db_table_kind_is_ignored() {
    let cl = CrossLayerResult {
        edges: vec![
            edge("http", "GET /x", "svc-a", "a.ts", 1),
            edge("http", "GET /x", "svc-b", "b.ts", 2),
        ],
        ..Default::default()
    };
    assert!(shared_db_table_findings(&cl).is_empty());
}

#[test]
fn duplicate_sites_are_deduped_before_counting() {
    let cl = CrossLayerResult {
        edges: vec![
            edge("db-table", "table:users", "svc-a", "a.ts", 3),
            edge("db-table", "table:users", "svc-a", "a.ts", 3),
            edge("db-table", "table:users", "svc-b", "b.ts", 9),
        ],
        ..Default::default()
    };
    let out = shared_db_table_findings(&cl);
    assert_eq!(
        out.len(),
        2,
        "one per source — svc-a's duplicate site is not a third"
    );
    for f in &out {
        let sources = f.data.as_ref().unwrap()["sources"].as_array().unwrap();
        assert_eq!(sources.len(), 2);
    }
}
