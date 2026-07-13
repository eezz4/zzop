//! `cross-layer/path-near-miss` (info) — an unprovided http consume whose key matches a provide's key after
//! allowing `{}` (parameter) positions to differ: same method, same segment count, every segment identical
//! or `{}` on one side. Deliberately strict beyond that — segments that merely look similar (`users` vs
//! `user`) do NOT count, since fuzzy-matching would turn this into a guesser rather than a precise "one side
//! generalized a parameter" signal. Info severity: lower confidence than a rule that pins down what differs.

use zzop_core::io::TaggedConsume;
use zzop_core::{disable_hint, Finding, Severity};

use super::{path_segments, split_key, HttpProvideSite};

pub fn path_near_miss_findings(
    unprovided_consumes: &[TaggedConsume],
    all_provides: &[HttpProvideSite],
) -> Vec<Finding> {
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
        let consume_segs = path_segments(path);

        let mut matches: Vec<&HttpProvideSite> = Vec::new();
        for p in all_provides {
            let Some((pmethod, ppath)) = split_key(&p.key) else {
                continue;
            };
            if pmethod != method || ppath == path {
                continue;
            }
            let provide_segs = path_segments(ppath);
            if provide_segs.len() != consume_segs.len() {
                continue;
            }
            let compatible = consume_segs
                .iter()
                .zip(provide_segs.iter())
                .all(|(cs, ps)| cs == ps || *cs == "{}" || *ps == "{}");
            if compatible {
                matches.push(p);
            }
        }
        if matches.is_empty() {
            continue;
        }
        matches.sort_by(|a, b| {
            a.source
                .cmp(&b.source)
                .then(a.file.cmp(&b.file))
                .then(a.line.cmp(&b.line))
        });
        let first = matches[0];
        let extra = matches.len() - 1;
        let extra_note = if extra > 0 {
            format!(" (and {extra} other near-miss route(s))")
        } else {
            String::new()
        };

        let message = format!(
            "consume `{method} {path}` (source `{}`) has no matching provide, but a same-shaped route \
             `{}` is provided at {}:{} (source `{}`){extra_note} — same method, same segment count, segments \
             equal except where one side uses a `{{}}` parameter. This could be a hardcoded literal where a \
             parameter was expected (or vice versa), or it could be two unrelated routes that happen to \
             share this shape — verify manually before treating this as drift. The consume-side method and \
             path reflect what static extraction read at the call site; a helper/wrapper around the call \
             can make them differ from the runtime request. {} if same-shaped-but-unrelated routes are \
             common in your stack.",
            c.source, first.key, first.file, first.line, first.source,
            disable_hint("cross-layer/path-near-miss"),
        );
        out.push(Finding {
            rule_id: "cross-layer/path-near-miss".to_string(),
            severity: Severity::Info,
            file: c.consume.file.clone(),
            line: c.consume.line,
            message,
            data: Some(serde_json::json!({
                "consumeKey": key,
                "consumeSource": c.source,
                "nearMissProvide": {"source": first.source, "file": first.file, "line": first.line, "key": first.key},
                "otherNearMissCount": extra,
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
            consume: zzop_core::IoConsume {
                client: None,
                body: None,
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
    fn param_position_vs_literal_is_a_near_miss() {
        let unprovided_consumes = vec![consume(
            "http",
            Some("GET /users/{}/profile"),
            "fe",
            "Ctx.tsx",
            7,
        )];
        let provides = vec![provide("GET /users/active/profile", "be", "Api.java", 14)];
        let out = path_near_miss_findings(&unprovided_consumes, &provides);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "cross-layer/path-near-miss");
        assert_eq!(out[0].severity, Severity::Info);
        assert_eq!(out[0].file, "Ctx.tsx");
        assert_eq!(out[0].line, 7);
        assert!(out[0].message.contains("GET /users/active/profile"));
        assert!(out[0].message.contains("Api.java:14"));
        assert!(out[0].message.contains("disabled_rules"));
    }

    #[test]
    fn plural_typo_literal_vs_literal_is_not_a_near_miss() {
        let unprovided_consumes = vec![consume(
            "http",
            Some("GET /users/{}/profile"),
            "fe",
            "Ctx.tsx",
            7,
        )];
        let provides = vec![provide("GET /user/{}/profile", "be", "Api.java", 14)];
        assert!(path_near_miss_findings(&unprovided_consumes, &provides).is_empty());
    }

    #[test]
    fn different_segment_count_is_not_a_near_miss() {
        let unprovided_consumes = vec![consume("http", Some("GET /users/{}"), "fe", "Ctx.tsx", 7)];
        let provides = vec![provide("GET /users/{}/profile", "be", "Api.java", 14)];
        assert!(path_near_miss_findings(&unprovided_consumes, &provides).is_empty());
    }

    #[test]
    fn differing_method_is_not_a_near_miss() {
        let unprovided_consumes = vec![consume(
            "http",
            Some("POST /users/{}/profile"),
            "fe",
            "Ctx.tsx",
            7,
        )];
        let provides = vec![provide("GET /users/active/profile", "be", "Api.java", 14)];
        assert!(path_near_miss_findings(&unprovided_consumes, &provides).is_empty());
    }

    #[test]
    fn non_http_dangling_consume_is_ignored() {
        let unprovided_consumes = vec![consume(
            "queue",
            Some("GET /users/{}/profile"),
            "fe",
            "Ctx.tsx",
            7,
        )];
        let provides = vec![provide("GET /users/active/profile", "be", "Api.java", 14)];
        assert!(path_near_miss_findings(&unprovided_consumes, &provides).is_empty());
    }
}
