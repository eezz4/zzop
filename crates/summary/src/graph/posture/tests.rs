use super::*;
use serde_json::json;

fn one_tree() -> Value {
    json!({
        "trees": [{
            "sourceId": "api",
            "output": {
                "findings": [
                    { "ruleId": "mutating-route-no-auth", "severity": "info",
                      "file": "src/users.ts", "line": 10, "message": "m" }
                ],
                "ir": { "io": { "provides": [
                    { "kind": "http", "key": "POST /users",  "file": "src/users.ts", "line": 10 },
                    { "kind": "http", "key": "DELETE /users/{}", "file": "src/users.ts", "line": 20 },
                    { "kind": "http", "key": "GET /users",   "file": "src/users.ts", "line": 30 },
                    { "kind": "db-table", "key": "users",    "file": "src/db.ts",    "line": 1 }
                ]}}
            }
        }]
    })
}

#[test]
fn only_mutating_http_routes_are_drawn() {
    let m = project(&one_tree(), None, DEFAULT_POSTURE_TOP);
    assert!(m.contains("POST /users"), "{m}");
    assert!(m.contains("DELETE /users/{}"), "{m}");
    assert!(!m.contains("GET /users"), "a read is not unguarded:\n{m}");
    assert!(!m.contains("db-table"), "{m}");
    assert!(m.contains("drawn 2 / total 2"), "{m}");
}

/// Guard status is the rule's verdict, matched on the route's OWN file+line — not a re-derivation.
#[test]
fn the_flagged_route_gets_the_flag_shape_and_the_other_does_not() {
    let m = project(&one_tree(), None, DEFAULT_POSTURE_TOP);
    assert!(m.contains("r0>\""), "the reported route is a flag:\n{m}");
    assert!(m.contains("r1[\""), "the unreported one is a box:\n{m}");
    assert!(m.contains("reported unguarded: 1"), "{m}");
}

/// The point of the whole picture: a box is NOT proof of a guard, because the rule is also silent on
/// routes it cannot judge. If this legend regresses, the diagram starts asserting safety it never had.
#[test]
fn a_box_is_labelled_guarded_or_exempt_never_proven_guarded() {
    let m = project(&one_tree(), None, DEFAULT_POSTURE_TOP);
    assert!(m.contains("GUARDED-OR-EXEMPT, not"), "{m}");
    assert!(
        m.contains("Absence of a finding is not proof of a guard"),
        "{m}"
    );
    assert!(m.contains("uncovered language"), "{m}");
}

#[test]
fn routes_are_grouped_by_tree_and_the_subgraph_id_is_sanitized() {
    let v = json!({ "trees": [{ "sourceId": "web-fe/app", "output": { "findings": [], "ir": { "io": {
        "provides": [{ "kind": "http", "key": "PUT /a", "file": "a.ts", "line": 1 }] }}}}]});
    let m = project(&v, None, DEFAULT_POSTURE_TOP);
    assert!(m.contains("subgraph web_fe_app"), "{m}");
}

#[test]
fn the_cap_is_per_tree_and_the_drop_is_disclosed() {
    let m = project(&one_tree(), None, 1);
    assert!(m.contains("drawn 1 / total 2"), "{m}");
    assert!(m.contains("PARTIAL VIEW"), "{m}");
}

#[test]
fn scope_filters_by_source_or_file_prefix() {
    let m = project(&one_tree(), Some("nope/"), DEFAULT_POSTURE_TOP);
    assert!(m.contains("drawn 0 / total 2"), "{m}");
}

/// "No routes extracted" and "no write surface" are different — the same silence-vs-clean rule.
#[test]
fn a_tree_with_no_mutating_routes_says_why_rather_than_implying_safety() {
    let v = json!({ "trees": [{ "sourceId": "a", "output": { "findings": [], "ir": { "io": { "provides": [] }}}}]});
    let m = project(&v, None, 5);
    assert!(m.contains("NOT the same as a repo with no"), "{m}");
    assert!(m.contains("extraction gap"), "{m}");
}

/// The T2 pin this module's doc promises: the local list must EQUAL the rule's own vocabulary.
///
/// The test below (`every_write_method_..._is_drawn`) cannot do this job and used to be described as if
/// it could — it iterates the LOCAL list, so it only ever asserts "whatever this file says is drawable".
/// Delete a verb here and it passes with one fewer iteration; add one to the RULE and it never looks.
/// Measured during the 2026-07-28 release audit: widening `WRITE_HTTP_METHODS` to five verbs left the
/// whole workspace green. This test reads BOTH sides, which is the only shape that catches either
/// direction. Ordering is deliberately ignored — the rule spells them PUT/DELETE/POST/PATCH and this
/// diagram POST/PUT/PATCH/DELETE; the claim is set equality, not spelling.
#[test]
fn the_drawn_write_verbs_equal_the_rules_own_vocabulary() {
    let mut drawn: Vec<&str> = WRITE_METHODS.to_vec();
    let mut gated: Vec<&str> = zzop_rules_http::WRITE_HTTP_METHODS.to_vec();
    drawn.sort_unstable();
    gated.sort_unstable();
    assert_eq!(
        drawn, gated,
        "posture.rs must draw exactly the verbs `mutating-route-no-auth` gates on — a verb in the rule \
         but not here is a route this diagram silently stops covering, and the reverse draws a route \
         the rule never judged as UNGUARDED-or-exempt."
    );
}

/// Every write method the rule gates on must be drawable — a drift here would silently hide DELETEs.
#[test]
fn every_write_method_the_rule_gates_on_is_drawn() {
    for method in WRITE_METHODS {
        let v = json!({ "trees": [{ "sourceId": "a", "output": { "findings": [], "ir": { "io": {
            "provides": [{ "kind": "http", "key": format!("{method} /x"), "file": "a.ts", "line": 1 }] }}}}]});
        assert!(
            project(&v, None, 5).contains("drawn 1 / total 1"),
            "{method}"
        );
    }
}

#[test]
fn the_same_analysis_renders_identical_bytes() {
    let v = one_tree();
    assert_eq!(project(&v, None, 5), project(&v, None, 5));
}
