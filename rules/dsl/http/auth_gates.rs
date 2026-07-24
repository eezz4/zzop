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
fn dev_path_is_not_an_auth_gates_concern() {
    // `/dev/` is an ENV-exposure axis (does dev tooling leak to prod?), owned by `route-exposure` —
    // authorization (who may call) is a different question, so auth-gates deliberately does not inspect it.
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.get(\"/api/dev/config\", api.plainConfig);\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "auth-gates").is_empty(), "{:?}", out.findings);
}

#[test]
fn multiple_protected_paths_all_missing_auth_are_all_flagged() {
    // Only the authorization-axis segments (`/admin/`, `/internal/`) are auth-gates' concern — the
    // `/dev/` route is `route-exposure`'s, so exactly two of the three fire here.
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.get(\"/api/admin/items\", api.itemList);\napiRoutes.delete(\"/api/internal/cache\", api.clearCache);\napiRoutes.get(\"/api/dev/flags\", api.featureFlags);\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "auth-gates").len(), 2, "{:?}", out.findings);
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

// --- the removed keyword carve-out (opus F2, 2026-07-23): a keyword inside a handler IDENTIFIER must
// NOT clear a security finding. The old same-line token belt over-cleared exactly these shapes (and,
// systematically, Django's `views.AdminView` / Java's `value="/admin/..."` argument) — a lexical name
// is not auth evidence. Real evidence = the `auth-guarded` attribute (native recognizer or Mode B
// injection); vetted cases use the `// auth-gates-ok` marker. These pins keep the carve-out removed.

#[test]
fn handler_identifier_containing_admin_keyword_no_longer_clears() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const adminHandlers: any;\napiRoutes.get(\"/api/admin/users\", adminHandlers.userList);\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "auth-gates").len(), 1, "{:?}", out.findings);
}

#[test]
fn handler_identifier_containing_role_keyword_no_longer_clears() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const handlers: any;\napiRoutes.get(\"/api/internal/report\", handlers.roleBasedReport);\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "auth-gates").len(), 1, "{:?}", out.findings);
}

#[test]
fn require_admin_hof_wrapper_is_recognized_natively_and_the_attribute_vetoes_the_finding() {
    // `apiRoutes.get(path, requireAdmin(handler))` is the higher-order-function guard idiom — the
    // guard WRAPS the handler instead of preceding it as middleware. The TS recognizer now judges a
    // route's CALL-shaped last argument by its callee's guard vocabulary
    // (`router_mounts::guard::judge_guard_wrapper_arg`) and mints `auth-guarded` on that route: the
    // same open-vocabulary attribute channel a Mode B overlay injects, so this rule's `attr_absent`
    // veto clears the finding with no rule change. This test was a KNOWN-FP pin before the recognizer
    // existed; it now pins the recognition itself. Its negative half — a handler IDENTIFIER merely NAMED
    // `adminHandlers.userList` must still fire, because a name is not evidence — is the
    // `handler_identifier_containing_admin_keyword_no_longer_clears` test above, and the recognizer
    // judges only CALL last-args precisely to keep that split.
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const handlers: any;\ndeclare function requireAdmin(h: any): any;\napiRoutes.get(\"/api/admin/settings\", requireAdmin(handlers.settings));\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "auth-gates").is_empty(), "{:?}", out.findings);
}

#[test]
fn handler_identifier_containing_guard_keyword_no_longer_clears() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const guardedHandlers: any;\napiRoutes.delete(\"/api/internal/flush\", guardedHandlers.flush);\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "auth-gates").len(), 1, "{:?}", out.findings);
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
fn env_gate_alone_does_not_clear_an_admin_route() {
    // An env check (`isLocal`/`NODE_ENV`) gates WHERE code runs, not WHO may call it — it is not
    // authorization, so an `/admin/` route carrying only an env gate is still a missing-auth finding.
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const isLocalScopedHandlers: any;\napiRoutes.get(\"/api/admin/users\", isLocalScopedHandlers.userList);\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "auth-gates").len(), 1, "{:?}", out.findings);
}

#[test]
fn node_env_gate_alone_does_not_clear_an_internal_route() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare function nodeEnvOnly(h: any): any;\napiRoutes.post(\"/api/internal/metrics\", nodeEnvOnly(handlers));\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "auth-gates").len(), 1, "{:?}", out.findings);
}

#[test]
fn auth_gate_ok_marker_on_the_same_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.get(\"/api/admin/users\", api.userList); // auth-gates-ok: reviewed, gated at the API gateway layer\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "auth-gates").is_empty(), "{:?}", out.findings);
}
