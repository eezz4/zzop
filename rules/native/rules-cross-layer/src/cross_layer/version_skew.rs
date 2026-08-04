//! `cross-layer/version-skew` (warning) — an `unprovided_consumes` http consume whose key differs from an existing
//! provide's key ONLY in one version-shaped path segment (`/v1/` vs `/v2/`, `/api/v3/...`). Same method,
//! same segment count, every other segment byte-identical — deliberately narrower than `path_near_miss`
//! (which allows `{}`-position drift too): this rule exists to name the specific, very common "caller still
//! points at the old API version" drift precisely, rather than lump it into the more general near-miss
//! bucket.

use zzop_core::io::TaggedConsume;
use zzop_core::{disable_hint, Finding, Severity};

use super::vocab::version_segment_re;
use super::{path_segments, split_key, HttpProvideSite};

/// `version_segment_pattern`: the run's declared `vocabulary.apiVersionSegmentPattern`, or `None` when
/// the author declared none (or declared one that will not parse). `None` means no segment is a version
/// segment, so no two paths are ever judged version-skewed — see [`super::vocab::version_segment_re`] for
/// why an absent declaration is never replaced with ours.
pub fn version_skew_findings(
    unprovided_consumes: &[TaggedConsume],
    all_provides: &[HttpProvideSite],
    version_segment_pattern: Option<&str>,
) -> Vec<Finding> {
    let version_re = version_segment_re(version_segment_pattern);

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

        let mut matches: Vec<(&HttpProvideSite, &str, &str)> = Vec::new();
        for p in all_provides {
            let Some((pmethod, ppath)) = split_key(&p.key) else {
                continue;
            };
            if pmethod != method {
                continue;
            }
            let provide_segs = path_segments(ppath);
            if provide_segs.len() != consume_segs.len() {
                continue;
            }
            let mut diff_idx: Option<usize> = None;
            let mut multiple_diffs = false;
            for (i, (cs, ps)) in consume_segs.iter().zip(provide_segs.iter()).enumerate() {
                if cs == ps {
                    continue;
                }
                if diff_idx.is_some() {
                    multiple_diffs = true;
                    break;
                }
                diff_idx = Some(i);
            }
            if multiple_diffs {
                continue;
            }
            let Some(i) = diff_idx else {
                continue; // identical path — should never be `unprovided`, but skip defensively.
            };
            let cs = consume_segs[i];
            let ps = provide_segs[i];
            if version_re
                .as_ref()
                .is_some_and(|re| re.is_match(cs) && re.is_match(ps))
            {
                matches.push((p, cs, ps));
            }
        }
        if matches.is_empty() {
            continue;
        }
        matches.sort_by(|a, b| {
            a.0.source
                .cmp(&b.0.source)
                .then(a.0.file.cmp(&b.0.file))
                .then(a.0.line.cmp(&b.0.line))
        });
        let (provide, consume_version, provide_version) = matches[0];

        let message = format!(
            "consume `{method} {path}` (source `{}`) requests version `{consume_version}`, but the matching \
             route is only provided at version `{provide_version}` — {}:{} (source `{}`, key `{}`). This looks \
             like the caller was not updated when the route moved to a new version. Point the caller at the \
             current version, or confirm the old version is intentionally still expected to exist somewhere. \
             {} if multiple API versions are intentionally served side by side and this caller is pinned to \
             an older one on purpose.",
            c.source, provide.file, provide.line, provide.source, provide.key,
            disable_hint("cross-layer/version-skew"),
        );
        out.push(Finding {
            rule_id: "cross-layer/version-skew".to_string(),
            severity: Severity::Warning,
            file: c.consume.file.clone(),
            line: c.consume.line,
            message,
            evidence_paths: vec![provide.file.clone()],
            data: Some(serde_json::json!({
                "consumeKey": key,
                "consumeSource": c.source,
                "consumeVersion": consume_version,
                "providedVersion": provide_version,
                "matchedProvide": {"source": provide.source, "file": provide.file, "line": provide.line, "key": provide.key},
            })),
        });
    }
    out.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cross_layer::VERSION_SEGMENT_PATTERN;

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
                retry_configured: None,
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
    fn version_only_difference_is_flagged_anchored_at_the_consume() {
        let unprovided_consumes = vec![consume(
            "http",
            Some("GET /api/v1/users"),
            "fe",
            "Ctx.tsx",
            5,
        )];
        let provides = vec![provide("GET /api/v2/users", "be", "Api.java", 30)];
        let out = version_skew_findings(
            &unprovided_consumes,
            &provides,
            Some(VERSION_SEGMENT_PATTERN),
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "cross-layer/version-skew");
        assert_eq!(out[0].severity, Severity::Warning);
        assert_eq!(out[0].file, "Ctx.tsx");
        assert_eq!(out[0].line, 5);
        assert!(out[0].message.contains("`v1`"));
        assert!(out[0].message.contains("`v2`"));
        assert!(out[0].message.contains("disabledRules"));
    }

    #[test]
    fn version_segment_plus_another_differing_segment_is_not_flagged() {
        // Only the version may differ — a second differing segment takes this out of scope for this rule
        // (it's a `path_near_miss`/unrelated-route candidate instead).
        let unprovided_consumes = vec![consume(
            "http",
            Some("GET /api/v1/users"),
            "fe",
            "Ctx.tsx",
            5,
        )];
        let provides = vec![provide("GET /api/v2/accounts", "be", "Api.java", 30)];
        assert!(version_skew_findings(
            &unprovided_consumes,
            &provides,
            Some(VERSION_SEGMENT_PATTERN)
        )
        .is_empty());
    }

    #[test]
    fn differing_method_is_not_flagged() {
        let unprovided_consumes = vec![consume(
            "http",
            Some("POST /api/v1/users"),
            "fe",
            "Ctx.tsx",
            5,
        )];
        let provides = vec![provide("GET /api/v2/users", "be", "Api.java", 30)];
        assert!(version_skew_findings(
            &unprovided_consumes,
            &provides,
            Some(VERSION_SEGMENT_PATTERN)
        )
        .is_empty());
    }

    #[test]
    fn non_version_shaped_differing_segment_is_not_flagged() {
        // "users" vs "orders" differ but neither looks like a version segment.
        let unprovided_consumes = vec![consume("http", Some("GET /api/users"), "fe", "Ctx.tsx", 5)];
        let provides = vec![provide("GET /api/orders", "be", "Api.java", 30)];
        assert!(version_skew_findings(
            &unprovided_consumes,
            &provides,
            Some(VERSION_SEGMENT_PATTERN)
        )
        .is_empty());
    }

    /// Seals the declared-pattern contract from both ends: a project vocabulary that spells versions
    /// `rel-1` sees the skew the built-in cannot, and a declaration that will not compile as a regex falls
    /// back to the built-in instead of panicking.
    #[test]
    fn a_declared_version_vocabulary_is_honored_and_an_uncompilable_one_falls_back() {
        let unprovided_consumes = vec![consume(
            "http",
            Some("GET /api/rel-1/users"),
            "fe",
            "Ctx.tsx",
            5,
        )];
        let provides = vec![provide("GET /api/rel-2/users", "be", "Api.java", 30)];
        assert!(version_skew_findings(
            &unprovided_consumes,
            &provides,
            Some(VERSION_SEGMENT_PATTERN)
        )
        .is_empty());
        let declared = r"(?i)^rel-[0-9]+$";
        assert_eq!(
            version_skew_findings(&unprovided_consumes, &provides, Some(declared)).len(),
            1
        );
        assert!(
            version_skew_findings(&unprovided_consumes, &provides, Some("(unclosed")).is_empty(),
            "an uncompilable declaration must match nothing — never panic, never match everything, \
             and never quietly become zzop's own pattern"
        );
        assert!(
            version_skew_findings(&unprovided_consumes, &provides, None).is_empty(),
            "with no declaration there is no version segment, so there is no skew to report"
        );
    }

    #[test]
    fn non_http_dangling_consume_is_ignored() {
        let unprovided_consumes = vec![consume(
            "queue",
            Some("GET /api/v1/users"),
            "fe",
            "Ctx.tsx",
            5,
        )];
        let provides = vec![provide("GET /api/v2/users", "be", "Api.java", 30)];
        assert!(version_skew_findings(
            &unprovided_consumes,
            &provides,
            Some(VERSION_SEGMENT_PATTERN)
        )
        .is_empty());
    }
}
