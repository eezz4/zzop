//! `cross-layer/external-shadow-internal` (warning) — an `external` (host-carrying) http consume whose
//! normalized `METHOD path` matches a route some analyzed tree actually PROVIDES. The caller reaches an
//! INTERNAL route through a hardcoded absolute URL (an environment host baked directly into the call site)
//! instead of the relative/proxy path the route is normally reached through — a classic source of
//! environment-specific breakage (the hardcoded host is wrong in another environment) and a bypass of
//! whatever the proxy/gateway layer was meant to enforce (auth, rewriting, rate limiting). Anchored at the
//! consume — the fix (drop the hardcoded host, go through the relative/proxied path) lands there.
//!
//! Consume sites in test-path files (`crate::unreachable::is_test_file`) are skipped — a test mocking a
//! vendor/own API is not deployed egress.

use std::collections::BTreeMap;

use zpz_core::io::TaggedConsume;
use zpz_core::{http_interface_key, Finding, Severity};

use super::{split_external_key, HttpProvideSite};

pub fn external_shadow_internal_findings(
    external_consumes: &[TaggedConsume],
    all_provides: &[HttpProvideSite],
) -> Vec<Finding> {
    let mut by_key: BTreeMap<&str, Vec<&HttpProvideSite>> = BTreeMap::new();
    for p in all_provides {
        by_key.entry(p.key.as_str()).or_default().push(p);
    }
    for sites in by_key.values_mut() {
        sites.sort_by(|a, b| {
            a.source
                .cmp(&b.source)
                .then(a.file.cmp(&b.file))
                .then(a.line.cmp(&b.line))
        });
    }

    let mut out = Vec::new();
    for c in external_consumes
        .iter()
        .filter(|c| c.consume.kind == "http" && !crate::unreachable::is_test_file(&c.consume.file))
    {
        let Some(key) = c.consume.key.as_deref() else {
            continue;
        };
        let Some(url) = split_external_key(key) else {
            continue;
        };
        let normalized = http_interface_key(url.method, url.path);
        let Some(sites) = by_key.get(normalized.as_str()) else {
            continue;
        };
        let Some(first) = sites.first() else {
            continue;
        };
        let other_provide_count = sites.len() - 1;
        let other_note = if other_provide_count > 0 {
            format!(" (and {other_provide_count} other provide site(s) also serve this route)")
        } else {
            String::new()
        };

        let message = format!(
            "consume `{key}` (source `{}`) reaches host `{}` with an absolute URL, but the same route \
             `{normalized}` is provided INTERNALLY by this analysis — at {}:{} (source `{}`){other_note}. This \
             looks like a hardcoded environment host baked into the call site instead of the relative or \
             proxied path the route is normally reached through, which breaks in any other environment and \
             may bypass whatever a gateway/proxy layer enforces (auth, rewriting, rate limiting). Verify \
             whether this call is meant to hit the internal route directly, and if so replace the hardcoded \
             host with the relative/proxy path. Disable via rule config `disabled_rules: \
             [\"cross-layer/external-shadow-internal\"]` if hitting this host directly (bypassing the proxy) \
             is intentional, e.g. a health check or an internal tool calling a fixed deployment URL on \
             purpose.",
            c.source, url.host, first.file, first.line, first.source,
        );
        out.push(Finding {
            rule_id: "cross-layer/external-shadow-internal".to_string(),
            severity: Severity::Warning,
            file: c.consume.file.clone(),
            line: c.consume.line,
            message,
            data: Some(serde_json::json!({
                "consumeKey": key,
                "host": url.host,
                "normalizedKey": normalized,
                "matchedProvide": {"source": first.source, "file": first.file, "line": first.line},
                "otherProvideCount": other_provide_count,
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
            consume: zpz_core::IoConsume {
                kind: "http".to_string(),
                key: key.map(str::to_string),
                file: file.to_string(),
                line,
                raw: None,
                method: None,
            },
        }
    }

    fn provide(key: &str, source: &str, file: &str, line: u32) -> HttpProvideSite {
        HttpProvideSite {
            source: source.to_string(),
            key: key.to_string(),
            file: file.to_string(),
            line,
        }
    }

    #[test]
    fn absolute_url_matching_an_internal_route_is_flagged_anchored_at_the_consume() {
        let external = vec![consume(
            Some("GET https://app.internal.example.com/api/users"),
            "fe",
            "Ctx.tsx",
            10,
        )];
        let provides = vec![provide("GET /api/users", "be", "Api.java", 20)];
        let out = external_shadow_internal_findings(&external, &provides);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "cross-layer/external-shadow-internal");
        assert_eq!(out[0].severity, Severity::Warning);
        assert_eq!(out[0].file, "Ctx.tsx");
        assert_eq!(out[0].line, 10);
        assert!(out[0].message.contains("app.internal.example.com"));
        assert!(out[0].message.contains("Api.java:20"));
        assert!(out[0].message.contains("disabled_rules"));
        let data = out[0].data.as_ref().unwrap();
        assert_eq!(data["host"], "app.internal.example.com");
        assert_eq!(data["normalizedKey"], "GET /api/users");
        assert_eq!(data["otherProvideCount"], 0);
    }

    #[test]
    fn unprovided_path_is_not_flagged() {
        let external = vec![consume(
            Some("GET https://api.vendor.com/v1/widgets"),
            "fe",
            "Ctx.tsx",
            10,
        )];
        let provides = vec![provide("GET /api/users", "be", "Api.java", 20)];
        assert!(external_shadow_internal_findings(&external, &provides).is_empty());
    }

    #[test]
    fn non_http_external_consume_is_ignored() {
        let mut c = consume(
            Some("GET https://app.internal.example.com/api/users"),
            "fe",
            "Ctx.tsx",
            10,
        );
        c.consume.kind = "queue".to_string();
        let provides = vec![provide("GET /api/users", "be", "Api.java", 20)];
        assert!(external_shadow_internal_findings(&[c], &provides).is_empty());
    }

    #[test]
    fn multiple_matching_provides_report_first_sorted_and_other_count() {
        let external = vec![consume(
            Some("GET https://app.internal.example.com/api/users"),
            "fe",
            "Ctx.tsx",
            10,
        )];
        let provides = vec![
            provide("GET /api/users", "be2", "Z.java", 1),
            provide("GET /api/users", "be1", "A.java", 5),
        ];
        let out = external_shadow_internal_findings(&external, &provides);
        assert_eq!(out.len(), 1);
        let data = out[0].data.as_ref().unwrap();
        assert_eq!(data["matchedProvide"]["source"], "be1");
        assert_eq!(data["matchedProvide"]["file"], "A.java");
        assert_eq!(data["otherProvideCount"], 1);
    }

    #[test]
    fn consume_in_a_test_fixture_file_is_skipped() {
        let external = vec![consume(
            Some("GET https://app.internal.example.com/api/users"),
            "fe",
            "src/__tests__/Ctx.test.tsx",
            10,
        )];
        let provides = vec![provide("GET /api/users", "be", "Api.java", 20)];
        assert!(external_shadow_internal_findings(&external, &provides).is_empty());
    }

    #[test]
    fn determinism_multiple_findings_sorted_by_file_then_line() {
        let external = vec![
            consume(
                Some("GET https://app.internal.example.com/api/orders"),
                "fe",
                "Z.tsx",
                1,
            ),
            consume(
                Some("GET https://app.internal.example.com/api/users"),
                "fe",
                "A.tsx",
                5,
            ),
        ];
        let provides = vec![
            provide("GET /api/orders", "be", "Api.java", 1),
            provide("GET /api/users", "be", "Api.java", 2),
        ];
        let out = external_shadow_internal_findings(&external, &provides);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].file, "A.tsx");
        assert_eq!(out[1].file, "Z.tsx");
    }
}
