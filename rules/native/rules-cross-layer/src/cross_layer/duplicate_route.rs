//! `cross-layer/duplicate-route` (warning) — the same `http` `(method, path)` key PROVIDED by 2+ DISTINCT
//! sources. This is the multi-tree-provider condition `zzop_core::io::link_cross_layer_io` already computes
//! internally (`ambiguous_keys`, io.rs), read back off its two observable traces: an unconsumed multi-source
//! key lands in `unconsumed_provides` (grouped by key), and one some consume references lands in
//! `ambiguous_consumes` with its full candidate list. A key provided by 2+ distinct sources can never
//! produce a `CrossLayerEdge` — the linker routes it to `ambiguous_consumes` before edge emission — so those
//! two buckets together cover every multi-source-provided http key, and `edges` is never a source here.
//!
//! Distinct from the existing single-tree `zzop_rules_http::duplicate_route` rule (id `"duplicate-route"`): that one
//! flags 2+ registrations of a route WITHIN one tree; this one only fires when the duplicates span 2+
//! DIFFERENT trees. Different id, so both can be registered/disabled independently.
//!
//! Provider sites in test-path files (`zzop_core::is_test_file`) are skipped, same policy as
//! `mutating-route-no-auth`. A dead multi-tree route also yields per-provider
//! `cross-layer/unconsumed-endpoint` info findings — the overlap is intentional (different questions: "who
//! serves this?" vs "who calls this?").

use std::collections::BTreeSet;

use zzop_core::io::CrossLayerResult;
use zzop_core::{disable_hint, Finding, Severity};

