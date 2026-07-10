//! `cross-layer/route-shadowing` (warning) — a `{}`-pattern route PROVIDED by one source tree would shadow a
//! same-method, same-segment-count LITERAL route provided by a DIFFERENT tree, IF both trees are served
//! behind one first-match gateway/router (e.g. tree A provides `GET /users/{}`, tree B provides
//! `GET /users/export` — a gateway trying A's route table first would swallow B's literal route before a
//! request ever reaches tree B).
//!
//! ## No overlap with the single-tree `route-shadowing` rule
//! `zzop_rules_http::route_shadowing` compares routes within ONE file, using registration order as the first-match
//! signal — that only makes sense within a single router instance. This rule fires ONLY across DIFFERENT
//! source trees, where there is no shared registration order to reason about (each tree's internal route
//! order is invisible to the other), so it ignores line order entirely and flags the shape overlap itself,
//! honestly conditioned on "IF served behind a shared first-match gateway" — deploy topology this static
//! analysis cannot see. The two rules partition the shadowing space by scope (same-source vs. cross-source)
//! and never double-fire on the same pair.
//!
//! ## Decidable subset
//! - provides registered in a test/fixture file are skipped (`zzop_core::is_test_file`) — not real
//!   deployed surface;
//! - same HTTP method, same segment count;
//! - every segment pairwise equal, OR the pattern's segment at that position is `{}` (any number of `{}`
//!   positions qualify — with no registration order to narrow against, a cross-tree pattern can shadow via
//!   multiple param segments at once);
//! - the two paths are not byte-identical (defensive — the segment-equality test alone doesn't rule out a
//!   pathological literal `{}` path segment);
//! - `source` differs between the pattern's provide and the literal's provide.
//!
//! One finding per shadowing PATTERN (not one per literal it shadows) — anchored at the pattern's own provide
//! site, since that route's registration is what would need to move/narrow to fix the shape. Every literal it
//! shadows is listed in `data.shadowed` (capped at 5, full count in `data.shadowedCount`).

use std::collections::BTreeMap;

use super::{path_segments, split_key, HttpProvideSite};
use zzop_core::is_test_file;

