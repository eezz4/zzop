use super::{hits, scan, TempDir};

// --- route-exposure ---

#[test]
fn dev_path_with_no_env_guard_is_flagged() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.get(\"/api/dev/config\", api.configHandler);\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "route-exposure").len(), 1, "{:?}", out.findings);
}

#[test]
fn debug_path_with_no_env_guard_is_flagged() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.get(\"/api/debug/state\", api.stateSnapshot);\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "route-exposure").len(), 1, "{:?}", out.findings);
}

#[test]
fn internal_path_with_no_env_guard_is_flagged() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.post(\"/api/internal/flush\", api.cacheFlush);\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "route-exposure").len(), 1, "{:?}", out.findings);
}

#[test]
fn dunder_test_path_with_no_env_guard_is_flagged() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.delete(\"/api/__test__/reset\", api.seedReset);\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "route-exposure").len(), 1, "{:?}", out.findings);
}

#[test]
fn playground_path_with_no_env_guard_is_flagged() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.get(\"/api/playground/echo\", api.echoHandler);\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "route-exposure").len(), 1, "{:?}", out.findings);
}

#[test]
fn multiple_dangerous_paths_without_guard_are_all_flagged() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.get(\"/api/dev/flags\", api.flagList);\napiRoutes.get(\"/api/debug/heap\", api.heapSnapshot);\napiRoutes.post(\"/api/internal/rebuild\", api.rebuildIndex);\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "route-exposure").len(), 3, "{:?}", out.findings);
}

#[test]
fn handler_identifier_containing_dev_hint_passes() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const devOnlyHandlers: any;\napiRoutes.get(\"/api/dev/config\", devOnlyHandlers.config);\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "route-exposure").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn handler_identifier_containing_guard_hint_passes() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const guardedMetrics: any;\napiRoutes.get(\"/api/internal/metrics\", guardedMetrics.handler);\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "route-exposure").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn handler_identifier_containing_require_dev_hint_passes() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare function requireDevAccess(): any;\napiRoutes.get(\"/api/debug/state\", requireDevAccess);\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "route-exposure").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn handler_identifier_containing_is_production_hint_passes() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const isProductionGuarded: any;\napiRoutes.get(\"/api/dev/tools\", isProductionGuarded.tools);\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "route-exposure").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn handler_identifier_containing_is_local_hint_passes() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare function isLocalOnlyEcho(): any;\napiRoutes.get(\"/api/playground/echo\", isLocalOnlyEcho);\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "route-exposure").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn ordinary_paths_are_not_inspected() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.get(\"/api/users\", api.userList);\napiRoutes.post(\"/api/items\", api.itemCreate);\napiRoutes.get(\"/api/health\", api.health);\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "route-exposure").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn handler_identifier_containing_node_env_hint_passes_route_exposure() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare function nodeEnvGuard(h: any): any;\napiRoutes.get(\"/api/dev/tools\", nodeEnvGuard(handlers));\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "route-exposure").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- message <-> matcher agreement (the `anchor_exclude_pattern` carve-out's exact edges) ---
//
// The clearing regex is `(?i),[^,"']*(?:dev|debug|internal|env|guard|isProduction|isLocal|NODE_ENV)`:
// the keyword must be reachable from SOME comma on the line without crossing another comma or a quote —
// i.e. it must sit in a later, UNQUOTED call argument. The message once said only "no guard-hint keyword
// on its registration line", which promised a far wider clear than the matcher performs; the three pins
// below fix both edges of the real carve-out and the message wording that now states it.

#[test]
fn a_guard_hint_before_the_first_comma_does_not_clear_the_finding() {
    // An `if (isDev)` gate IS on the registration line, but ahead of every comma — no comma precedes it,
    // so the carve-out cannot reach it. The old message claimed this line was cleared; it is not.
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\ndeclare const isDev: boolean;\nif (isDev) apiRoutes.get(\"/api/dev/config\", api.configHandler);\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "route-exposure").len(), 1, "{:?}", out.findings);
}

#[test]
fn a_guard_hint_inside_a_quoted_later_argument_does_not_clear_the_finding() {
    // `"devOnly"` follows a comma, but a quote stands between that comma and the keyword, so `[^,"']*`
    // cannot span it. A string literal is not evidence of a guard — this is the edge the quote exclusion
    // exists for, and the message now says "unquoted" rather than "on its registration line".
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.get(\"/api/dev/config\", \"devOnly\", api.configHandler);\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "route-exposure").len(), 1, "{:?}", out.findings);
}

#[test]
fn the_emitted_message_states_the_carve_outs_real_shape_not_a_wider_one() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.get(\"/api/dev/config\", api.configHandler);\n",
    );
    let out = scan(&dir);
    let finding = hits(&out, "route-exposure")
        .into_iter()
        .next()
        .expect("the fixture above must fire once");
    assert!(
        finding
            .message
            .contains("as a later, unquoted call argument"),
        "message must state WHERE a guard hint has to sit: {}",
        finding.message
    );
    assert!(
        finding
            .message
            .contains("anywhere before the line's first comma, does not clear this"),
        "message must state the two positions that do NOT clear: {}",
        finding.message
    );
    assert!(
        !finding
            .message
            .contains("keyword (dev/debug/internal/env/guard/isProduction/isLocal/NODE_ENV) on its registration line"),
        "the pre-fix wording promised a line-wide keyword search the matcher never performs: {}",
        finding.message
    );
}

#[test]
fn route_exposure_ok_marker_on_the_same_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.get(\"/api/dev/config\", api.configHandler); // route-exposure-ok: reviewed, disabled outside CI\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "route-exposure").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- skip_comment_lines + test-path file_exclude_pattern ---
// A commented-out route registration must not fire any of these rules, and each excludes test-fixture
// paths (e.g. this pack's own `__tests__` dir) as scaffolding, not a deployed route.

#[test]
fn admin_route_shape_mentioned_only_in_a_comment_is_not_flagged() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\n// apiRoutes.get(\"/api/admin/users\", api.userList) -- moved below with a guard\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "auth-gates").is_empty(), "{:?}", out.findings);
}

#[test]
fn dev_route_registered_in_a_routes_test_fixture_path_is_not_flagged() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/__tests__/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.get(\"/api/dev/config\", api.configHandler);\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "route-exposure").is_empty(),
        "{:?}",
        out.findings
    );
}
