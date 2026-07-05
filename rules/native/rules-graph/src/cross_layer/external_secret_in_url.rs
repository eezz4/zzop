//! `cross-layer/external-secret-in-url` (warning) — an external HTTP consume whose query string carries a
//! secret-shaped parameter name (`token`, `api_key`, `password`, ...). Fires for both a literal value and an
//! interpolated `{}` placeholder: either way the parameter transits the URL, and URLs are captured by
//! proxy/CDN/access logs and leak through the `Referer` header and browser history, so a dynamic secret is
//! not materially safer than a literal one (`hasLiteralValue` in the finding data just records which case it
//! was; it never gates whether the rule fires). Anchored at the consume site, since that's where the fix
//! (move the parameter to a header or the request body) lands.

use zpz_core::io::TaggedConsume;
use zpz_core::{Finding, Severity};

use super::split_external_key;

/// Query parameter names (already lowercased) that commonly carry a secret/credential value.
const SECRET_PARAM_NAMES: &[&str] = &[
    "token",
    "access_token",
    "accesstoken",
    "apikey",
    "api_key",
    "api-key",
    "api_token",
    "apitoken",
    "key",
    "secret",
    "client_secret",
    "password",
    "auth",
    "signature",
    "jwt",
];

pub fn external_secret_in_url_findings(external_consumes: &[TaggedConsume]) -> Vec<Finding> {
    let mut out = Vec::new();
    for c in external_consumes
        .iter()
        .filter(|c| c.consume.kind == "http")
    {
        let Some(key) = c.consume.key.as_deref() else {
            continue;
        };
        let Some(url) = split_external_key(key) else {
            continue;
        };
        let Some(query) = url.query else {
            continue;
        };

        let mut matched: Vec<String> = Vec::new();
        let mut has_literal_value = false;
        for pair in query.split('&') {
            if pair.is_empty() {
                continue;
            }
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            let lower_name = name.to_ascii_lowercase();
            if !SECRET_PARAM_NAMES.contains(&lower_name.as_str()) {
                continue;
            }
            matched.push(lower_name);
            if !value.is_empty() && value != "{}" {
                has_literal_value = true;
            }
        }
        if matched.is_empty() {
            continue;
        }
        matched.sort();
        matched.dedup();
        let params_list = matched.join(", ");

        let message = format!(
            "external call `{} {}{}` (source `{}`) puts secret-shaped query parameter(s) `{params_list}` on \
             the URL. Query strings are captured by proxy/CDN/access logs and leak through the `Referer` \
             header and browser history — that's true whether the value is a literal secret or an \
             interpolated `{{}}` one, so both cases are flagged here. Move `{params_list}` to a request header \
             (e.g. `Authorization`) or the request body instead of a URL query parameter. Disable via rule \
             config `disabled_rules: [\"cross-layer/external-secret-in-url\"]` if this parameter name is a \
             false positive for this integration (e.g. a non-secret lookup id that just happens to share a \
             name like `key`).",
            url.method, url.host, url.path, c.source,
        );

        out.push(Finding {
            rule_id: "cross-layer/external-secret-in-url".to_string(),
            severity: Severity::Warning,
            file: c.consume.file.clone(),
            line: c.consume.line,
            message,
            data: Some(serde_json::json!({
                "key": key,
                "host": url.host,
                "matchedParams": matched,
                "hasLiteralValue": has_literal_value,
                "consumeSource": c.source,
            })),
        });
    }
    out.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn consume(
        kind: &str,
        key: Option<&str>,
        source: &str,
        file: &str,
        line: u32,
    ) -> TaggedConsume {
        TaggedConsume {
            source: source.to_string(),
            consume: zpz_core::IoConsume {
                kind: kind.to_string(),
                key: key.map(str::to_string),
                file: file.to_string(),
                line,
                raw: None,
                method: None,
            },
        }
    }

    #[test]
    fn literal_secret_value_in_query_is_flagged() {
        let external = vec![consume(
            "http",
            Some("GET https://api.vendor.com/v1/users?token=abc123"),
            "fe",
            "Client.ts",
            10,
        )];
        let out = external_secret_in_url_findings(&external);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "cross-layer/external-secret-in-url");
        assert_eq!(out[0].severity, Severity::Warning);
        assert_eq!(out[0].file, "Client.ts");
        assert_eq!(out[0].line, 10);
        assert!(out[0].message.contains("`token`"));
        assert!(out[0].message.contains("disabled_rules"));
        let data = out[0].data.as_ref().unwrap();
        assert_eq!(data["hasLiteralValue"], true);
        assert_eq!(data["host"], "api.vendor.com");
    }

    #[test]
    fn interpolated_placeholder_secret_value_still_fires() {
        // A dynamic secret still transits the URL — query strings hit logs/referrers/history either way.
        let external = vec![consume(
            "http",
            Some("GET https://api.vendor.com/v1/users?api_key={}"),
            "fe",
            "Client.ts",
            10,
        )];
        let out = external_secret_in_url_findings(&external);
        assert_eq!(out.len(), 1);
        let data = out[0].data.as_ref().unwrap();
        assert_eq!(data["hasLiteralValue"], false);
        assert_eq!(data["matchedParams"], serde_json::json!(["api_key"]));
    }

    #[test]
    fn no_query_string_is_not_flagged() {
        let external = vec![consume(
            "http",
            Some("GET https://api.vendor.com/v1/users"),
            "fe",
            "Client.ts",
            10,
        )];
        assert!(external_secret_in_url_findings(&external).is_empty());
    }

    #[test]
    fn non_secret_query_params_are_not_flagged() {
        let external = vec![consume(
            "http",
            Some("GET https://api.vendor.com/v1/users?page=2&sort=name"),
            "fe",
            "Client.ts",
            10,
        )];
        assert!(external_secret_in_url_findings(&external).is_empty());
    }

    #[test]
    fn non_http_kind_is_ignored() {
        let external = vec![consume(
            "queue",
            Some("GET https://api.vendor.com/v1/users?token=abc123"),
            "fe",
            "Client.ts",
            10,
        )];
        assert!(external_secret_in_url_findings(&external).is_empty());
    }

    #[test]
    fn multiple_matched_params_are_listed_sorted_and_deduped() {
        let external = vec![consume(
            "http",
            Some("GET https://api.vendor.com/v1/users?Token=abc&password=x&token=abc"),
            "fe",
            "Client.ts",
            10,
        )];
        let out = external_secret_in_url_findings(&external);
        assert_eq!(out.len(), 1);
        let data = out[0].data.as_ref().unwrap();
        assert_eq!(
            data["matchedParams"],
            serde_json::json!(["password", "token"])
        );
    }

    #[test]
    fn findings_are_sorted_deterministically_by_file_then_line() {
        let external = vec![
            consume(
                "http",
                Some("GET https://b.vendor.com/v1/x?token=1"),
                "fe",
                "b.ts",
                5,
            ),
            consume(
                "http",
                Some("GET https://a.vendor.com/v1/x?token=1"),
                "fe",
                "a.ts",
                20,
            ),
            consume(
                "http",
                Some("GET https://a.vendor.com/v1/y?token=1"),
                "fe",
                "a.ts",
                3,
            ),
        ];
        let out = external_secret_in_url_findings(&external);
        assert_eq!(out.len(), 3);
        assert_eq!((out[0].file.as_str(), out[0].line), ("a.ts", 3));
        assert_eq!((out[1].file.as_str(), out[1].line), ("a.ts", 20));
        assert_eq!((out[2].file.as_str(), out[2].line), ("b.ts", 5));
    }
}
