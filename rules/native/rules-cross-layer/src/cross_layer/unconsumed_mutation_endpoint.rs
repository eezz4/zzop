//! `cross-layer/unconsumed-mutation-endpoint` (warning) — one finding per unconsumed write-verb HTTP provide
//! (`is_write_method`: POST/PUT/PATCH/DELETE): an endpoint that MUTATES state and that no source in this
//! analysis calls. An unconsumed write endpoint is standing attack surface — reachable by anyone who finds
//! it — not merely dead code, hence a warning here versus the plain info of `cross-layer/unconsumed-endpoint`.
//! This rule intentionally co-fires with that rule for the same site: it reports "unreferenced" uniformly
//! across all methods, while this one is the severity-split for the write subset specifically.
//!
//! Provider sites in test-path files (`zzop_core::is_test_file`) are skipped — not deployed surface.
//!
//! ## Near-miss cross-reference
//! Same annotation as the sibling `unconsumed_endpoint`: when a write provide here is ALSO the chosen
//! near-miss target of an unmatched `cross-layer/route-near-miss` consume (`near_miss_targets`, sourced from
//! `route_near_miss::route_near_miss_results`), the message gains a cross-reference note pointing at that
//! finding — see `unconsumed_endpoint`'s module doc for the dogfood motivation.

use std::collections::BTreeMap;

use zzop_core::io::TaggedProvide;
use zzop_core::{disable_hint, Finding, Severity};

use super::route_near_miss::NearMissTargetRef;
use super::{is_write_method, split_key};

