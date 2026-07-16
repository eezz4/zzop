//! `ws-no-auth` + native `duplicate-route` tests (split from `fullstack.rs`).

use super::*;

// --- ws-no-auth ---

#[test]
fn websocket_opened_without_auth_material_is_flagged() {
    let dir = TempDir::new("zzop-fullstack");
    dir.write(
        "src/socket.ts",
        "export function connect() {\n  return new WebSocket(\"wss://example.com/stream\");\n}\n",
    );
    let out = scan(&dir);
    let found = hits(&out, "ws-no-auth");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(found[0].line, 2);
}

#[test]
fn websocket_opened_with_token_in_the_same_function_is_not_flagged() {
    let dir = TempDir::new("zzop-fullstack");
    dir.write(
        "src/socket.ts",
        "export function connect(token: string) {\n  return new WebSocket(`wss://example.com/stream?token=${token}`);\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "ws-no-auth").is_empty(), "{:?}", out.findings);
}

#[test]
fn ws_auth_ok_marker_above_the_websocket_call_suppresses_the_finding() {
    let dir = TempDir::new("zzop-fullstack");
    dir.write(
        "src/socket.ts",
        "export function connect() {\n  // ws-auth-ok: public read-only market-data feed, no auth by design\n  return new WebSocket(\"wss://example.com/stream\");\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "ws-no-auth").is_empty(), "{:?}", out.findings);
}

// --- duplicate-route (native) ---

fn duplicate_route_hits(out: &AnalyzeOutput) -> Vec<&zzop_core::Finding> {
    out.findings
        .iter()
        .filter(|f| f.rule_id == "duplicate-route")
        .collect()
}

#[test]
fn same_route_registered_in_two_files_is_flagged_once_at_the_later_site() {
    let dir = TempDir::new("zzop-fullstack");
    dir.write(
        "src/routes/a.ts",
        "const apiRoutes = new Hono();\napiRoutes.get(\"/api/users\", api.userList);\n",
    );
    dir.write(
        "src/routes/b.ts",
        "const apiRoutes = new Hono();\napiRoutes.get(\"/api/users\", api.userListAgain);\n",
    );
    let out = scan(&dir);
    let found = duplicate_route_hits(&out);
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    // Sites are sorted (file, line): "a.ts" < "b.ts", so b.ts is the later/duplicate site and a.ts is the canonical first site named in the message.
    assert_eq!(found[0].file, "src/routes/b.ts");
    assert_eq!(found[0].line, 2);
    assert!(found[0].message.contains("GET /api/users"));
    assert!(found[0].message.contains("src/routes/a.ts:2"));
}

#[test]
fn two_distinct_routes_across_files_are_not_flagged() {
    let dir = TempDir::new("zzop-fullstack");
    dir.write(
        "src/routes/a.ts",
        "const apiRoutes = new Hono();\napiRoutes.get(\"/api/users\", api.userList);\n",
    );
    dir.write(
        "src/routes/b.ts",
        "const apiRoutes = new Hono();\napiRoutes.get(\"/api/items\", api.itemList);\n",
    );
    let out = scan(&dir);
    assert!(duplicate_route_hits(&out).is_empty(), "{:?}", out.findings);
}

#[test]
fn duplicate_route_can_be_disabled_via_rule_config() {
    let dir = TempDir::new("zzop-fullstack");
    dir.write(
        "src/routes/a.ts",
        "const apiRoutes = new Hono();\napiRoutes.get(\"/api/users\", api.userList);\n",
    );
    dir.write(
        "src/routes/b.ts",
        "const apiRoutes = new Hono();\napiRoutes.get(\"/api/users\", api.userListAgain);\n",
    );
    let rule_config = zzop_core::RuleConfig {
        disabled_rules: vec!["duplicate-route".to_string()],
        ..zzop_core::RuleConfig::default()
    };
    let out = analyze_tree(dir.path(), &config_with(rule_config));
    assert!(duplicate_route_hits(&out).is_empty(), "{:?}", out.findings);
}
