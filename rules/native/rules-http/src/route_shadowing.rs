//! `route-shadowing` — within one file's HTTP `provides`, a param-segment route registered at an EARLIER
//! line than a literal-segment route of the same shape shadows it in a first-match router (Express/Koa/
//! Fastify-style, "the first registered pattern that matches wins"): the param route also matches every
//! request the literal route was meant to catch, making the literal handler unreachable in practice.
//!
//! Two `IoProvide`s (`kind == "http"`) shadow each other only when: same `file` (cross-file pairs carry no
//! registration-order signal) and method; same segment count in the normalized `http_interface_key` path;
//! every segment identical except one, where the earlier route has a `{}` placeholder and the later route
//! has a literal there; and the param route's `line` is strictly less than the literal route's. A position
//! where both are literal is `duplicate-route`'s territory. When several earlier param routes qualify, the
//! EARLIEST is reported, since it intercepts first in a first-match router.
//!
//! "First registered pattern wins" is an Express/Koa/Fastify convention, not universal — some routers pick
//! the most-specific match regardless of order and are unaffected. This stays a Warning (never Critical)
//! for that reason, stated explicitly in the message so a false positive is easy to recognize and disable.

pub fn route_shadowing_findings(io_provides: &[zzop_core::IoProvide]) -> Vec<zzop_core::Finding> {
    let mut by_file_method: std::collections::BTreeMap<(&str, &str), Vec<&zzop_core::IoProvide>> =
        std::collections::BTreeMap::new();
    for p in io_provides {
        if p.kind != "http" {
            continue;
        }
        let Some((method, _path)) = p.key.split_once(' ') else {
            continue;
        };
        by_file_method
            .entry((p.file.as_str(), method))
            .or_default()
            .push(p);
    }

    let mut findings = Vec::new();
    for ((_, _), mut routes) in by_file_method {
        routes.sort_by_key(|p| p.line);
        for (i, literal) in routes.iter().enumerate() {
            let Some((_, literal_path)) = literal.key.split_once(' ') else {
                continue;
            };
            let literal_segs: Vec<&str> = literal_path.split('/').collect();
            let mut earliest_shadow: Option<&zzop_core::IoProvide> = None;
            for cand in &routes[..i] {
                let Some((_, cand_path)) = cand.key.split_once(' ') else {
                    continue;
                };
                let cand_segs: Vec<&str> = cand_path.split('/').collect();
                if !shadows(&cand_segs, &literal_segs) {
                    continue;
                }
                if earliest_shadow.is_none_or(|cur| cand.line < cur.line) {
                    earliest_shadow = Some(cand);
                }
            }
            if let Some(param) = earliest_shadow {
                findings.push(zzop_core::Finding {
                    rule_id: "route-shadowing".to_string(),
                    severity: zzop_core::Severity::Warning,
                    file: literal.file.clone(),
                    line: literal.line,
                    message: format!(
                        "Route `{}` (registered here at line {}) is shadowed by an earlier param route `{}` \
                         registered at line {} in the same file — in a first-match router (Express/Koa/\
                         Fastify-style), the param route's pattern also matches every request this literal \
                         route was meant to catch, so the earlier registration intercepts first and this \
                         handler is effectively unreachable. Fix: register the literal route BEFORE the param \
                         route (or merge them into one handler that branches on the concrete value). Precision \
                         limit: \"first registered pattern wins\" is framework-dependent — a router that picks \
                         the most-specific match regardless of registration order is unaffected by this shape; \
                         disable {} if that's your framework or the ordering is intentional (this rule has no \
                         inline suppression marker).",
                        literal.key,
                        literal.line,
                        param.key,
                        param.line,
                        // `disable_hint` itself always starts with "Disable " — this site's surrounding
                        // sentence already supplies "disable" (mid-sentence, after a semicolon), so only the
                        // "via config ..." remainder is spliced in, same technique
                        // `rules-schema/src/message.rs`'s `disable_hint_tail` uses.
                        zzop_core::disable_hint("route-shadowing")
                            .strip_prefix("Disable ")
                            .expect("disable_hint always starts with \"Disable \"")
                    ),
                    data: Some(serde_json::json!({
                        "literalKey": literal.key,
                        "literalLine": literal.line,
                        "paramKey": param.key,
                        "paramLine": param.line,
                        "file": literal.file,
                    })),
                });
            }
        }
    }
    findings.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    findings
}

/// True when `param_segs` (the earlier route) and `literal_segs` (the later route) are the same shape
/// except for exactly one position, where `param_segs` holds `{}` and `literal_segs` holds a literal there.
fn shadows(param_segs: &[&str], literal_segs: &[&str]) -> bool {
    if param_segs.len() != literal_segs.len() {
        return false;
    }
    let mut diff_at_param_placeholder = false;
    for (a, b) in param_segs.iter().zip(literal_segs.iter()) {
        if a == b {
            continue;
        }
        if diff_at_param_placeholder {
            return false; // a second differing position — not the decidable subset
        }
        if *a != "{}" || *b == "{}" {
            return false; // the differing position must be param-vs-literal, not literal-vs-literal
        }
        diff_at_param_placeholder = true;
    }
    diff_at_param_placeholder
}

