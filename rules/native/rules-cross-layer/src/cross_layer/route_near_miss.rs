//! `cross-layer/route-near-miss` (info) — an unprovided http consume whose key differs from a same-method
//! provide by EXACTLY ONE structural dimension: `case` (a path segment differs only in letter casing) or
//! `prefix` (the shorter path is an exact suffix of the longer, with a 1-2 all-literal leading prefix
//! added/removed — a base-path like `/api` or `/api/v1`, the classic `setGlobalPrefix` drift). Deliberately
//! disjoint from `path_near_miss`: that rule owns the SAME segment count, every-segment-equal-or-`{}` case
//! (pure parameter generalization) — this rule never fires on a pair `path_near_miss` would already explain,
//! and neither dimension here can produce a `path_near_miss`-shaped pair (case requires an exact-case
//! mismatch on a same-length pair, which `path_near_miss`'s equal-or-`{}` test already rejects; prefix
//! requires a segment-count difference, which `path_near_miss` requires to be absent).
//!
//! Info severity (same as the sibling `path_near_miss`): these are honest "verify manually" near-misses, not
//! confirmed drift — a same-method one-dimension-apart provide is strong evidence but a wrapper can make the
//! runtime request differ from the call site, and unrelated routes can be one dimension apart by coincidence.
//! Info keeps the rule FP-safe while still surfacing the near-miss in the cross-repo report.
//!
//! An earlier draft carried a third `arity` dimension (literal segments equal, `{}` parameter count differs).
//! It was dropped: literal-segments-equal-plus-differing-`{}`-count is dominated by legitimate REST
//! collection/item and nested-resource shapes (`GET /users` vs `GET /users/{}`, `GET /{orgId}` vs
//! `GET /{orgId}/{repoId}`) — those are distinct endpoints, not near-misses — so arity was mostly-FP even at
//! Info.

use zzop_core::io::TaggedConsume;
use zzop_core::{disable_hint, Finding, Severity};

use super::{path_segments, split_key, HttpProvideSite};

/// The one structural dimension a route-near-miss pair differs by, in priority order (`case` > `prefix`,
/// most to least confident — see module doc).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dimension {
    Case,
    Prefix,
}

impl Dimension {
    fn as_str(self) -> &'static str {
        match self {
            Dimension::Case => "case",
            Dimension::Prefix => "prefix",
        }
    }
}

/// Whether `consume_segs`/`provide_segs` form a pair `path_near_miss` already owns: same segment count,
/// every segment either exactly equal or `{}` on one side (pure parameter generalization). Used to keep this
/// rule strictly disjoint from `path_near_miss`, even though — see the module doc — neither dimension below
/// can actually produce such a pair; kept as an explicit guard so that invariant is checked, not just
/// reasoned about.
fn is_path_near_miss_pair(consume_segs: &[&str], provide_segs: &[&str]) -> bool {
    consume_segs.len() == provide_segs.len()
        && consume_segs
            .iter()
            .zip(provide_segs.iter())
            .all(|(cs, ps)| cs == ps || *cs == "{}" || *ps == "{}")
}

/// `case` dimension: same segment count, every segment equal case-insensitively, and at least one segment
/// differs case-sensitively (otherwise the pair is an exact match, which cannot appear among unprovided
/// consumes in the first place). A segment that is `{}` on one side can never satisfy
/// case-insensitive-equality against a literal segment, so this can never overlap with `path_near_miss`'s
/// parameter-generalization case.
fn case_dimension_match(consume_segs: &[&str], provide_segs: &[&str]) -> bool {
    if consume_segs.len() != provide_segs.len() {
        return false;
    }
    let mut any_case_diff = false;
    for (cs, ps) in consume_segs.iter().zip(provide_segs.iter()) {
        if cs.to_lowercase() != ps.to_lowercase() {
            return false;
        }
        if cs != ps {
            any_case_diff = true;
        }
    }
    any_case_diff
}

