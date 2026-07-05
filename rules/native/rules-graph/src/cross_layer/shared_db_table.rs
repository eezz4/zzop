//! `cross-layer/shared-db-table` (warning) — the same `db-table` key CONSUMED by 2+ distinct sources.
//! Consumes are the signal; provides don't matter here (unlike `duplicate_route`, which is about who
//! PROVIDES a key) — a table is "shared" when multiple sources read/write it, regardless of which source
//! declares its schema. Signal is pulled from three places — `edges` (kind `db-table`, consumer side),
//! `ambiguous_consumes`, and `unprovided_consumes` — since all three can carry a `db-table` consume.
//!
//! Two sources merely consuming the same table-key string is only evidence of a naming coincidence, not
//! proof of a shared physical database (an unrelated repo providing its own same-named table lands in
//! `ambiguous_consumes` instead, via join integrity) — the finding message says so explicitly.

use std::collections::BTreeSet;

use zzop_core::io::CrossLayerResult;
use zzop_core::{Finding, Severity};

pub fn shared_db_table_findings(cross_layer: &CrossLayerResult) -> Vec<Finding> {
    let mut by_key: std::collections::BTreeMap<String, Vec<(String, String, u32)>> =
        std::collections::BTreeMap::new();

    for e in cross_layer.edges.iter().filter(|e| e.kind == "db-table") {
        by_key.entry(e.key.clone()).or_default().push((
            e.from.source.clone(),
            e.from.file.clone(),
            e.from.line,
        ));
    }
    for a in cross_layer
        .ambiguous_consumes
        .iter()
        .filter(|a| a.consume.kind == "db-table")
    {
        if let Some(key) = &a.consume.key {
            by_key.entry(key.clone()).or_default().push((
                a.source.clone(),
                a.consume.file.clone(),
                a.consume.line,
            ));
        }
    }
    for d in cross_layer
        .unprovided_consumes
        .iter()
        .filter(|d| d.consume.kind == "db-table")
    {
        if let Some(key) = &d.consume.key {
            by_key.entry(key.clone()).or_default().push((
                d.source.clone(),
                d.consume.file.clone(),
                d.consume.line,
            ));
        }
    }

    let mut out = Vec::new();
    for (key, mut sites) in by_key {
        sites.sort();
        sites.dedup();
        let distinct_sources: BTreeSet<&str> = sites.iter().map(|(s, _, _)| s.as_str()).collect();
        if distinct_sources.len() < 2 {
            continue;
        }
        let sources_list: Vec<&str> = distinct_sources.into_iter().collect();
        let (first_source, first_file, first_line) = sites[0].clone();
        let message = format!(
            "db table `{key}` is consumed by {} distinct sources ({}) — first at {first_file}:{first_line} \
             (source `{first_source}`). This only shows the same table identifier is referenced from multiple \
             analyzed sources, not that they physically share one database: unrelated repos with independent \
             databases can coincidentally name a table the same. Verify these sources actually share one \
             database before treating this as real coupling. Disable via rule config \
             `disabled_rules: [\"cross-layer/shared-db-table\"]` if table-name collisions across independent \
             databases are expected in your stack.",
            sources_list.len(),
            sources_list.join(", "),
        );
        out.push(Finding {
            rule_id: "cross-layer/shared-db-table".to_string(),
            severity: Severity::Warning,
            file: first_file,
            line: first_line,
            message,
            data: Some(serde_json::json!({
                "key": key,
                "sources": sources_list,
            })),
        });
    }
    out.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    out
}

#[cfg(test)]
mod tests {
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

    fn unprovided_consume(
        kind: &str,
        key: &str,
        source: &str,
        file: &str,
        line: u32,
    ) -> TaggedConsume {
        TaggedConsume {
            source: source.to_string(),
            consume: IoConsume {
                kind: kind.to_string(),
                key: Some(key.to_string()),
                file: file.to_string(),
                line,
                raw: None,
                method: None,
            },
        }
    }

    #[test]
    fn same_table_consumed_by_two_edge_sources_is_flagged() {
        let cl = CrossLayerResult {
            edges: vec![
                edge("db-table", "table:users", "svc-a", "a.ts", 3),
                edge("db-table", "table:users", "svc-b", "b.ts", 9),
            ],
            ..Default::default()
        };
        let out = shared_db_table_findings(&cl);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "cross-layer/shared-db-table");
        assert_eq!(out[0].severity, Severity::Warning);
        assert_eq!(out[0].file, "a.ts");
        assert_eq!(out[0].line, 3);
        assert!(out[0].message.contains("svc-a"));
        assert!(out[0].message.contains("svc-b"));
        assert!(out[0].message.contains("Verify"));
        assert!(out[0].message.contains("disabled_rules"));
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
        assert_eq!(out.len(), 1);
        assert!(out[0].message.contains("svc-a"));
        assert!(out[0].message.contains("svc-b"));
    }

    #[test]
    fn ambiguous_consume_of_a_db_table_counts_toward_the_signal() {
        let cl = CrossLayerResult {
            edges: vec![edge("db-table", "table:orders", "svc-a", "a.ts", 1)],
            ambiguous_consumes: vec![AmbiguousConsume {
                source: "svc-c".to_string(),
                consume: IoConsume {
                    kind: "db-table".to_string(),
                    key: Some("table:orders".to_string()),
                    file: "c.ts".to_string(),
                    line: 4,
                    raw: None,
                    method: None,
                },
                candidates: vec![TaggedProvide {
                    source: "db1".to_string(),
                    provide: zzop_core::IoProvide {
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
        assert_eq!(out.len(), 1);
        assert!(out[0].message.contains("svc-a"));
        assert!(out[0].message.contains("svc-c"));
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
        assert_eq!(out.len(), 1);
        let sources = out[0].data.as_ref().unwrap()["sources"].as_array().unwrap();
        assert_eq!(sources.len(), 2);
    }
}
