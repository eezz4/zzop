use super::{hits, scan, TempDir};

// --- protected-path-no-auth-evidence ---

#[test]
fn admin_path_with_no_role_check_handler_is_flagged() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.get(\"/api/admin/users\", api.userList);\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "protected-path-no-auth-evidence").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn internal_path_with_no_role_check_handler_is_flagged() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.post(\"/api/internal/metrics\", api.metricsWrite);\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "protected-path-no-auth-evidence").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn dev_path_is_not_an_auth_gates_concern() {
    // `/dev/` is an ENV-exposure axis (does dev tooling leak to prod?), owned by `dev-path-no-guard-hint` —
    // authorization (who may call) is a different question, so protected-path-no-auth-evidence deliberately does not inspect it.
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.get(\"/api/dev/config\", api.plainConfig);\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "protected-path-no-auth-evidence").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn multiple_protected_paths_all_missing_auth_are_all_flagged() {
    // Only the authorization-axis segments (`/admin/`, `/internal/`) are protected-path-no-auth-evidence' concern — the
    // `/dev/` route is `dev-path-no-guard-hint`'s, so exactly two of the three fire here.
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.get(\"/api/admin/items\", api.itemList);\napiRoutes.delete(\"/api/internal/cache\", api.clearCache);\napiRoutes.get(\"/api/dev/flags\", api.featureFlags);\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "protected-path-no-auth-evidence").len(),
        2,
        "{:?}",
        out.findings
    );
}

#[test]
fn extra_path_segments_after_protected_segment_is_still_flagged() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.get(\"/api/admin/users/:id/detail\", api.userDetail);\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "protected-path-no-auth-evidence").len(),
        1,
        "{:?}",
        out.findings
    );
}

// --- the removed keyword carve-out (opus F2, 2026-07-23): a keyword inside a handler IDENTIFIER must
// NOT clear a security finding. The old same-line token belt over-cleared exactly these shapes (and,
// systematically, Django's `views.AdminView` / Java's `value="/admin/..."` argument) — a lexical name
// is not auth evidence. Real evidence = the `auth-guarded` attribute (native recognizer or Mode B
// injection); vetted cases use the `// zzop-protected-path-no-auth-evidence-ok` marker. These pins keep the carve-out removed.

#[test]
fn handler_identifier_containing_admin_keyword_no_longer_clears() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const adminHandlers: any;\napiRoutes.get(\"/api/admin/users\", adminHandlers.userList);\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "protected-path-no-auth-evidence").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn handler_identifier_containing_role_keyword_no_longer_clears() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const handlers: any;\napiRoutes.get(\"/api/internal/report\", handlers.roleBasedReport);\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "protected-path-no-auth-evidence").len(),
        1,
        "{:?}",
        out.findings
    );
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
    assert!(
        hits(&out, "protected-path-no-auth-evidence").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn handler_identifier_containing_guard_keyword_no_longer_clears() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const guardedHandlers: any;\napiRoutes.delete(\"/api/internal/flush\", guardedHandlers.flush);\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "protected-path-no-auth-evidence").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn ordinary_path_with_no_protected_segment_is_not_inspected() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.get(\"/api/users\", api.userList);\napiRoutes.post(\"/api/items\", api.itemCreate);\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "protected-path-no-auth-evidence").is_empty(),
        "{:?}",
        out.findings
    );
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
    assert_eq!(
        hits(&out, "protected-path-no-auth-evidence").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn node_env_gate_alone_does_not_clear_an_internal_route() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare function nodeEnvOnly(h: any): any;\napiRoutes.post(\"/api/internal/metrics\", nodeEnvOnly(handlers));\n",
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "protected-path-no-auth-evidence").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn go_admin_route_is_out_of_scope_because_go_has_no_auth_guarded_producer() {
    // 2026-08-02 (U63①, rules-owner FINDING-1, refuter-confirmed): the `attr_absent: "auth-guarded"`
    // veto only ever clears a route in a language whose recognizers can PRODUCE that attribute —
    // TS/JS, Java, Python today. Go (and C#) have route recognizers but no auth-evidence producer, so
    // before the `file_pattern` narrowing every correctly guarded Go/C# admin route was a structurally
    // guaranteed false positive. This pins the narrowing with a route the Go parser really extracts:
    // remove `|java|py)$`'s exclusion of go and this fixture goes red again. Re-widen per language
    // exactly when that language gains an `auth-guarded` producer.
    let dir = TempDir::new("zzop-http");
    dir.write(
        "cmd/server/main.go",
        "package main\n\nimport \"github.com/gin-gonic/gin\"\n\nfunc main() {\n\tr := gin.Default()\n\tr.GET(\"/admin/users\", listUsers)\n\tr.Run()\n}\n",
    );
    let out = scan(&dir);
    // Anti-vacuity leg: the Go parser really did extract the /admin provide — so the silence below is
    // attributable to the rule's `file_pattern`, not to extraction failing and proving nothing.
    let io = out.ir.ir.io.as_ref().expect("go tree yields IoFacts");
    assert!(
        io.provides.iter().any(|p| p.key.contains("/admin/users")),
        "expected the gin adapter to extract GET /admin/users: {:?}",
        io.provides
    );
    assert!(
        hits(&out, "protected-path-no-auth-evidence").is_empty(),
        "a Go /admin route must be out of this rule's scope until Go has an auth-guarded producer: {:?}",
        out.findings
    );
}

#[test]
fn auth_gate_ok_marker_on_the_same_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/routes/apiRoutes.ts",
        "declare const apiRoutes: any;\ndeclare const api: any;\napiRoutes.get(\"/api/admin/users\", api.userList); // zzop-protected-path-no-auth-evidence-ok: reviewed, gated at the API gateway layer\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "protected-path-no-auth-evidence").is_empty(),
        "{:?}",
        out.findings
    );
}