/// `prefix` dimension: the shorter path's segments are an exact (case-sensitive, `{}`-included) suffix of
/// the longer path's segments, and the leading run of segments added/removed is 1 or 2 AND all-literal (no
/// `{}`). Two guards against unrelated-route false matches: a real base prefix like `/api` or `/api/v1` is
/// short (1-2 segments), and a `{}` parameter is never a base path — `GET /articles` vs `GET /{}/articles`
/// must NOT fire, so the leading diff run must contain no `{}` (the shared suffix may still contain `{}`, as
/// an exact segment match). Returns the 1/2 leading segments (the "prefix") on a match, so the caller can
/// report exactly what differs.
fn prefix_dimension_match<'a>(
    consume_segs: &[&'a str],
    provide_segs: &[&'a str],
) -> Option<Vec<&'a str>> {
    let (shorter, longer) = if consume_segs.len() <= provide_segs.len() {
        (consume_segs, provide_segs)
    } else {
        (provide_segs, consume_segs)
    };
    let diff = longer.len() - shorter.len();
    if diff == 0 || diff > 2 || shorter.is_empty() {
        return None;
    }
    if longer[diff..] != *shorter {
        return None;
    }
    let leading = &longer[..diff];
    // The added/removed leading run must be an all-literal base path; a `{}` parameter is not a prefix.
    if leading.contains(&"{}") {
        return None;
    }
    Some(leading.to_vec())
}

fn provide_path_segs(p: &HttpProvideSite) -> Option<(&str, Vec<&str>)> {
    let (pmethod, ppath) = split_key(&p.key)?;
    Some((pmethod, path_segments(ppath)))
}

