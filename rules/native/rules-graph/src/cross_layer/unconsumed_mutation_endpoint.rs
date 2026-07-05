//! `cross-layer/unconsumed-mutation-endpoint` (warning) — one finding per unconsumed write-verb HTTP provide
//! (`is_write_method`: POST/PUT/PATCH/DELETE): an endpoint that MUTATES state and that no source in this
//! analysis calls. An unconsumed write endpoint is standing attack surface — reachable by anyone who finds
//! it — not merely dead code, hence a warning here versus the plain info of `cross-layer/unconsumed-endpoint`.
//! This rule intentionally co-fires with that rule for the same site: it reports "unreferenced" uniformly
//! across all methods, while this one is the severity-split for the write subset specifically.
//!
//! Provider sites in test-path files (`crate::unreachable::is_test_file`) are skipped — not deployed surface.

use zpz_core::io::TaggedProvide;
use zpz_core::{Finding, Severity};

use super::{is_write_method, split_key};

pub fn unconsumed_mutation_endpoint_findings(
    unconsumed_provides: &[TaggedProvide],
) -> Vec<Finding> {
    let mut out: Vec<Finding> = unconsumed_provides
        .iter()
        .filter(|p| p.provide.kind == "http" && !crate::unreachable::is_test_file(&p.provide.file))
        .filter_map(|p| {
            let (method, _path) = split_key(&p.provide.key)?;
            if !is_write_method(method) {
                return None;
            }
            let key = &p.provide.key;
            let message = format!(
                "write endpoint `{key}` (source `{}`) is not called by any source in this analysis. Because it \
                 mutates state, an unconsumed write route is standing attack surface — reachable by anyone \
                 who finds it — not just dead code. That said, this analysis cannot see every caller: a repo \
                 not included in this `analyzeTrees` run, a mobile/native client, a webhook sender, or a \
                 dynamically-built URL this run could not statically resolve may still call it. This finding \
                 intentionally co-fires with `cross-layer/unconsumed-endpoint` for the same site — this rule \
                 is the severity-split for write verbs specifically. Confirm with real traffic/access logs \
                 before removing the route, or add authorization/rate-limiting if it must stay reachable. \
                 Disable via rule config `disabled_rules: [\"cross-layer/unconsumed-mutation-endpoint\"]` if \
                 provider-only write endpoints (webhook targets, endpoints consumed only outside this \
                 analysis) are expected in your stack.",
                p.source
            );
            Some(Finding {
                rule_id: "cross-layer/unconsumed-mutation-endpoint".to_string(),
                severity: Severity::Warning,
                file: p.provide.file.clone(),
                line: p.provide.line,
                message,
                data: Some(serde_json::json!({
                    "key": key,
                    "source": p.source,
                    "method": method,
                    "symbol": p.provide.symbol,
                })),
            })
        })
        .collect();
    out.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use zpz_core::io::IoProvide;

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

    #[test]
    fn dead_write_endpoint_is_flagged_with_method_and_source() {
        let out = unconsumed_mutation_endpoint_findings(&[unconsumed_provide(
            "http",
            "DELETE /api/users/{}",
            "be",
            "Api.java",
            12,
            Some("deleteUser"),
        )]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "cross-layer/unconsumed-mutation-endpoint");
        assert_eq!(out[0].severity, Severity::Warning);
        assert_eq!(out[0].file, "Api.java");
        assert_eq!(out[0].line, 12);
        assert!(out[0].message.contains("DELETE /api/users/{}"));
        assert!(out[0].message.contains("standing attack surface"));
        assert!(out[0].message.contains("cross-layer/unconsumed-endpoint"));
        assert!(out[0].message.contains("disabled_rules"));
        let data = out[0].data.as_ref().unwrap();
        assert_eq!(data["method"], "DELETE");
        assert_eq!(data["symbol"], "deleteUser");
    }

    #[test]
    fn read_method_dead_endpoint_is_not_this_rules_turf() {
        let out = unconsumed_mutation_endpoint_findings(&[unconsumed_provide(
            "http",
            "GET /api/users",
            "be",
            "Api.java",
            12,
            None,
        )]);
        assert!(out.is_empty());
    }

    #[test]
    fn dead_provide_registered_in_a_test_fixture_file_is_skipped() {
        let out = unconsumed_mutation_endpoint_findings(&[unconsumed_provide(
            "http",
            "POST /api/users",
            "be",
            "src/api/__test__/handlers.test.ts",
            5,
            None,
        )]);
        assert!(out.is_empty());
    }

    #[test]
    fn non_http_dead_provide_is_ignored() {
        let out = unconsumed_mutation_endpoint_findings(&[unconsumed_provide(
            "db-table",
            "table:users",
            "db",
            "schema.sql",
            1,
            None,
        )]);
        assert!(out.is_empty());
    }

    #[test]
    fn determinism_multiple_findings_sorted_by_file_then_line() {
        let out = unconsumed_mutation_endpoint_findings(&[
            unconsumed_provide("http", "POST /b", "be", "z.java", 1, None),
            unconsumed_provide("http", "PUT /a", "be", "a.java", 9, None),
            unconsumed_provide("http", "PATCH /c", "be", "a.java", 2, None),
        ]);
        let sites: Vec<(&str, u32)> = out.iter().map(|f| (f.file.as_str(), f.line)).collect();
        assert_eq!(sites, vec![("a.java", 2), ("a.java", 9), ("z.java", 1)]);
    }
}
