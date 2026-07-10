//! `cross-layer/ambiguous-consume` (warning) — one finding per `CrossLayerResult::ambiguous_consumes` entry
//! whose kind is not `"db-table"`: a consume whose key is provided by 2+ distinct source trees, so which
//! provider actually serves the call depends on deploy-time routing (load balancer, service mesh,
//! feature-flagged rollout, ...) that static analysis cannot see. `"db-table"` sharing is normal/expected
//! and belongs to `cross-layer/shared-db-table` instead.
//!
//! Anchored at the consume site, not any candidate provider — the ambiguity is a property of the call site:
//! the caller cannot know, from source alone, which candidate will actually answer.

use zzop_core::io::AmbiguousConsume;
use zzop_core::{disable_hint, Finding, Severity};

/// `candidates` in the emitted `data` are capped to this many entries — the finding cites the total via
/// `candidateCount`/`candidateSourceCount` even when the list itself is truncated.
const MAX_CANDIDATES: usize = 5;

pub fn ambiguous_consume_findings(ambiguous_consumes: &[AmbiguousConsume]) -> Vec<Finding> {
    let mut out: Vec<Finding> = ambiguous_consumes
        .iter()
        .filter(|a| a.consume.kind != "db-table")
        .map(|a| {
            let kind = &a.consume.kind;
            let key = a.consume.key.as_deref().unwrap_or("<unresolved>");
            let mut distinct_sources: Vec<&str> =
                a.candidates.iter().map(|c| c.source.as_str()).collect();
            distinct_sources.sort();
            distinct_sources.dedup();
            let candidate_source_count = distinct_sources.len();
            let candidate_count = a.candidates.len();

            let candidates_json: Vec<serde_json::Value> = a
                .candidates
                .iter()
                .take(MAX_CANDIDATES)
                .map(|c| {
                    serde_json::json!({
                        "source": c.source,
                        "file": c.provide.file,
                        "line": c.provide.line,
                    })
                })
                .collect();

            let message = format!(
                "consume `{kind} {key}` (source `{}`) matches provides in {candidate_source_count} distinct \
                 sources — which one actually answers this call at runtime depends on deploy-time \
                 routing (load balancer / service mesh / gateway rule) that this static analysis cannot \
                 observe. Either disambiguate the route (e.g. give each service a distinct key prefix) or \
                 confirm which of the {candidate_count} candidate provider(s) is the intended one and \
                 document that routing decision. {} if this is an intentional gateway fan-out where any \
                 provider is equally valid (e.g. a stateless health/echo route deliberately duplicated across \
                 replicas behind a shared gateway).",
                a.source,
                disable_hint("cross-layer/ambiguous-consume")
            );

            Finding {
                rule_id: "cross-layer/ambiguous-consume".to_string(),
                severity: Severity::Warning,
                file: a.consume.file.clone(),
                line: a.consume.line,
                message,
                data: Some(serde_json::json!({
                    "kind": kind,
                    "key": key,
                    "consumeSource": a.source,
                    "candidateSourceCount": candidate_source_count,
                    "candidates": candidates_json,
                    "candidateCount": candidate_count,
                })),
            }
        })
        .collect();
    out.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use zzop_core::io::{IoConsume, TaggedProvide};
    use zzop_core::IoProvide;

    fn tagged_provide(source: &str, file: &str, line: u32) -> TaggedProvide {
        TaggedProvide {
            source: source.to_string(),
            provide: IoProvide {
                kind: "http".to_string(),
                key: "GET /health".to_string(),
                file: file.to_string(),
                line,
                symbol: None,
            },
        }
    }

    fn ambiguous(
        kind: &str,
        key: Option<&str>,
        source: &str,
        file: &str,
        line: u32,
        candidates: Vec<TaggedProvide>,
    ) -> AmbiguousConsume {
        AmbiguousConsume {
            source: source.to_string(),
            consume: IoConsume {
                kind: kind.to_string(),
                key: key.map(str::to_string),
                file: file.to_string(),
                line,
                raw: None,
                method: None,
            },
            candidates,
        }
    }

    #[test]
    fn multi_tree_candidates_are_flagged_anchored_at_the_consume() {
        let entries = vec![ambiguous(
            "http",
            Some("GET /health"),
            "gateway",
            "gw.ts",
            1,
            vec![
                tagged_provide("svc-a", "svc-a/health.ts", 3),
                tagged_provide("svc-b", "svc-b/health.ts", 7),
            ],
        )];
        let out = ambiguous_consume_findings(&entries);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "cross-layer/ambiguous-consume");
        assert_eq!(out[0].severity, Severity::Warning);
        assert_eq!(out[0].file, "gw.ts");
        assert_eq!(out[0].line, 1);
        assert!(out[0].message.contains("GET /health"));
        assert!(out[0].message.contains("2 distinct"));
        assert!(out[0].message.contains("disabled_rules"));
        let data = out[0].data.as_ref().unwrap();
        assert_eq!(data["candidateSourceCount"], 2);
        assert_eq!(data["candidateCount"], 2);
        assert_eq!(data["candidates"][0]["source"], "svc-a");
        assert_eq!(data["candidates"][1]["source"], "svc-b");
    }

    #[test]
    fn db_table_kind_is_excluded_it_belongs_to_shared_db_table() {
        let entries = vec![ambiguous(
            "db-table",
            Some("table:users"),
            "be",
            "Repo.java",
            10,
            vec![
                tagged_provide("svc-a", "a/schema.sql", 1),
                tagged_provide("svc-b", "b/schema.sql", 1),
            ],
        )];
        assert!(ambiguous_consume_findings(&entries).is_empty());
    }

    #[test]
    fn candidates_beyond_the_cap_are_truncated_but_counted_honestly() {
        let candidates: Vec<TaggedProvide> = ('a'..='g')
            .map(|c| tagged_provide(&format!("svc-{c}"), &format!("{c}.ts"), 1))
            .collect();
        let entries = vec![ambiguous(
            "http",
            Some("GET /health"),
            "gateway",
            "gw.ts",
            1,
            candidates,
        )];
        let out = ambiguous_consume_findings(&entries);
        assert_eq!(out.len(), 1);
        let data = out[0].data.as_ref().unwrap();
        assert_eq!(data["candidateSourceCount"], 7);
        assert_eq!(data["candidateCount"], 7);
        assert_eq!(data["candidates"].as_array().unwrap().len(), 5);
    }

    #[test]
    fn determinism_multiple_findings_sorted_by_file_then_line() {
        let entries = vec![
            ambiguous(
                "http",
                Some("GET /b"),
                "gw",
                "z.ts",
                1,
                vec![
                    tagged_provide("svc-a", "a.ts", 1),
                    tagged_provide("svc-b", "b.ts", 1),
                ],
            ),
            ambiguous(
                "http",
                Some("GET /a"),
                "gw",
                "a.ts",
                9,
                vec![
                    tagged_provide("svc-a", "a.ts", 1),
                    tagged_provide("svc-b", "b.ts", 1),
                ],
            ),
            ambiguous(
                "http",
                Some("GET /c"),
                "gw",
                "a.ts",
                2,
                vec![
                    tagged_provide("svc-a", "a.ts", 1),
                    tagged_provide("svc-b", "b.ts", 1),
                ],
            ),
        ];
        let out = ambiguous_consume_findings(&entries);
        let sites: Vec<(&str, u32)> = out.iter().map(|f| (f.file.as_str(), f.line)).collect();
        assert_eq!(sites, vec![("a.ts", 2), ("a.ts", 9), ("z.ts", 1)]);
    }
}