pub fn unconsumed_mutation_endpoint_findings(
    unconsumed_provides: &[TaggedProvide],
    near_miss_targets: &BTreeMap<(String, String, u32), NearMissTargetRef>,
) -> Vec<Finding> {
    let mut out: Vec<Finding> = unconsumed_provides
        .iter()
        .filter(|p| p.provide.kind == "http" && !zzop_core::is_test_file(&p.provide.file))
        .filter_map(|p| {
            let (method, _path) = split_key(&p.provide.key)?;
            if !is_write_method(method) {
                return None;
            }
            let key = &p.provide.key;
            let near_miss = near_miss_targets.get(&(
                p.source.clone(),
                p.provide.file.clone(),
                p.provide.line,
            ));
            let near_miss_note = if let Some(t) = near_miss {
                format!(
                    " However, {} unmatched http consume(s) in this run name this route as their closest \
                     near-miss candidate (see the `cross-layer/route-near-miss` finding at {}:{}) — the route \
                     may actually be called through a drifted or base-relative path rather than being dead.",
                    t.count, t.consume_file, t.consume_line
                )
            } else {
                String::new()
            };
            let message = format!(
                "write endpoint `{key}` (source `{}`) is not called by any source in this analysis. Because it \
                 mutates state, an unconsumed write route is standing attack surface — reachable by anyone \
                 who finds it — not just dead code. That said, this analysis cannot see every caller: a repo \
                 not included in this `analyzeTrees` run, a mobile/native client, a webhook sender, or a \
                 dynamically-built URL this run could not statically resolve may still call it. This finding \
                 intentionally co-fires with `cross-layer/unconsumed-endpoint` for the same site — this rule \
                 is the severity-split for write verbs specifically. Confirm with real traffic/access logs \
                 before removing the route, or add authorization/rate-limiting if it must stay reachable.\
                 {near_miss_note} {} if provider-only write endpoints (webhook targets, endpoints consumed only outside this \
                 analysis) are expected in your stack.",
                p.source,
                disable_hint("cross-layer/unconsumed-mutation-endpoint")
            );
            let mut data = serde_json::json!({
                "key": key,
                "source": p.source,
                "method": method,
                "symbol": p.provide.symbol,
            });
            if let Some(t) = near_miss {
                data["nearMissConsumeCount"] = serde_json::json!(t.count);
                data["nearMissConsumeExample"] =
                    serde_json::json!(format!("{}:{}", t.consume_file, t.consume_line));
            }
            Some(Finding {
                rule_id: "cross-layer/unconsumed-mutation-endpoint".to_string(),
                severity: Severity::Warning,
                file: p.provide.file.clone(),
                line: p.provide.line,
                message,
                data: Some(data),
            })
        })
        .collect();
    out.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use zzop_core::io::IoProvide;

    fn unconsumed_provide(
        kind: &str,
        key: &str,
        source: &str,
        file: &str,
        line: u32,
        symbol: Option<&str>,
    ) -> TaggedProvide {
        TaggedProvide {
            source: source.to_string(),
            provide: IoProvide {
                kind: kind.to_string(),
                key: key.to_string(),
                file: file.to_string(),
                line,
                symbol: symbol.map(str::to_string),
            },
        }
    }

    fn no_near_miss() -> BTreeMap<(String, String, u32), NearMissTargetRef> {
        BTreeMap::new()
    }

    #[test]
    fn dead_write_endpoint_is_flagged_with_method_and_source() {
        let out = unconsumed_mutation_endpoint_findings(
            &[unconsumed_provide(
                "http",
                "DELETE /api/users/{}",
                "be",
                "Api.java",
                12,
                Some("deleteUser"),
            )],
            &no_near_miss(),
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "cross-layer/unconsumed-mutation-endpoint");
        assert_eq!(out[0].severity, Severity::Warning);
        assert_eq!(out[0].file, "Api.java");
        assert_eq!(out[0].line, 12);
        assert!(out[0].message.contains("DELETE /api/users/{}"));
        assert!(out[0].message.contains("standing attack surface"));
        assert!(out[0].message.contains("cross-layer/unconsumed-endpoint"));
        assert!(out[0].message.contains("disabled_rules"));
        assert!(!out[0].message.contains("near-miss"));
        let data = out[0].data.as_ref().unwrap();
        assert_eq!(data["method"], "DELETE");
        assert_eq!(data["symbol"], "deleteUser");
    }

    #[test]
    fn read_method_dead_endpoint_is_not_this_rules_turf() {
        let out = unconsumed_mutation_endpoint_findings(
            &[unconsumed_provide(
                "http",
                "GET /api/users",
                "be",
                "Api.java",
                12,
                None,
            )],
            &no_near_miss(),
        );
        assert!(out.is_empty());
    }

    #[test]
    fn dead_provide_registered_in_a_test_fixture_file_is_skipped() {
        let out = unconsumed_mutation_endpoint_findings(
            &[unconsumed_provide(
                "http",
                "POST /api/users",
                "be",
                "src/api/__test__/handlers.test.ts",
                5,
                None,
            )],
            &no_near_miss(),
        );
        assert!(out.is_empty());
    }

    #[test]
    fn non_http_dead_provide_is_ignored() {
        let out = unconsumed_mutation_endpoint_findings(
            &[unconsumed_provide(
                "db-table",
                "table:users",
                "db",
                "schema.sql",
                1,
                None,
            )],
            &no_near_miss(),
        );
        assert!(out.is_empty());
    }

    #[test]
    fn determinism_multiple_findings_sorted_by_file_then_line() {
        let out = unconsumed_mutation_endpoint_findings(
            &[
                unconsumed_provide("http", "POST /b", "be", "z.java", 1, None),
                unconsumed_provide("http", "PUT /a", "be", "a.java", 9, None),
                unconsumed_provide("http", "PATCH /c", "be", "a.java", 2, None),
            ],
            &no_near_miss(),
        );
        let sites: Vec<(&str, u32)> = out.iter().map(|f| (f.file.as_str(), f.line)).collect();
        assert_eq!(sites, vec![("a.java", 2), ("a.java", 9), ("z.java", 1)]);
    }

    #[test]
    fn near_miss_cross_reference_note_fires_when_the_provide_is_a_near_miss_target() {
        let mut targets = BTreeMap::new();
        targets.insert(
            ("be".to_string(), "Api.java".to_string(), 12),
            NearMissTargetRef {
                consume_file: "Api.tsx".to_string(),
                consume_line: 7,
                count: 2,
            },
        );
        let out = unconsumed_mutation_endpoint_findings(
            &[unconsumed_provide(
                "http",
                "DELETE /api/users/{}",
                "be",
                "Api.java",
                12,
                Some("deleteUser"),
            )],
            &targets,
        );
        assert_eq!(out.len(), 1);
        assert!(out[0].message.contains("2 unmatched http consume(s)"));
        assert!(out[0]
            .message
            .contains("cross-layer/route-near-miss` finding at Api.tsx:7"));
        let data = out[0].data.as_ref().unwrap();
        assert_eq!(data["nearMissConsumeCount"], 2);
        assert_eq!(data["nearMissConsumeExample"], "Api.tsx:7");
    }

    #[test]
    fn near_miss_cross_reference_note_is_absent_when_the_provide_is_not_a_near_miss_target() {
        let out = unconsumed_mutation_endpoint_findings(
            &[unconsumed_provide(
                "http",
                "DELETE /api/users/{}",
                "be",
                "Api.java",
                12,
                Some("deleteUser"),
            )],
            &no_near_miss(),
        );
        assert_eq!(out.len(), 1);
        assert!(!out[0].message.contains("near-miss"));
        assert!(out[0]
            .data
            .as_ref()
            .unwrap()
            .get("nearMissConsumeCount")
            .is_none());
    }
}