#[cfg(test)]
mod tests {
    //! Unit tests for `route_shadowing_findings`'s grouping + shape logic (e2e coverage: `packages/engine/tests/analyze_io_natives.rs`).
    use super::*;

    fn provide(key: &str, file: &str, line: u32) -> zzop_core::IoProvide {
        zzop_core::IoProvide {
            kind: "http".to_string(),
            key: key.to_string(),
            file: file.to_string(),
            line,
            symbol: None,
        }
    }

    #[test]
    fn earlier_param_route_shadows_a_later_literal_route_same_position() {
        let provides = vec![
            provide("GET /items/{}", "r.ts", 2),
            provide("GET /items/active", "r.ts", 5),
        ];
        let found = route_shadowing_findings(&provides);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].file, "r.ts");
        assert_eq!(found[0].line, 5);
        assert_eq!(found[0].rule_id, "route-shadowing");
        assert_eq!(found[0].severity, zzop_core::Severity::Warning);
        assert!(found[0].message.contains("line 2"));
    }

    /// Pins the exact rendered message — regression coverage for the mid-sentence, lowercase-"disable"
    /// `disable_hint` splice this message went through during the 2026-07-10 dialect-consolidation sweep
    /// (unlike most native messages, this one reads "...disable {tail}", not "...Disable via config...").
    #[test]
    fn message_is_byte_identical_to_the_pre_sweep_text() {
        let provides = vec![
            provide("GET /items/{}", "r.ts", 2),
            provide("GET /items/active", "r.ts", 5),
        ];
        let found = route_shadowing_findings(&provides);
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].message,
            "Route `GET /items/active` (registered here at line 5) is shadowed by an earlier param route \
             `GET /items/{}` registered at line 2 in the same file — in a first-match router (Express/Koa/\
             Fastify-style), the param route's pattern also matches every request this literal route was \
             meant to catch, so the earlier registration intercepts first and this handler is effectively \
             unreachable. Fix: register the literal route BEFORE the param route (or merge them into one \
             handler that branches on the concrete value). Precision limit: \"first registered pattern \
             wins\" is framework-dependent — a router that picks the most-specific match regardless of \
             registration order is unaffected by this shape; disable via config `rules: { \
             \"route-shadowing\": \"off\" }` (embedders: `disabled_rules`) if that's your framework or the \
             ordering is intentional (this rule has no inline suppression marker)."
        );
    }

    #[test]
    fn literal_route_registered_before_the_param_route_is_not_shadowed() {
        let provides = vec![
            provide("GET /items/active", "r.ts", 2),
            provide("GET /items/{}", "r.ts", 5),
        ];
        assert!(route_shadowing_findings(&provides).is_empty());
    }

    #[test]
    fn different_methods_are_never_compared() {
        let provides = vec![
            provide("GET /items/{}", "r.ts", 2),
            provide("POST /items/active", "r.ts", 5),
        ];
        assert!(route_shadowing_findings(&provides).is_empty());
    }

    #[test]
    fn different_files_are_never_compared() {
        let provides = vec![
            provide("GET /items/{}", "a.ts", 2),
            provide("GET /items/active", "b.ts", 5),
        ];
        assert!(route_shadowing_findings(&provides).is_empty());
    }

    #[test]
    fn two_literal_routes_at_the_same_position_are_not_flagged_shadowing() {
        // Same-key duplicate registration is `duplicate-route`'s territory, not this rule's.
        let provides = vec![
            provide("GET /items/active", "r.ts", 2),
            provide("GET /items/paused", "r.ts", 5),
        ];
        assert!(route_shadowing_findings(&provides).is_empty());
    }

    #[test]
    fn differing_segment_count_is_not_shadowing() {
        let provides = vec![
            provide("GET /items/{}", "r.ts", 2),
            provide("GET /items/active/extra", "r.ts", 5),
        ];
        assert!(route_shadowing_findings(&provides).is_empty());
    }

    #[test]
    fn two_differing_positions_is_out_of_the_decidable_subset() {
        let provides = vec![
            provide("GET /items/{}/{}", "r.ts", 2),
            provide("GET /items/active/paused", "r.ts", 5),
        ];
        assert!(route_shadowing_findings(&provides).is_empty());
    }

    #[test]
    fn earliest_of_multiple_qualifying_param_routes_is_reported() {
        let provides = vec![
            provide("GET /items/{}", "r.ts", 8),
            provide("GET /items/{}", "r.ts", 2),
            provide("GET /items/active", "r.ts", 10),
        ];
        let found = route_shadowing_findings(&provides);
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].data.as_ref().unwrap()["paramLine"].as_u64(),
            Some(2)
        );
    }

    #[test]
    fn non_http_provides_are_ignored() {
        let provides = vec![
            zzop_core::IoProvide {
                kind: "queue".to_string(),
                key: "GET /items/{}".to_string(),
                file: "r.ts".to_string(),
                line: 2,
                symbol: None,
            },
            zzop_core::IoProvide {
                kind: "queue".to_string(),
                key: "GET /items/active".to_string(),
                file: "r.ts".to_string(),
                line: 5,
                symbol: None,
            },
        ];
        assert!(route_shadowing_findings(&provides).is_empty());
    }
}