pub fn route_near_miss_findings(
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

        // Priority order: case > prefix. As soon as one dimension has at least one candidate, stop — the
        // lower-priority dimension never gets consulted for this consume.
        let mut chosen: Option<(Dimension, Vec<&HttpProvideSite>)> = None;
        for dimension in [Dimension::Case, Dimension::Prefix] {
            let mut candidates: Vec<&HttpProvideSite> = Vec::new();
            for p in all_provides {
                let Some((pmethod, provide_segs)) = provide_path_segs(p) else {
                    continue;
                };
                if pmethod != method {
                    continue;
                }
                if is_path_near_miss_pair(&consume_segs, &provide_segs) {
                    continue;
                }
                let matched = match dimension {
                    Dimension::Case => case_dimension_match(&consume_segs, &provide_segs),
                    Dimension::Prefix => {
                        prefix_dimension_match(&consume_segs, &provide_segs).is_some()
                    }
                };
                if matched {
                    candidates.push(p);
                }
            }
            if !candidates.is_empty() {
                chosen = Some((dimension, candidates));
                break;
            }
        }

        let Some((dimension, mut candidates)) = chosen else {
            continue;
        };
        candidates.sort_by(|a, b| {
            a.source
                .cmp(&b.source)
                .then(a.file.cmp(&b.file))
                .then(a.line.cmp(&b.line))
        });
        let first = candidates[0];
        let extra = candidates.len() - 1;
        let extra_note = if extra > 0 {
            format!(" (and {extra} other near-miss route(s))")
        } else {
            String::new()
        };

        // Recompute the dimension-specific detail from `first` itself (not from whatever candidate was
        // encountered first during the scan above) — with multiple candidates on the same dimension, they
        // can carry different prefixes, so the message must describe the ACTUAL chosen provide.
        let dimension_detail = match dimension {
            Dimension::Case => "differs only by path segment letter casing — the segments match \
                 case-insensitively but not case-sensitively"
                .to_string(),
            Dimension::Prefix => {
                let first_provide_segs =
                    provide_path_segs(first).map(|(_, s)| s).unwrap_or_default();
                let prefix_segments = prefix_dimension_match(&consume_segs, &first_provide_segs)
                    .expect("first was selected as a prefix-dimension candidate");
                let prefix_str = format!("/{}", prefix_segments.join("/"));
                if consume_segs.len() < first_provide_segs.len() {
                    format!(
                        "differs only by a missing path prefix (`{prefix_str}`) — the consume is missing a \
                         leading segment the provide has"
                    )
                } else {
                    format!(
                        "differs only by an extra path prefix (`{prefix_str}`) — the consume carries a \
                         leading segment the provide does not have"
                    )
                }
            }
        };

        let message = format!(
            "consume `{method} {path}` (source `{}`) has no exact provider, but `{}` provides `{}` at \
             {}:{}{extra_note} — {dimension_detail}. This could be genuine route drift (align the call path \
             with the served route, or vice versa), or two unrelated routes that happen to be one dimension \
             apart — verify manually before treating this as drift. The consume-side method and path reflect \
             what static extraction read at the call site; a helper/wrapper around the call can make them \
             differ from the runtime request. {} if one-dimension-apart-but-unrelated routes are common in \
             your stack.",
            c.source, first.source, first.key, first.file, first.line,
            disable_hint("cross-layer/route-near-miss"),
        );

        out.push(Finding {
            rule_id: "cross-layer/route-near-miss".to_string(),
            severity: Severity::Info,
            file: c.consume.file.clone(),
            line: c.consume.line,
            message,
            data: Some(serde_json::json!({
                "consumeKey": key,
                "consumeSource": c.source,
                "dimension": dimension.as_str(),
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
    fn case_dimension_fires() {
        let unprovided_consumes = vec![consume(
            "http",
            Some("GET /api/Articles"),
            "fe-react",
            "Api.tsx",
            10,
        )];
        let provides = vec![provide(
            "GET /api/articles",
            "be-nest",
            "articles.controller.ts",
            22,
        )];
        let out = route_near_miss_findings(&unprovided_consumes, &provides);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "cross-layer/route-near-miss");
        assert_eq!(out[0].severity, Severity::Info);
        assert_eq!(out[0].file, "Api.tsx");
        assert_eq!(out[0].line, 10);
        assert_eq!(out[0].data.as_ref().unwrap()["dimension"], "case");
        assert!(out[0].message.contains("casing"));
        assert!(out[0].message.contains("disabled_rules"));
        assert!(out[0].message.contains("articles.controller.ts:22"));
    }

    #[test]
    fn prefix_dimension_fires_when_consume_is_missing_the_prefix() {
        // The canonical NestJS setGlobalPrefix('api') drift — a 1-segment shared suffix. MUST fire.
        let unprovided_consumes = vec![consume(
            "http",
            Some("GET /articles"),
            "fe-react",
            "Api.tsx",
            10,
        )];
        let provides = vec![provide(
            "GET /api/articles",
            "be-nest",
            "articles.controller.ts",
            22,
        )];
        let out = route_near_miss_findings(&unprovided_consumes, &provides);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].data.as_ref().unwrap()["dimension"], "prefix");
        assert!(out[0].message.contains("missing path prefix"));
        assert!(out[0].message.contains("/api"));
    }

    #[test]
    fn prefix_dimension_fires_when_consume_carries_the_extra_prefix() {
        let unprovided_consumes = vec![consume(
            "http",
            Some("GET /api/articles"),
            "fe-react",
            "Api.tsx",
            10,
        )];
        let provides = vec![provide(
            "GET /articles",
            "be-nest",
            "articles.controller.ts",
            22,
        )];
        let out = route_near_miss_findings(&unprovided_consumes, &provides);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].data.as_ref().unwrap()["dimension"], "prefix");
        assert!(out[0].message.contains("extra path prefix"));
        assert!(out[0].message.contains("/api"));
    }

    #[test]
    fn prefix_dimension_single_short_segment_suffix_is_accepted_low_confidence() {
        // `/me` vs `/api/me` — a 1-segment shared suffix. Intentionally accepted at Info: low-confidence
        // but the base-prefix shape is real. Pinned here so a future tightening is a visible change.
        let unprovided_consumes = vec![consume("http", Some("GET /me"), "fe-react", "Api.tsx", 10)];
        let provides = vec![provide("GET /api/me", "be-nest", "me.controller.ts", 22)];
        let out = route_near_miss_findings(&unprovided_consumes, &provides);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].data.as_ref().unwrap()["dimension"], "prefix");
    }

    #[test]
    fn prefix_dimension_param_leading_run_is_excluded() {
        // `/articles` vs `/{}/articles` — the added leading run is a `{}` parameter, not a base path. Must
        // NOT fire (FIX 3): a parameter segment is never a prefix.
        let unprovided_consumes = vec![consume(
            "http",
            Some("GET /articles"),
            "fe-react",
            "Api.tsx",
            10,
        )];
        let provides = vec![provide(
            "GET /{}/articles",
            "be-nest",
            "articles.controller.ts",
            22,
        )];
        assert!(route_near_miss_findings(&unprovided_consumes, &provides).is_empty());
    }

    #[test]
    fn prefix_dimension_does_not_fire_when_added_prefix_is_too_long() {
        // `/x` vs `/a/b/c/x` — a 3-segment added prefix exceeds the 1-2 base-path bound. Must NOT fire.
        let unprovided_consumes = vec![consume("http", Some("GET /x"), "fe-react", "Api.tsx", 10)];
        let provides = vec![provide("GET /a/b/c/x", "be-nest", "x.controller.ts", 22)];
        assert!(route_near_miss_findings(&unprovided_consumes, &provides).is_empty());
    }

    #[test]
    fn prefix_dimension_does_not_fire_when_suffix_does_not_match() {
        let unprovided_consumes = vec![consume(
            "http",
            Some("GET /widgets"),
            "fe-react",
            "Api.tsx",
            10,
        )];
        let provides = vec![provide(
            "GET /api/articles",
            "be-nest",
            "articles.controller.ts",
            22,
        )];
        assert!(route_near_miss_findings(&unprovided_consumes, &provides).is_empty());
    }

    #[test]
    fn arity_shaped_pair_no_longer_fires() {
        // Literal segments equal, differing `{}` count (`/api/articles/{}/{}` vs `/api/articles/{}`) — the
        // former `arity` dimension. Dropped as mostly-FP (REST collection/item shapes), so nothing fires.
        let unprovided_consumes = vec![consume(
            "http",
            Some("GET /api/articles/{}/{}"),
            "fe-react",
            "Api.tsx",
            10,
        )];
        let provides = vec![provide(
            "GET /api/articles/{}",
            "be-nest",
            "articles.controller.ts",
            22,
        )];
        assert!(route_near_miss_findings(&unprovided_consumes, &provides).is_empty());
    }

    #[test]
    fn pure_param_generalization_pair_is_disjoint_from_path_near_miss() {
        // Same segment count, every segment equal-or-`{}` — this is path_near_miss's pair shape, not
        // route-near-miss's; route-near-miss must stay silent on it.
        let unprovided_consumes = vec![consume(
            "http",
            Some("GET /users/{}/profile"),
            "fe-react",
            "Api.tsx",
            10,
        )];
        let provides = vec![provide(
            "GET /users/active/profile",
            "be-nest",
            "users.controller.ts",
            22,
        )];
        assert!(route_near_miss_findings(&unprovided_consumes, &provides).is_empty());
    }

    #[test]
    fn differing_method_is_not_a_near_miss() {
        let unprovided_consumes = vec![consume(
            "http",
            Some("POST /api/Articles"),
            "fe-react",
            "Api.tsx",
            10,
        )];
        let provides = vec![provide(
            "GET /api/articles",
            "be-nest",
            "articles.controller.ts",
            22,
        )];
        assert!(route_near_miss_findings(&unprovided_consumes, &provides).is_empty());
    }

    #[test]
    fn genuinely_unrelated_routes_do_not_fire() {
        let unprovided_consumes = vec![consume(
            "http",
            Some("GET /widgets/{}"),
            "fe-react",
            "Api.tsx",
            10,
        )];
        let provides = vec![provide(
            "GET /api/articles",
            "be-nest",
            "articles.controller.ts",
            22,
        )];
        assert!(route_near_miss_findings(&unprovided_consumes, &provides).is_empty());
    }

    #[test]
    fn non_http_dangling_consume_is_ignored() {
        let unprovided_consumes = vec![consume(
            "queue",
            Some("GET /api/Articles"),
            "fe-react",
            "Api.tsx",
            10,
        )];
        let provides = vec![provide(
            "GET /api/articles",
            "be-nest",
            "articles.controller.ts",
            22,
        )];
        assert!(route_near_miss_findings(&unprovided_consumes, &provides).is_empty());
    }

    #[test]
    fn priority_prefers_case_over_prefix() {
        // One provide matches by case, another by prefix — case wins (higher confidence).
        let unprovided_consumes = vec![consume(
            "http",
            Some("GET /api/Articles"),
            "fe-react",
            "Api.tsx",
            10,
        )];
        let provides = vec![
            // Prefix match: consume carries an extra `/api` the provide lacks.
            provide("GET /Articles", "be-nest", "prefix.ts", 5),
            // Case match: same segment count, differs only in casing.
            provide("GET /api/articles", "be-nest", "case.ts", 9),
        ];
        let out = route_near_miss_findings(&unprovided_consumes, &provides);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].data.as_ref().unwrap()["dimension"], "case");
        assert!(out[0].message.contains("case.ts:9"));
    }

    #[test]
    fn determinism_stable_sorted_output() {
        let unprovided_consumes = vec![
            consume("http", Some("GET /api/Articles"), "fe-react", "B.tsx", 2),
            consume("http", Some("GET /api/Comments"), "fe-react", "A.tsx", 1),
        ];
        let provides = vec![
            provide("GET /api/articles", "be-nest", "articles.controller.ts", 22),
            provide("GET /api/comments", "be-nest", "comments.controller.ts", 8),
        ];
        let out = route_near_miss_findings(&unprovided_consumes, &provides);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].file, "A.tsx");
        assert_eq!(out[1].file, "B.tsx");

        // Re-running yields byte-identical output (candidate sort is deterministic too).
        let out2 = route_near_miss_findings(&unprovided_consumes, &provides);
        assert_eq!(
            out.iter().map(|f| &f.message).collect::<Vec<_>>(),
            out2.iter().map(|f| &f.message).collect::<Vec<_>>()
        );
    }
}
