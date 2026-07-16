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
