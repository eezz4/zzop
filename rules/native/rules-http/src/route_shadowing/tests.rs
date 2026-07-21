//! Unit tests for `route_shadowing_findings`'s grouping + shape logic + the first-match-router framework
//! gate (e2e coverage: `crates/engine/tests/analyze_io_natives.rs`).

use super::*;

fn provide(key: &str, file: &str, line: u32) -> zzop_core::IoProvide {
    zzop_core::IoProvide {
        body: None,
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
         NestJS-style), the param route's pattern also matches every request this literal route was \
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
fn a_specificity_match_framework_java_is_exempt() {
    // The be-spring-jwt shape: `@GetMapping("/{username}")` before `@GetMapping("/me")` in one
    // controller. Spring's AntPathMatcher picks the literal `/me` regardless of order, so this is NOT
    // a shadow — a `.java` provide must not fire (would be a false positive).
    let provides = vec![
        provide("GET /users/{}", "UserController.java", 74),
        provide("GET /users/me", "UserController.java", 87),
    ];
    assert!(
        route_shadowing_findings(&provides).is_empty(),
        "Spring (specificity-match) routes must be exempt"
    );
}

#[test]
fn python_fastapi_first_match_still_shadows() {
    // FastAPI IS first-match (Starlette matches in registration order), so a `.py` param-before-literal
    // pair legitimately shadows — the gate must KEEP Python, not blanket-exempt non-TS.
    let provides = vec![
        provide("GET /users/{}", "routes.py", 3),
        provide("GET /users/me", "routes.py", 7),
    ];
    let found = route_shadowing_findings(&provides);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].line, 7);
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
            body: None,
            kind: "queue".to_string(),
            key: "GET /items/{}".to_string(),
            file: "r.ts".to_string(),
            line: 2,
            symbol: None,
        },
        zzop_core::IoProvide {
            body: None,
            kind: "queue".to_string(),
            key: "GET /items/active".to_string(),
            file: "r.ts".to_string(),
            line: 5,
            symbol: None,
        },
    ];
    assert!(route_shadowing_findings(&provides).is_empty());
}
