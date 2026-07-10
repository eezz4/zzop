//! `cross-layer/unconsumed-endpoint` (info) — one finding per `CrossLayerResult::unconsumed_provides` entry of
//! kind `"http"`: an endpoint no source in this `analyzeTrees` run calls. Severity starts at info (not
//! warning) because "no consumer WITHIN this analysis" is weaker evidence than "no consumer at all" — see
//! the message's own caveat.
//!
//! Provider sites in test-path files (`zzop_core::is_test_file`) are skipped — a route registered
//! in a test fixture is not deployed surface. A dead route provided by 2+ trees ALSO fires one warning
//! `cross-layer/duplicate-route` finding for the same key — intentional overlap, different questions.

use zzop_core::io::{TaggedConsume, TaggedProvide};
use zzop_core::{disable_hint, Finding, Severity};

pub fn unconsumed_endpoint_findings(
    unconsumed_provides: &[TaggedProvide],
    unresolved_consumes: &[TaggedConsume],
) -> Vec<Finding> {
    let unresolved_http = unresolved_consumes
        .iter()
        .filter(|c| c.consume.kind == "http")
        .count();

    let mut out: Vec<Finding> = unconsumed_provides
        .iter()
        .filter(|p| p.provide.kind == "http" && !zzop_core::is_test_file(&p.provide.file))
        .map(|p| {
            let key = &p.provide.key;
            let message = format!(
                "endpoint `{key}` (source `{}`) is not called by any source in this analysis. This may be \
                 genuinely dead route code, or it may be consumed by a caller this analysis cannot see — a \
                 repo not included in this `analyzeTrees` run, a mobile/native/third-party client, or one of \
                 the {unresolved_http} unresolved dynamic-URL http consume(s) this run could not statically \
                 match to a key (see `crossLayer.unresolvedConsumes`). Confirm with real traffic/access logs before \
                 removing the route. {} if provider-only endpoints (webhook targets, health probes, \
                 endpoints consumed only outside this analysis) are expected in your stack.",
                p.source,
                disable_hint("cross-layer/unconsumed-endpoint")
            );
            Finding {
                rule_id: "cross-layer/unconsumed-endpoint".to_string(),
                severity: Severity::Info,
                file: p.provide.file.clone(),
                line: p.provide.line,
                message,
                data: Some(serde_json::json!({
                    "key": key,
                    "source": p.source,
                    "unresolvedHttpConsumeCount": unresolved_http,
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

    fn dead(key: &str, source: &str, file: &str, line: u32) -> TaggedProvide {
        TaggedProvide {
            source: source.to_string(),
            provide: zzop_core::IoProvide {
                kind: "http".to_string(),
                key: key.to_string(),
                file: file.to_string(),
                line,
                symbol: None,
            },
        }
    }

    fn dead_kind(kind: &str, key: &str, source: &str, file: &str, line: u32) -> TaggedProvide {
        TaggedProvide {
            source: source.to_string(),
            provide: zzop_core::IoProvide {
                kind: kind.to_string(),
                key: key.to_string(),
                file: file.to_string(),
                line,
                symbol: None,
            },
        }
    }

    fn unresolved(kind: &str, source: &str) -> TaggedConsume {
        TaggedConsume {
            source: source.to_string(),
            consume: zzop_core::IoConsume {
                kind: kind.to_string(),
                key: None,
                file: "dyn.ts".to_string(),
                line: 1,
                raw: Some("dyn".to_string()),
                method: None,
            },
        }
    }

    #[test]
    fn dead_http_provide_is_flagged_with_source_and_anchor() {
        let out = unconsumed_endpoint_findings(&[dead("GET /orphan", "be", "Api.java", 12)], &[]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "cross-layer/unconsumed-endpoint");
        assert_eq!(out[0].severity, Severity::Info);
        assert_eq!(out[0].file, "Api.java");
        assert_eq!(out[0].line, 12);
        assert!(out[0].message.contains("GET /orphan"));
        assert!(out[0].message.contains("source `be`"));
        assert!(out[0].message.contains("disabled_rules"));
    }

    #[test]
    fn dead_provide_registered_in_a_test_fixture_file_is_skipped() {
        let out = unconsumed_endpoint_findings(
            &[dead(
                "GET /fixture",
                "be",
                "src/api/__test__/handlers.test.ts",
                125,
            )],
            &[],
        );
        assert!(out.is_empty());
    }

    #[test]
    fn non_http_dead_provide_is_ignored() {
        let out = unconsumed_endpoint_findings(
            &[dead_kind("db-table", "table:users", "db", "schema.sql", 1)],
            &[],
        );
        assert!(out.is_empty());
    }

    #[test]
    fn no_unconsumed_provides_is_empty() {
        assert!(unconsumed_endpoint_findings(&[], &[]).is_empty());
    }

    #[test]
    fn message_states_the_unresolved_http_count_honestly() {
        let out = unconsumed_endpoint_findings(
            &[dead("GET /orphan", "be", "Api.java", 12)],
            &[
                unresolved("http", "fe"),
                unresolved("http", "fe"),
                unresolved("queue", "fe"), // not http — must not inflate the count
            ],
        );
        assert_eq!(out.len(), 1);
        assert!(out[0].message.contains("2 unresolved"));
    }

    #[test]
    fn multiple_unconsumed_provides_are_sorted_by_file_then_line() {
        let out = unconsumed_endpoint_findings(
            &[
                dead("GET /b", "be", "z.java", 1),
                dead("GET /a", "be", "a.java", 9),
                dead("GET /c", "be", "a.java", 2),
            ],
            &[],
        );
        let sites: Vec<(&str, u32)> = out.iter().map(|f| (f.file.as_str(), f.line)).collect();
        assert_eq!(sites, vec![("a.java", 2), ("a.java", 9), ("z.java", 1)]);
    }
}