pub fn cross_layer_duplicate_route_findings(cross_layer: &CrossLayerResult) -> Vec<Finding> {
    let mut by_key: std::collections::BTreeMap<String, Vec<(String, String, u32)>> =
        std::collections::BTreeMap::new();

    for p in cross_layer
        .unconsumed_provides
        .iter()
        // A verb-unknown route (`UNKNOWN_VERB` sentinel `"? <path>"`) is never a "duplicate route"
        // candidate — its method is unknown, so comparing sentinel keys across trees is meaningless, and
        // its `"? <path>"` key must never surface in a finding message. It is disclosed via
        // `cross-layer/unknown-verb-route` instead.
        .filter(|p| {
            p.provide.kind == "http"
                && zzop_core::unknown_verb_route_path(&p.provide.key).is_none()
                && !zzop_core::is_test_file(&p.provide.file)
        })
    {
        by_key.entry(p.provide.key.clone()).or_default().push((
            p.source.clone(),
            p.provide.file.clone(),
            p.provide.line,
        ));
    }
    for a in cross_layer
        .ambiguous_consumes
        .iter()
        .filter(|a| a.consume.kind == "http")
    {
        for cand in &a.candidates {
            if zzop_core::is_test_file(&cand.provide.file) {
                continue;
            }
            by_key.entry(cand.provide.key.clone()).or_default().push((
                cand.source.clone(),
                cand.provide.file.clone(),
                cand.provide.line,
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
        let (first_source, first_file, first_line) = sites[0].clone();
        let sites_desc: Vec<String> = sites
            .iter()
            .map(|(s, f, l)| format!("{s}:{f}:{l}"))
            .collect();
        let message = format!(
            "route `{key}` is provided by {} distinct sources ({}) — first at {first_file}:{first_line} (source \
             `{first_source}`). A caller cannot deterministically tell which source's handler serves a request \
             for this route; if these sources are ever deployed behind the same host/gateway, whichever one \
             wins is a deploy-order accident, not a design decision. Merge the handlers, or namespace the \
             routes apart (a path prefix, a different host). {} if these are intentionally separate services \
             on different hosts that happen to share a route shape.",
            distinct_sources.len(),
            sites_desc.join(", "),
            disable_hint("cross-layer/duplicate-route"),
        );
        out.push(Finding {
            rule_id: "cross-layer/duplicate-route".to_string(),
            severity: Severity::Warning,
            file: first_file,
            line: first_line,
            message,
            data: Some(serde_json::json!({
                "key": key,
                "sites": sites_desc,
            })),
        });
    }
    out.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use zzop_core::io::{AmbiguousConsume, TaggedProvide};
    use zzop_core::{IoConsume, IoProvide};

    fn dead(key: &str, source: &str, file: &str, line: u32) -> TaggedProvide {
        TaggedProvide {
            source: source.to_string(),
            provide: IoProvide {
                body: None,
                kind: "http".to_string(),
                key: key.to_string(),
                file: file.to_string(),
                line,
                symbol: None,
            },
        }
    }

    #[test]
    fn key_provided_by_two_trees_with_no_consumer_is_flagged_from_unconsumed_provides() {
        let cl = CrossLayerResult {
            unconsumed_provides: vec![
                dead("DELETE /api/me", "svc-a", "a.ts", 3),
                dead("DELETE /api/me", "svc-b", "b.ts", 9),
            ],
            ..Default::default()
        };
        let out = cross_layer_duplicate_route_findings(&cl);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "cross-layer/duplicate-route");
        assert_eq!(out[0].severity, Severity::Warning);
        assert_eq!(out[0].file, "a.ts");
        assert_eq!(out[0].line, 3);
        assert!(out[0].message.contains("svc-a"));
        assert!(out[0].message.contains("svc-b"));
        assert!(out[0].message.contains("disabled_rules"));
    }

    #[test]
    fn two_verb_unknown_sentinels_at_the_same_path_are_not_a_duplicate_route() {
        // A verb-unknown sentinel (`"? <path>"`) is never a duplicate-route candidate — its method is
        // unknown, so its key must never be grouped or surfaced in a finding message (it is disclosed via
        // `cross-layer/unknown-verb-route` instead). Two serve-all handlers at the same path across trees
        // must NOT produce a `? /api/foo` duplicate-route warning (opus review F1 regression pin).
        let cl = CrossLayerResult {
            unconsumed_provides: vec![
                dead("? /api/foo", "svc-a", "a/pages/api/foo.ts", 1),
                dead("? /api/foo", "svc-b", "b/foo.go", 1),
            ],
            ..Default::default()
        };
        assert!(cross_layer_duplicate_route_findings(&cl).is_empty());
    }

    #[test]
    fn key_provided_by_two_trees_and_referenced_by_a_consume_is_flagged_from_ambiguous() {
        let cl = CrossLayerResult {
            ambiguous_consumes: vec![AmbiguousConsume {
                source: "gateway".to_string(),
                consume: IoConsume {
                    client: None,
                    body: None,
                    kind: "http".to_string(),
                    key: Some("GET /health".to_string()),
                    file: "gw.ts".to_string(),
                    line: 1,
                    raw: None,
                    method: None,
                },
                candidates: vec![
                    dead("GET /health", "svc-a", "svc-a/health.ts", 3),
                    dead("GET /health", "svc-b", "svc-b/health.ts", 7),
                ],
            }],
            ..Default::default()
        };
        let out = cross_layer_duplicate_route_findings(&cl);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].file, "svc-a/health.ts");
        assert_eq!(out[0].line, 3);
        assert!(out[0].message.contains("svc-a"));
        assert!(out[0].message.contains("svc-b"));
    }

    #[test]
    fn provider_site_in_a_test_fixture_file_does_not_count_toward_duplication() {
        // svc-b's "registration" is a test fixture — not deployed surface. With it skipped, only one
        // real provider tree remains, so no duplicate-route finding.
        let cl = CrossLayerResult {
            unconsumed_provides: vec![
                dead("DELETE /api/me", "svc-a", "src/api/routes.ts", 3),
                dead(
                    "DELETE /api/me",
                    "svc-b",
                    "src/api/__test__/handlers.test.ts",
                    125,
                ),
            ],
            ..Default::default()
        };
        assert!(cross_layer_duplicate_route_findings(&cl).is_empty());
    }

    #[test]
    fn key_provided_by_only_one_tree_is_not_flagged() {
        let cl = CrossLayerResult {
            unconsumed_provides: vec![
                dead("GET /api/users", "svc-a", "a.ts", 3),
                dead("GET /api/users", "svc-a", "a2.ts", 5),
            ],
            ..Default::default()
        };
        assert!(cross_layer_duplicate_route_findings(&cl).is_empty());
    }

    #[test]
    fn non_http_kind_is_ignored() {
        let cl = CrossLayerResult {
            unconsumed_provides: vec![
                TaggedProvide {
                    source: "svc-a".to_string(),
                    provide: IoProvide {
                        body: None,
                        kind: "db-table".to_string(),
                        key: "table:users".to_string(),
                        file: "a.sql".to_string(),
                        line: 1,
                        symbol: None,
                    },
                },
                TaggedProvide {
                    source: "svc-b".to_string(),
                    provide: IoProvide {
                        body: None,
                        kind: "db-table".to_string(),
                        key: "table:users".to_string(),
                        file: "b.sql".to_string(),
                        line: 1,
                        symbol: None,
                    },
                },
            ],
            ..Default::default()
        };
        assert!(cross_layer_duplicate_route_findings(&cl).is_empty());
    }
}
