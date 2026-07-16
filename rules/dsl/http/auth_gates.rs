use super::{hits, scan, TempDir};

// --- auth-gates ---

#[test]
fn admin_path_with_no_role_check_handler_is_flagged() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.get(\"/api/admin/users\", api.userList);\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "auth-gates").len(), 1, "{:?}", out.findings);
}

#[test]
fn internal_path_with_no_role_check_handler_is_flagged() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.post(\"/api/internal/metrics\", api.metricsWrite);\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "auth-gates").len(), 1, "{:?}", out.findings);
}

#[test]
fn dev_path_with_no_role_check_handler_is_flagged() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.get(\"/api/dev/config\", api.devConfig);\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "auth-gates").len(), 1, "{:?}", out.findings);
}

#[test]
fn multiple_protected_paths_all_missing_auth_are_all_flagged() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.get(\"/api/admin/items\", api.itemList);\napiRoutes.delete(\"/api/internal/cache\", api.clearCache);\napiRoutes.get(\"/api/dev/flags\", api.featureFlags);\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "auth-gates").len(), 3, "{:?}", out.findings);
}

#[test]
fn extra_path_segments_after_protected_segment_is_still_flagged() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.get(\"/api/admin/users/:id/detail\", api.userDetail);\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "auth-gates").len(), 1, "{:?}", out.findings);
}

#[test]
fn handler_identifier_containing_admin_keyword_passes() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const adminHandlers: any;\napiRoutes.get(\"/api/admin/users\", adminHandlers.userList);\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "auth-gates").is_empty(), "{:?}", out.findings);
}

#[test]
fn handler_identifier_containing_role_keyword_passes() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const handlers: any;\napiRoutes.get(\"/api/internal/report\", handlers.roleBasedReport);\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "auth-gates").is_empty(), "{:?}", out.findings);
}

#[test]
fn handler_identifier_containing_require_admin_call_passes() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const handlers: any;\ndeclare function requireAdmin(h: any): any;\napiRoutes.get(\"/api/admin/settings\", requireAdmin(handlers.settings));\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "auth-gates").is_empty(), "{:?}", out.findings);
}

#[test]
fn handler_identifier_containing_guard_keyword_passes() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const guardedHandlers: any;\napiRoutes.delete(\"/api/internal/flush\", guardedHandlers.flush);\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "auth-gates").is_empty(), "{:?}", out.findings);
}

#[test]
fn ordinary_path_with_no_protected_segment_is_not_inspected() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.get(\"/api/users\", api.userList);\napiRoutes.post(\"/api/items\", api.itemCreate);\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "auth-gates").is_empty(), "{:?}", out.findings);
}

#[test]
fn handler_identifier_containing_is_local_env_gate_hint_passes() {
    // An env-scoped gate like `if (!CONFIG.isLocal()) return 403;` must not be flagged as missing auth — an environment check does gate the route.
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const isLocalGuardedHandlers: any;\napiRoutes.get(\"/api/admin/users\", isLocalGuardedHandlers.userList);\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "auth-gates").is_empty(), "{:?}", out.findings);
}

#[test]
fn handler_identifier_containing_node_env_hint_passes() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare function nodeEnvGuard(h: any): any;\napiRoutes.post(\"/api/internal/metrics\", nodeEnvGuard(handlers));\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "auth-gates").is_empty(), "{:?}", out.findings);
}

#[test]
fn auth_gate_ok_marker_on_the_same_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.get(\"/api/admin/users\", api.userList); // auth-gate-ok: reviewed, gated at the API gateway layer\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "auth-gates").is_empty(), "{:?}", out.findings);
}