pub fn cross_tree_route_shadowing_findings(
    all_provides: &[HttpProvideSite],
) -> Vec<zzop_core::Finding> {
    struct Literal<'a> {
        site: &'a HttpProvideSite,
        segs: Vec<&'a str>,
    }

    let mut patterns: Vec<&HttpProvideSite> = Vec::new();
    let mut literals_by_method_count: BTreeMap<(&str, usize), Vec<Literal>> = BTreeMap::new();

    for p in all_provides {
        if is_test_file(&p.file) {
            continue;
        }
        let Some((method, path)) = split_key(&p.key) else {
            continue;
        };
        let segs = path_segments(path);
        if segs.contains(&"{}") {
            patterns.push(p);
        } else {
            literals_by_method_count
                .entry((method, segs.len()))
                .or_default()
                .push(Literal { site: p, segs });
        }
    }

    patterns.sort_by(|a, b| {
        a.source
            .cmp(&b.source)
            .then(a.file.cmp(&b.file))
            .then(a.line.cmp(&b.line))
            .then(a.key.cmp(&b.key))
    });

    let mut out = Vec::new();
    for pattern in patterns {
        let Some((method, path)) = split_key(&pattern.key) else {
            continue;
        };
        let pattern_segs = path_segments(path);
        let Some(candidates) = literals_by_method_count.get(&(method, pattern_segs.len())) else {
            continue;
        };

        let mut shadowed: Vec<&HttpProvideSite> = Vec::new();
        for lit in candidates {
            if lit.site.source == pattern.source {
                continue;
            }
            if lit.site.key == pattern.key {
                continue; // byte-identical paths — defensively excluded, see module doc
            }
            let matches = pattern_segs
                .iter()
                .zip(lit.segs.iter())
                .all(|(p, l)| *p == *l || *p == "{}");
            if matches {
                shadowed.push(lit.site);
            }
        }
        if shadowed.is_empty() {
            continue;
        }
        shadowed.sort_by(|a, b| {
            a.source
                .cmp(&b.source)
                .then(a.file.cmp(&b.file))
                .then(a.line.cmp(&b.line))
        });

        let shadowed_count = shadowed.len();
        let sample: Vec<serde_json::Value> = shadowed
            .iter()
            .take(5)
            .map(|s| {
                serde_json::json!({
                    "key": s.key,
                    "source": s.source,
                    "file": s.file,
                    "line": s.line,
                })
            })
            .collect();
        let example = shadowed[0];

        let message = format!(
            "route `{}` (source `{}`, {}:{}) is a param-pattern route that would shadow {} same-method, same-\
             segment-count literal route(s) provided by a DIFFERENT source — e.g. `{}` (source `{}`, {}:{}) — IF \
             both sources are served behind one shared first-match gateway/router (the pattern's shape matches \
             every request the literal route was meant to catch, so a gateway that tries this source's routes \
             first would swallow the other source's literal route before it ever gets there). This is a static \
             analysis and cannot see deploy topology, so this only actually bites when the two sources share a \
             first-match gateway — confirm that first. Fix: disambiguate the route prefixes between the sources \
             (e.g. mount each source under its own path prefix), or ensure the gateway registers literal routes \
             before pattern routes regardless of which source provided them. {} if these sources are never \
             routed through one shared gateway.",
            pattern.key,
            pattern.source,
            pattern.file,
            pattern.line,
            shadowed_count,
            example.key,
            example.source,
            example.file,
            example.line,
            zzop_core::disable_hint("cross-layer/route-shadowing"),
        );

        out.push(zzop_core::Finding {
            rule_id: "cross-layer/route-shadowing".to_string(),
            severity: zzop_core::Severity::Warning,
            file: pattern.file.clone(),
            line: pattern.line,
            message,
            data: Some(serde_json::json!({
                "patternKey": pattern.key,
                "patternSource": pattern.source,
                "shadowedCount": shadowed_count,
                "shadowed": sample,
            })),
        });
    }

    out.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provide(source: &str, key: &str, file: &str, line: u32) -> HttpProvideSite {
        HttpProvideSite {
            source: source.to_string(),
            key: key.to_string(),
            file: file.to_string(),
            line,
        }
    }

    #[test]
    fn cross_tree_pattern_shadows_a_different_tree_literal() {
        let provides = vec![
            provide("be", "GET /users/{}", "be/routes.rs", 10),
            provide("gw", "GET /users/export", "gw/routes.ts", 20),
        ];
        let out = cross_tree_route_shadowing_findings(&provides);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "cross-layer/route-shadowing");
        assert_eq!(out[0].severity, zzop_core::Severity::Warning);
        assert_eq!(out[0].file, "be/routes.rs");
        assert_eq!(out[0].line, 10);
        assert!(out[0].message.contains("shared first-match gateway"));
        assert!(out[0].message.contains("disabled_rules"));
        let data = out[0].data.as_ref().unwrap();
        assert_eq!(data["patternKey"], "GET /users/{}");
        assert_eq!(data["shadowedCount"], 1);
    }

    #[test]
    fn same_tree_pattern_and_literal_are_not_flagged() {
        // Same-source overlap is the single-tree `route-shadowing` rule's territory.
        let provides = vec![
            provide("be", "GET /users/{}", "be/routes.rs", 10),
            provide("be", "GET /users/export", "be/routes.rs", 20),
        ];
        assert!(cross_tree_route_shadowing_findings(&provides).is_empty());
    }

    #[test]
    fn differing_segment_count_is_not_shadowing() {
        let provides = vec![
            provide("be", "GET /users/{}", "be/routes.rs", 10),
            provide("gw", "GET /users/export/extra", "gw/routes.ts", 20),
        ];
        assert!(cross_tree_route_shadowing_findings(&provides).is_empty());
    }

    #[test]
    fn byte_identical_paths_are_excluded() {
        let provides = vec![
            provide("be", "GET /users/{}", "be/routes.rs", 10),
            provide("gw", "GET /users/{}", "gw/routes.ts", 20),
        ];
        // Both are patterns, so neither lands in the literal bucket — no finding.
        assert!(cross_tree_route_shadowing_findings(&provides).is_empty());
    }

    #[test]
    fn test_file_provides_are_skipped() {
        let provides = vec![
            provide("be", "GET /users/{}", "be/routes.test.ts", 10),
            provide("gw", "GET /users/export", "gw/routes.ts", 20),
        ];
        assert!(cross_tree_route_shadowing_findings(&provides).is_empty());

        let provides2 = vec![
            provide("be", "GET /users/{}", "be/routes.rs", 10),
            provide("gw", "GET /users/export", "gw/routes.test.ts", 20),
        ];
        assert!(cross_tree_route_shadowing_findings(&provides2).is_empty());
    }

    #[test]
    fn output_is_deterministic_across_input_order() {
        let a = vec![
            provide("be", "GET /users/{}", "be/routes.rs", 10),
            provide("gw", "GET /users/export", "gw/routes.ts", 20),
            provide("gw", "GET /users/active", "gw/routes.ts", 5),
        ];
        let mut b = a.clone();
        b.reverse();
        let out_a = cross_tree_route_shadowing_findings(&a);
        let out_b = cross_tree_route_shadowing_findings(&b);
        assert_eq!(out_a.len(), out_b.len());
        for (x, y) in out_a.iter().zip(out_b.iter()) {
            assert_eq!(x.file, y.file);
            assert_eq!(x.line, y.line);
            assert_eq!(x.data, y.data);
        }
    }
}
