//! `cross-layer/method-mismatch` (warning) — an unprovided http consume whose PATH exactly matches a provide
//! elsewhere in the analysis, but whose METHOD differs (e.g. FE calls `POST /api/users`, only
//! `GET /api/users` is provided). Anchored at the consume, since that's where the fix (correct the method, or
//! add the missing handler) lands. Distinct from `version_skew`/`path_near_miss`, which require the path
//! itself to differ — this one requires the path to be byte-identical.

use std::collections::BTreeMap;

use zpz_core::io::TaggedConsume;
use zpz_core::{Finding, Severity};

use super::{split_key, HttpProvideSite};

pub fn method_mismatch_findings(
    unprovided_consumes: &[TaggedConsume],
    all_provides: &[HttpProvideSite],
) -> Vec<Finding> {
    let mut by_path: BTreeMap<&str, Vec<&HttpProvideSite>> = BTreeMap::new();
    for p in all_provides {
        if let Some((_, path)) = split_key(&p.key) {
            by_path.entry(path).or_default().push(p);
        }
    }

    let mut out = Vec::new();
    for c in unprovided_consumes
        .iter()
        .filter(|c| c.consume.kind == "http")
    {
        let Some(key) = c.consume.key.as_deref() else {
            continue;
        };
        let Some((method, path)) = split_key(key) else {
            continue;
        };
        let Some(candidates) = by_path.get(path) else {
            continue;
        };
        let mut others: Vec<&HttpProvideSite> = candidates
            .iter()
            .copied()
            .filter(|p| split_key(&p.key).map(|(m, _)| m) != Some(method))
            .collect();
        if others.is_empty() {
            continue;
        }
        others.sort_by(|a, b| {
            a.source
                .cmp(&b.source)
                .then(a.file.cmp(&b.file))
                .then(a.line.cmp(&b.line))
        });
        let first = others[0];
        let first_method = split_key(&first.key).map(|(m, _)| m).unwrap_or("");
        let mut other_methods: Vec<&str> = others
            .iter()
            .filter_map(|p| split_key(&p.key).map(|(m, _)| m))
            .collect();
        other_methods.sort();
        other_methods.dedup();

        let message = format!(
            "consume `{method} {path}` (source `{}`) has no matching provide, but `{path}` IS provided with a \
             different method ({}) — e.g. at {}:{} (source `{}`, method `{first_method}`). This looks like a \
             method typo/drift between caller and route registration rather than a missing route entirely. \
             Verify the intended HTTP method on either side and fix the mismatched one. The consume-side \
             method reflects what static extraction read at the call site — if the call goes through a \
             helper/wrapper (multipart, a custom fetch wrapper, ...), verify the literal method manually \
             before changing either side. Disable via rule \
             config `disabled_rules: [\"cross-layer/method-mismatch\"]` if this path legitimately supports \
             multiple methods registered as separate routes and the caller's method is simply not one of \
             them yet.",
            c.source,
            other_methods.join(", "),
            first.file,
            first.line,
            first.source,
        );
        out.push(Finding {
            rule_id: "cross-layer/method-mismatch".to_string(),
            severity: Severity::Warning,
            file: c.consume.file.clone(),
            line: c.consume.line,
            message,
            data: Some(serde_json::json!({
                "consumeKey": key,
                "consumeSource": c.source,
                "path": path,
                "consumeMethod": method,
                "providedMethods": other_methods,
                "exampleProvide": {"source": first.source, "file": first.file, "line": first.line},
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

    fn provide(key: &str, source: &str, file: &str, line: u32) -> HttpProvideSite {
        HttpProvideSite {
            source: source.to_string(),
            key: key.to_string(),
            file: file.to_string(),
            line,
        }
    }

    #[test]
    fn same_path_different_method_is_flagged_anchored_at_the_consume() {
        let unprovided_consumes = vec![consume(
            "http",
            Some("POST /api/users"),
            "fe",
            "Ctx.tsx",
            10,
        )];
        let provides = vec![provide("GET /api/users", "be", "Api.java", 20)];
        let out = method_mismatch_findings(&unprovided_consumes, &provides);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "cross-layer/method-mismatch");
        assert_eq!(out[0].severity, Severity::Warning);
        assert_eq!(out[0].file, "Ctx.tsx");
        assert_eq!(out[0].line, 10);
        assert!(out[0].message.contains("POST /api/users"));
        assert!(out[0].message.contains("GET"));
        assert!(out[0].message.contains("Api.java:20"));
        assert!(out[0].message.contains("disabled_rules"));
    }

    #[test]
    fn same_path_same_method_is_not_a_mismatch() {
        // Would have joined in a real pipeline; checks the function alone doesn't false-positive on this shape.
        let unprovided_consumes =
            vec![consume("http", Some("GET /api/users"), "fe", "Ctx.tsx", 10)];
        let provides = vec![provide("GET /api/users", "be", "Api.java", 20)];
        assert!(method_mismatch_findings(&unprovided_consumes, &provides).is_empty());
    }

    #[test]
    fn different_path_is_not_flagged_by_this_rule() {
        let unprovided_consumes = vec![consume(
            "http",
            Some("POST /api/orders"),
            "fe",
            "Ctx.tsx",
            10,
        )];
        let provides = vec![provide("GET /api/users", "be", "Api.java", 20)];
        assert!(method_mismatch_findings(&unprovided_consumes, &provides).is_empty());
    }

    #[test]
    fn non_http_dangling_consume_is_ignored() {
        let unprovided_consumes = vec![consume(
            "queue",
            Some("POST /api/users"),
            "fe",
            "Ctx.tsx",
            10,
        )];
        let provides = vec![provide("GET /api/users", "be", "Api.java", 20)];
        assert!(method_mismatch_findings(&unprovided_consumes, &provides).is_empty());
    }

    #[test]
    fn multiple_other_methods_are_all_named_and_anchored_at_the_first_sorted_site() {
        let unprovided_consumes = vec![consume(
            "http",
            Some("DELETE /api/users"),
            "fe",
            "Ctx.tsx",
            1,
        )];
        let provides = vec![
            provide("PUT /api/users", "be", "b.java", 5),
            provide("GET /api/users", "be", "a.java", 2),
        ];
        let out = method_mismatch_findings(&unprovided_consumes, &provides);
        assert_eq!(out.len(), 1);
        assert!(out[0].message.contains("GET, PUT"));
        let data = out[0].data.as_ref().unwrap();
        assert_eq!(data["exampleProvide"]["file"], "a.java");
    }
}
