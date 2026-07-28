//! End-to-end coverage for the three fullstack io/graph native rules added alongside `duplicate-route`/
//! `route-shadowing`'s siblings (rule-pack catalog #46/#47/#48): `route-shadowing`, `mutating-route-no-auth`,
//! and `unprovided-consume` — all `zzop_rules_http`, wired into `zzop_engine::analyze::assemble` beside
//! `schema-usage`/`duplicate-route`. Same `TempDir` fixture-tree pattern as `pack_sql.rs`/
//! `analyze_callgraph.rs`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::RuleConfig;
use zzop_engine::{analyze_tree, AnalyzeOutput, EngineConfig};

struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, rel: &str, content: &str) {
        let full = self.0.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(full, content).unwrap();
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn hits<'a>(out: &'a AnalyzeOutput, rule: &str) -> Vec<&'a zzop_core::Finding> {
    out.findings.iter().filter(|f| f.rule_id == rule).collect()
}

fn scan_with(dir: &TempDir, rule_config: RuleConfig) -> AnalyzeOutput {
    analyze_tree(
        dir.path(),
        &EngineConfig {
            rule_config,
            ..EngineConfig::default()
        },
    )
}

fn scan(dir: &TempDir) -> AnalyzeOutput {
    scan_with(dir, RuleConfig::default())
}

fn disabled(rule: &str) -> RuleConfig {
    RuleConfig {
        disabled_rules: vec![rule.to_string()],
        ..RuleConfig::default()
    }
}

// ---------------------------------------------------------------------------------------------
// route-shadowing (#46)
// ---------------------------------------------------------------------------------------------

/// An earlier `:id` param route shadows a later `/items/active` literal route in the same file.
fn route_shadowing_fixture() -> TempDir {
    let dir = TempDir::new("zzop-route-shadowing");
    dir.write(
        "routes/items.ts",
        "const apiRoutes = new Hono();\napiRoutes.get(\"/items/:id\", api.getItem);\napiRoutes.get(\"/items/active\", api.listActive);\n",
    );
    dir
}

#[test]
fn earlier_param_route_shadowing_a_later_literal_route_is_flagged() {
    let dir = route_shadowing_fixture();
    let out = scan(&dir);
    let found = hits(&out, "route-shadowing");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(found[0].file, "routes/items.ts");
    assert_eq!(found[0].line, 3);
    assert_eq!(found[0].severity, zzop_core::Severity::Warning);
    assert!(found[0].message.contains("line 2"));
    assert!(found[0].message.contains("disabled_rules"));
}

#[test]
fn literal_route_registered_before_the_param_route_is_not_flagged() {
    let dir = TempDir::new("zzop-route-shadowing-negative");
    dir.write(
        "routes/items.ts",
        "const apiRoutes = new Hono();\napiRoutes.get(\"/items/active\", api.listActive);\napiRoutes.get(\"/items/:id\", api.getItem);\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "route-shadowing").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn route_shadowing_disabled_via_config_removes_the_finding() {
    let dir = route_shadowing_fixture();
    let out = scan_with(&dir, disabled("route-shadowing"));
    assert!(
        hits(&out, "route-shadowing").is_empty(),
        "{:?}",
        out.findings
    );
}

// ---------------------------------------------------------------------------------------------
// mutating-route-no-auth (#47)
// ---------------------------------------------------------------------------------------------

/// `POST /users` -> `createUser` never calls anything guard-shaped. `DELETE /users/:id` ->
/// `deleteUserGuarded` calls `requireAuth` (same-file call edge) before deleting.
fn mutating_no_auth_fixture() -> TempDir {
    let dir = TempDir::new("zzop-mutating-no-auth");
    dir.write(
        "routes/api.ts",
        "const apiRoutes = new Hono();\napiRoutes.post(\"/users\", createUser);\napiRoutes.delete(\"/users/:id\", deleteUserGuarded);\n",
    );
    dir.write(
        "routes/handlers.ts",
        "export function createUser(c) {\n  return prisma.user.create({ data: {} });\n}\n\nexport function deleteUserGuarded(c) {\n  requireAuth(c);\n  return prisma.user.delete({ where: { id: c.id } });\n}\n\nexport function requireAuth(c) {\n  return true;\n}\n",
    );
    dir
}

#[test]
fn mutating_handler_never_reaching_a_guard_is_flagged() {
    let dir = mutating_no_auth_fixture();
    let out = scan(&dir);
    let found = hits(&out, "mutating-route-no-auth");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(found[0].file, "routes/api.ts");
    assert_eq!(found[0].line, 2);
    assert_eq!(found[0].severity, zzop_core::Severity::Info);
    let data = found[0].data.as_ref().unwrap();
    assert_eq!(data["method"], "POST");
    assert_eq!(data["path"], "/users");
    assert!(found[0].message.contains("disabled_rules"));
    // Never fires for the guarded DELETE handler.
    assert!(!found.iter().any(|f| f.line == 3));
}

#[test]
fn auth_acquisition_path_route_with_no_guard_is_never_flagged() {
    let dir = TempDir::new("zzop-mutating-no-auth-acquisition");
    dir.write(
        "routes/api.ts",
        "const apiRoutes = new Hono();\napiRoutes.post(\"/api/auth/register\", registerUser);\n",
    );
    dir.write(
        "routes/handlers.ts",
        "export function registerUser(c) {\n  return prisma.user.create({ data: {} });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "mutating-route-no-auth").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn conditional_exempt_segment_with_no_auth_family_segment_still_fires() {
    // /devices/register — "register" is a conditional-tier auth-acquisition word, but no auth-family
    // segment (auth/login/signin/signup/session/oauth) appears anywhere else in the path, so this is an
    // ordinary device-registration route, not the auth-acquisition surface — it is checked normally.
    let dir = TempDir::new("zzop-mutating-no-auth-devices-register");
    dir.write(
        "routes/api.ts",
        "const apiRoutes = new Hono();\napiRoutes.post(\"/devices/register\", registerDevice);\n",
    );
    dir.write(
        "routes/handlers.ts",
        "export function registerDevice(c) {\n  return prisma.device.create({ data: {} });\n}\n",
    );
    let out = scan(&dir);
    let found = hits(&out, "mutating-route-no-auth");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(found[0].data.as_ref().unwrap()["path"], "/devices/register");
}

#[test]
fn conditional_exempt_segment_paired_with_auth_family_segment_is_exempt() {
    // /auth/register — "register" (conditional tier) paired with "auth" (auth-family) elsewhere in the
    // path IS exempt, unlike the standalone /devices/register case above.
    let dir = TempDir::new("zzop-mutating-no-auth-auth-register");
    dir.write(
        "routes/api.ts",
        "const apiRoutes = new Hono();\napiRoutes.post(\"/auth/register\", registerAccount);\n",
    );
    dir.write(
        "routes/handlers.ts",
        "export function registerAccount(c) {\n  return prisma.user.create({ data: {} });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "mutating-route-no-auth").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn author_path_route_with_no_guard_still_fires_segment_precision() {
    // Handler is deliberately named without an "auth" substring (unlike the path) so this test isolates
    // the PATH-segment exemption from the separate, unrelated guard-vocabulary name match on the handler
    // symbol itself (which would independently clear a handler literally named e.g. `updateAuthorProfile`).
    let dir = TempDir::new("zzop-mutating-no-auth-author");
    dir.write(
        "routes/api.ts",
        "const apiRoutes = new Hono();\napiRoutes.post(\"/author/profile\", patchWriterBio);\n",
    );
    dir.write(
        "routes/handlers.ts",
        "export function patchWriterBio(c) {\n  return prisma.author.update({ where: { id: c.id }, data: {} });\n}\n",
    );
    let out = scan(&dir);
    let found = hits(&out, "mutating-route-no-auth");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(found[0].severity, zzop_core::Severity::Info);
}

#[test]
fn handler_reaching_a_guard_looking_callee_is_not_flagged() {
    let dir = mutating_no_auth_fixture();
    let out = scan(&dir);
    assert!(
        !hits(&out, "mutating-route-no-auth")
            .iter()
            .any(|f| f.data.as_ref().unwrap()["path"] == "/users/{}"),
        "{:?}",
        out.findings
    );
}

#[test]
fn mutating_route_no_auth_disabled_via_config_removes_the_finding() {
    let dir = mutating_no_auth_fixture();
    let out = scan_with(&dir, disabled("mutating-route-no-auth"));
    assert!(
        hits(&out, "mutating-route-no-auth").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn mutating_route_registered_in_a_test_fixture_file_is_not_flagged() {
    // A route defined/invoked only from a __test__/__tests__ fixture file (to exercise a handler in
    // isolation) is not exposed application surface, and must not be flagged.
    let dir = TempDir::new("zzop-mutating-no-auth-test-fixture");
    dir.write(
        "routes/__tests__/api.test.ts",
        "const apiRoutes = new Hono();\napiRoutes.post(\"/users\", createUser);\n",
    );
    dir.write(
        "routes/__tests__/handlers.test.ts",
        "export function createUser(c) {\n  return prisma.user.create({ data: {} });\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "mutating-route-no-auth").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn nestjs_class_level_use_guards_exempts_every_route_in_the_controller() {
    // A class-level `@UseGuards` chain genuinely guards every route in the controller, but the
    // handler's own body never calls anything guard-named, so the call-graph BFS alone would
    // false-positive on it. This is the end-to-end test proving the fix works through the real
    // engine: real swc parse, real BFS, real `extract_controller_guarded_lines` wiring in
    // `native_rules/callgraph.rs`.
    let dir = TempDir::new("zzop-mutating-no-auth-nest-guarded");
    dir.write(
        "items.controller.ts",
        concat!(
            "import { Controller, Post, UseGuards } from '@nestjs/common';\n\n",
            "@Controller('items')\n",
            "@UseGuards(JwtAuthGuard)\n",
            "export class ItemsController {\n",
            "  @Post('x')\n",
            "  async create() {\n",
            "    return true;\n",
            "  }\n",
            "}\n"
        ),
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "mutating-route-no-auth").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn nestjs_route_with_no_use_guards_anywhere_still_fires() {
    // Companion negative fixture — proves the NestJS `@UseGuards` exemption is precise, not a blanket
    // suppression of the whole rule for NestJS-shaped trees.
    let dir = TempDir::new("zzop-mutating-no-auth-nest-unguarded");
    dir.write(
        "items.controller.ts",
        concat!(
            "import { Controller, Post } from '@nestjs/common';\n\n",
            "@Controller('items')\n",
            "export class ItemsController {\n",
            "  @Post('x')\n",
            "  async create() {\n",
            "    return true;\n",
            "  }\n",
            "}\n"
        ),
    );
    let out = scan(&dir);
    let found = hits(&out, "mutating-route-no-auth");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(found[0].file, "items.controller.ts");
}

#[test]
fn nestjs_forroutes_auth_middleware_exempts_the_covered_route() {
    // Same single-route controller as `nestjs_route_with_no_use_guards_anywhere_still_fires` (which proves
    // it FIRES without any guard) — here a sibling module binds `AuthMiddleware` to that exact route via
    // `consumer.apply(AuthMiddleware).forRoutes({path, method})`. The BFS can't see the middleware, so only
    // the forRoutes exemption can clear it: the finding must now be gone. This is the NestJS route-scoped-
    // middleware analog of the `@UseGuards` and Spring `@PreAuthorize` end-to-end tests.
    let dir = TempDir::new("zzop-mutating-no-auth-nest-forroutes");
    dir.write(
        "items.controller.ts",
        concat!(
            "import { Controller, Post } from '@nestjs/common';\n\n",
            "@Controller('items')\n",
            "export class ItemsController {\n",
            "  @Post('x')\n",
            "  async create() {\n",
            "    return true;\n",
            "  }\n",
            "}\n"
        ),
    );
    dir.write(
        "items.module.ts",
        concat!(
            "import { MiddlewareConsumer, Module, NestModule, RequestMethod } from '@nestjs/common';\n\n",
            "export class ItemsModule implements NestModule {\n",
            "  public configure(consumer: MiddlewareConsumer) {\n",
            "    consumer\n",
            "      .apply(AuthMiddleware)\n",
            "      .forRoutes({path: 'items/x', method: RequestMethod.POST});\n",
            "  }\n",
            "}\n"
        ),
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "mutating-route-no-auth").is_empty(),
        "forRoutes(AuthMiddleware) should exempt POST /items/x: {:?}",
        hits(&out, "mutating-route-no-auth")
    );
}

#[test]
fn nestjs_forroutes_with_a_non_auth_middleware_does_not_exempt() {
    // Companion negative: a LoggerMiddleware bound via forRoutes must NOT clear the route (false-clear
    // guard) — the same POST /items/x still fires, proving the exemption is auth-name-gated.
    let dir = TempDir::new("zzop-mutating-no-auth-nest-forroutes-nonauth");
    dir.write(
        "items.controller.ts",
        concat!(
            "import { Controller, Post } from '@nestjs/common';\n\n",
            "@Controller('items')\n",
            "export class ItemsController {\n",
            "  @Post('x')\n",
            "  async create() {\n",
            "    return true;\n",
            "  }\n",
            "}\n"
        ),
    );
    dir.write(
        "items.module.ts",
        concat!(
            "import { MiddlewareConsumer, Module, NestModule, RequestMethod } from '@nestjs/common';\n\n",
            "export class ItemsModule implements NestModule {\n",
            "  public configure(consumer: MiddlewareConsumer) {\n",
            "    consumer.apply(LoggerMiddleware).forRoutes({path: 'items/x', method: RequestMethod.POST});\n",
            "  }\n",
            "}\n"
        ),
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "mutating-route-no-auth").len(),
        1,
        "a non-auth middleware must not exempt: {:?}",
        out.findings
    );
}

// --- Java call-graph coverage (CALL_GRAPH_COVERED_EXTENSIONS lift) ---------------------------
//
// Real-shape regressions distilled from `corpus/oss/be-spring`'s `io.spring.api` package — the ground
// truth `mutating_route_no_auth`'s module doc names for the Java `RawCall` wiring
// (`zzop_parser_java_21::lang::calls::parse_calls` + `run_callgraph_rules`'s Java loop).

/// `CommentsApi.deleteComment`-shaped: a `RequestMapping(method = RequestMethod.DELETE)` handler whose
/// inline guard (`AuthorizationService.canWriteComment(...)`) sits inside a `.map(lambda -> {...})` body.
///
/// Writes TWO files, and the second one is load-bearing: `AuthorizationService` must be DECLARED
/// somewhere in the tree, exactly as `corpus/oss/be-spring` declares it. This fixture used to write the
/// handler alone, and on 2026-07-27 that omission is what made it go red — the rule stopped accepting a
/// guard qualifier that names no declared symbol (a phantom `#SessionDep.add` was clearing
/// unauthenticated FastAPI routes). A fixture in a SUPPRESSION path that drops a line the real corpus
/// has does not test a smaller case; it tests a different one, and the difference is the defect.
fn java_comments_api_fixture(dir: &TempDir) {
    // `corpus/oss/be-spring/src/main/java/io/spring/core/service/AuthorizationService.java`, trimmed to
    // the one static method this handler calls. Its package/type path is what the Java import resolver
    // walks to reach the guard.
    dir.write(
        "src/main/java/io/spring/core/service/AuthorizationService.java",
        concat!(
            "package io.spring.core.service;\n\n",
            "public class AuthorizationService {\n",
            "  public static boolean canWriteComment(User user, Article article, Comment comment) {\n",
            "    return user.getId().equals(comment.getUserId());\n",
            "  }\n",
            "}\n"
        ),
    );
    dir.write(
        "src/main/java/io/spring/api/CommentsApi.java",
        concat!(
            "package io.spring.api;\n\n",
            "import io.spring.core.service.AuthorizationService;\n",
            "import org.springframework.http.ResponseEntity;\n",
            "import org.springframework.web.bind.annotation.PathVariable;\n",
            "import org.springframework.web.bind.annotation.RequestMapping;\n",
            "import org.springframework.web.bind.annotation.RequestMethod;\n",
            "import org.springframework.web.bind.annotation.RestController;\n\n",
            "@RestController\n",
            "@RequestMapping(path = \"/articles/{slug}/comments\")\n",
            "public class CommentsApi {\n",
            "  private CommentRepository commentRepository;\n\n",
            "  @RequestMapping(path = \"{id}\", method = RequestMethod.DELETE)\n",
            "  public ResponseEntity deleteComment(\n",
            "      @PathVariable(\"slug\") String slug, @PathVariable(\"id\") String commentId) {\n",
            "    return commentRepository\n",
            "        .findById(slug, commentId)\n",
            "        .map(\n",
            "            comment -> {\n",
            "              if (!AuthorizationService.canWriteComment(null, null, comment)) {\n",
            "                throw new RuntimeException();\n",
            "              }\n",
            "              commentRepository.remove(comment);\n",
            "              return ResponseEntity.noContent().build();\n",
            "            })\n",
            "        .orElseThrow(RuntimeException::new);\n",
            "  }\n",
            "}\n"
        ),
    );
}

/// `CurrentUserApi.updateProfile`-shaped: a `@PutMapping` handler with NO guard anywhere in its call
/// graph — the one true positive in the be-spring ground truth.
fn java_current_user_api_fixture(dir: &TempDir) {
    dir.write(
        "src/main/java/io/spring/api/CurrentUserApi.java",
        concat!(
            "package io.spring.api;\n\n",
            "import org.springframework.http.ResponseEntity;\n",
            "import org.springframework.web.bind.annotation.PutMapping;\n",
            "import org.springframework.web.bind.annotation.RequestMapping;\n",
            "import org.springframework.web.bind.annotation.RestController;\n\n",
            "@RestController\n",
            "@RequestMapping(path = \"/user\")\n",
            "public class CurrentUserApi {\n",
            "  private UserService userService;\n\n",
            "  @PutMapping\n",
            "  public ResponseEntity updateProfile() {\n",
            "    userService.updateUser(null);\n",
            "    return ResponseEntity.ok().build();\n",
            "  }\n",
            "}\n"
        ),
    );
}

#[test]
fn java_handler_reaching_an_authorization_service_lambda_guard_is_not_flagged() {
    // CommentsApi.deleteComment: the guard is inline inside a `.map(comment -> {...})` lambda — the
    // exact shape `zzop_parser_java_21::lang::calls::parse_calls`'s "lambda bodies ARE covered" doc
    // exists to handle. Must NOT be flagged.
    let dir = TempDir::new("zzop-mutating-no-auth-java-comments");
    java_comments_api_fixture(&dir);
    let out = scan(&dir);
    assert!(
        hits(&out, "mutating-route-no-auth").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn java_handler_with_no_reachable_guard_is_flagged() {
    // CurrentUserApi.updateProfile: no guard anywhere reachable — the one true positive in the
    // be-spring ground truth this wiring was built to catch.
    let dir = TempDir::new("zzop-mutating-no-auth-java-current-user");
    java_current_user_api_fixture(&dir);
    let out = scan(&dir);
    let found = hits(&out, "mutating-route-no-auth");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(
        found[0].file,
        "src/main/java/io/spring/api/CurrentUserApi.java"
    );
    assert_eq!(found[0].data.as_ref().unwrap()["method"], "PUT");
}

/// `CurrentUserApi.updateProfile` guarded by method-level Spring `@PreAuthorize` — identical to
/// `java_current_user_api_fixture` except for the annotation. The handler body still reaches no
/// call-graph guard, so ONLY the annotation exemption can clear it.
fn java_current_user_api_preauthorize_fixture(dir: &TempDir) {
    dir.write(
        "src/main/java/io/spring/api/CurrentUserApi.java",
        concat!(
            "package io.spring.api;\n\n",
            "import org.springframework.http.ResponseEntity;\n",
            "import org.springframework.security.access.prepost.PreAuthorize;\n",
            "import org.springframework.web.bind.annotation.PutMapping;\n",
            "import org.springframework.web.bind.annotation.RequestMapping;\n",
            "import org.springframework.web.bind.annotation.RestController;\n\n",
            "@RestController\n",
            "@RequestMapping(path = \"/user\")\n",
            "public class CurrentUserApi {\n",
            "  private UserService userService;\n\n",
            "  @PutMapping\n",
            "  @PreAuthorize(\"isAuthenticated()\")\n",
            "  public ResponseEntity updateProfile() {\n",
            "    userService.updateUser(null);\n",
            "    return ResponseEntity.ok().build();\n",
            "  }\n",
            "}\n"
        ),
    );
}

#[test]
fn java_spring_method_level_preauthorize_exempts_a_mutating_route() {
    // End-to-end proof through the real engine (parser `extract_spring_guarded_lines` + `run_callgraph_
    // rules` wiring + the rule's `decorator_guarded` exemption): the SAME PUT /user handler that fires in
    // `java_handler_with_no_reachable_guard_is_flagged` is now silenced purely by its `@PreAuthorize`,
    // which the call-graph BFS structurally cannot see. This is the Java parallel of the NestJS
    // `@UseGuards` end-to-end test.
    let dir = TempDir::new("zzop-mutating-no-auth-java-preauthorize");
    java_current_user_api_preauthorize_fixture(&dir);
    let out = scan(&dir);
    assert!(
        hits(&out, "mutating-route-no-auth").is_empty(),
        "@PreAuthorize should exempt PUT /user: {:?}",
        out.findings
    );
}

#[test]
fn java_call_graph_coverage_does_not_disturb_the_ambiguous_handler_bailout() {
    // Both fixtures together: `deleteComment`/`updateProfile` are each unique in this small tree, so
    // both resolve — proving `.java` becoming call-graph-covered doesn't touch the separate
    // `resolve_handler` ambiguity gate (item (c) of the task's validation list). The guarded handler
    // stays silent, the unguarded one still fires — same outcome as the two tests above, now run
    // together in one tree.
    let dir = TempDir::new("zzop-mutating-no-auth-java-both");
    java_comments_api_fixture(&dir);
    java_current_user_api_fixture(&dir);
    let out = scan(&dir);
    let found = hits(&out, "mutating-route-no-auth");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(
        found[0].file,
        "src/main/java/io/spring/api/CurrentUserApi.java"
    );
}

/// The be-spring `WebSecurityConfig` shape: secure-by-default (`.anyRequest().authenticated()`) with a
/// `POST /widgets` permitAll exception.
fn java_security_config_fixture(dir: &TempDir) {
    dir.write(
        "src/main/java/io/spring/api/security/WebSecurityConfig.java",
        concat!(
            "package io.spring.api.security;\n\n",
            "import org.springframework.http.HttpMethod;\n",
            "import org.springframework.security.config.annotation.web.builders.HttpSecurity;\n\n",
            "public class WebSecurityConfig {\n",
            "  protected void configure(HttpSecurity http) throws Exception {\n",
            "    http.csrf().disable().authorizeRequests()\n",
            "        .antMatchers(HttpMethod.POST, \"/widgets\").permitAll()\n",
            "        .anyRequest().authenticated();\n",
            "  }\n",
            "}\n"
        ),
    );
}

/// A `POST /widgets` controller — the route the config marks `permitAll` (genuinely open).
fn java_widget_api_fixture(dir: &TempDir) {
    dir.write(
        "src/main/java/io/spring/api/WidgetApi.java",
        concat!(
            "package io.spring.api;\n\n",
            "import org.springframework.http.ResponseEntity;\n",
            "import org.springframework.web.bind.annotation.PostMapping;\n",
            "import org.springframework.web.bind.annotation.RequestMapping;\n",
            "import org.springframework.web.bind.annotation.RestController;\n\n",
            "@RestController\n",
            "@RequestMapping(path = \"/widgets\")\n",
            "public class WidgetApi {\n",
            "  @PostMapping\n",
            "  public ResponseEntity createWidget() {\n",
            "    return ResponseEntity.ok().build();\n",
            "  }\n",
            "}\n"
        ),
    );
}

#[test]
fn spring_security_global_posture_exempts_authenticated_but_not_permitall_routes() {
    // The B12 ① end-to-end proof: a secure-by-default `WebSecurityConfig` governs the tree. `PUT /user`
    // (CurrentUserApi) escapes every permitAll -> authenticated -> exempt (the measured be-spring FP,
    // which fires WITHOUT the config in `java_handler_with_no_reachable_guard_is_flagged`). `POST /widgets`
    // IS permitAll -> genuinely open -> must STILL fire. This pins that the global-posture exemption never
    // false-clears an open route.
    let dir = TempDir::new("zzop-mutating-no-auth-spring-posture");
    java_current_user_api_fixture(&dir);
    java_widget_api_fixture(&dir);
    java_security_config_fixture(&dir);
    let out = scan(&dir);
    let found = hits(&out, "mutating-route-no-auth");
    let paths: Vec<&str> = found
        .iter()
        .map(|f| f.data.as_ref().unwrap()["path"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(
        paths,
        vec!["/widgets"],
        "only the permitAll POST /widgets fires; authenticated PUT /user is exempt: {:?}",
        out.findings
    );
}

#[test]
fn spring_security_posture_absent_keeps_the_finding() {
    // Without a security config, `PUT /user` fires as before — the exemption is purely additive.
    let dir = TempDir::new("zzop-mutating-no-auth-spring-noposture");
    java_current_user_api_fixture(&dir);
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "mutating-route-no-auth").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn spring_security_posture_does_not_exempt_a_sibling_modules_routes() {
    // A monorepo: module `service-a` has a secure-by-default config; `service-b` has NONE. `service-a`'s
    // posture must NOT reach `service-b`'s genuinely-unguarded route — it is scoped to its own
    // `.../src/main/java/` source root. `service-b`'s `POST /b` must still fire.
    let dir = TempDir::new("zzop-mutating-no-auth-spring-crossapp");
    dir.write(
        "service-a/src/main/java/a/WebSecurityConfig.java",
        concat!(
            "package a;\n",
            "import org.springframework.security.config.annotation.web.builders.HttpSecurity;\n",
            "public class WebSecurityConfig {\n",
            "  protected void configure(HttpSecurity http) throws Exception {\n",
            "    http.authorizeRequests().anyRequest().authenticated();\n",
            "  }\n}\n"
        ),
    );
    dir.write(
        "service-a/src/main/java/a/AApi.java",
        concat!(
            "package a;\n",
            "import org.springframework.web.bind.annotation.PutMapping;\n",
            "import org.springframework.web.bind.annotation.RequestMapping;\n",
            "import org.springframework.web.bind.annotation.RestController;\n",
            "@RestController @RequestMapping(path = \"/a\")\n",
            "public class AApi {\n  @PutMapping public String updateA() { return \"a\"; }\n}\n"
        ),
    );
    dir.write(
        "service-b/src/main/java/b/BApi.java",
        concat!(
            "package b;\n",
            "import org.springframework.web.bind.annotation.PostMapping;\n",
            "import org.springframework.web.bind.annotation.RequestMapping;\n",
            "import org.springframework.web.bind.annotation.RestController;\n",
            "@RestController @RequestMapping(path = \"/b\")\n",
            "public class BApi {\n  @PostMapping public String createB() { return \"b\"; }\n}\n"
        ),
    );
    let out = scan(&dir);
    let paths: Vec<&str> = hits(&out, "mutating-route-no-auth")
        .iter()
        .map(|f| f.data.as_ref().unwrap()["path"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(
        paths,
        vec!["/b"],
        "service-a's posture exempts /a but must NOT reach service-b's /b: {:?}",
        out.findings
    );
}

#[test]
fn safe_get_routes_are_never_checked_for_missing_auth() {
    let dir = TempDir::new("zzop-mutating-no-auth-get");
    dir.write(
        "routes/api.ts",
        "const apiRoutes = new Hono();\napiRoutes.get(\"/users\", listUsers);\n",
    );
    dir.write(
        "routes/handlers.ts",
        "export function listUsers(c) {\n  return prisma.user.findMany();\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "mutating-route-no-auth").is_empty(),
        "{:?}",
        out.findings
    );
}

// ---------------------------------------------------------------------------------------------
// unprovided-consume (#48)
// ---------------------------------------------------------------------------------------------

/// This tree both provides `GET /api/users` and consumes it (matched, line 2) plus a nonexistent
/// `GET /api/missing` (unmatched, line 3).
fn unprovided_consume_fixture() -> TempDir {
    let dir = TempDir::new("zzop-unprovided-consume");
    dir.write(
        "routes/api.ts",
        "const apiRoutes = new Hono();\napiRoutes.get(\"/api/users\", api.listUsers);\n",
    );
    dir.write(
        "src/client.ts",
        "export function loadUsers() { return axios.get(\"/api/users\"); }\nexport function loadMissing() { return axios.get(\"/api/missing\"); }\n",
    );
    dir
}

#[test]
fn unmatched_consume_is_flagged_when_the_tree_also_provides_http_routes() {
    let dir = unprovided_consume_fixture();
    let out = scan(&dir);
    let found = hits(&out, "unprovided-consume");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(found[0].file, "src/client.ts");
    assert_eq!(found[0].line, 2);
    assert_eq!(found[0].severity, zzop_core::Severity::Info);
    assert_eq!(
        found[0].data.as_ref().unwrap()["key"].as_str(),
        Some("GET /api/missing")
    );
    assert!(found[0].message.contains("disabled_rules"));
    // The matched consume (line 1) never fires.
    assert!(!found.iter().any(|f| f.line == 1));
}

#[test]
fn static_asset_extension_consume_is_never_flagged() {
    let dir = TempDir::new("zzop-unprovided-consume-asset");
    dir.write(
        "routes/api.ts",
        "const apiRoutes = new Hono();\napiRoutes.get(\"/api/users\", api.listUsers);\n",
    );
    dir.write(
        "src/client.ts",
        "export function loadConfig() { return axios.get(\"/public/config.json\"); }\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "unprovided-consume").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn rails_style_json_api_route_with_an_api_segment_still_fires() {
    // GET /api/users.json is a real Rails-style, format-suffixed API route — the /api/ segment stops
    // the default json/xml veto from applying.
    let dir = TempDir::new("zzop-unprovided-consume-json-api");
    dir.write(
        "routes/api.ts",
        "const apiRoutes = new Hono();\napiRoutes.get(\"/api/accounts\", api.listAccounts);\n",
    );
    dir.write(
        "src/client.ts",
        "export function loadUsers() { return axios.get(\"/api/users.json\"); }\n",
    );
    let out = scan(&dir);
    let found = hits(&out, "unprovided-consume");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(
        found[0].data.as_ref().unwrap()["key"].as_str(),
        Some("GET /api/users.json")
    );
    assert_eq!(found[0].severity, zzop_core::Severity::Info);
}

#[test]
fn json_under_a_public_asset_directory_is_vetoed() {
    let dir = TempDir::new("zzop-unprovided-consume-public-json");
    dir.write(
        "routes/api.ts",
        "const apiRoutes = new Hono();\napiRoutes.get(\"/api/users\", api.listUsers);\n",
    );
    dir.write(
        "src/client.ts",
        "export function loadRecipes() { return axios.get(\"/public/recipes.json\"); }\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "unprovided-consume").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn next_js_public_prefix_stripped_json_path_is_vetoed() {
    // Next.js serves public/ files with the `public/` prefix stripped from the URL, so
    // `public/i18n/ko.json` on disk is fetched at `GET /i18n/ko.json` — no asset-directory segment
    // survives in the key for a directory allowlist to match. The inverted (API-segment) gate still
    // vetoes it since no /api/,/graphql/,/rpc/,/vN/ segment is present either.
    let dir = TempDir::new("zzop-unprovided-consume-i18n-json");
    dir.write(
        "routes/api.ts",
        "const apiRoutes = new Hono();\napiRoutes.get(\"/api/users\", api.listUsers);\n",
    );
    dir.write(
        "src/client.ts",
        "export function loadLocale() { return axios.get(\"/i18n/ko.json\"); }\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "unprovided-consume").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn pure_fe_tree_with_zero_http_provides_is_never_flagged() {
    let dir = TempDir::new("zzop-unprovided-consume-pure-fe");
    dir.write(
        "src/client.ts",
        "export function loadRemote() { return axios.get(\"/remote/thing\"); }\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "unprovided-consume").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn unprovided_consume_disabled_via_config_removes_the_finding() {
    let dir = unprovided_consume_fixture();
    let out = scan_with(&dir, disabled("unprovided-consume"));
    assert!(
        hits(&out, "unprovided-consume").is_empty(),
        "{:?}",
        out.findings
    );
}

/// This tree provides one route family (`/settle`) and consumes three unmatched keys under a totally
/// different, foreign first segment (`/orders`) — end-to-end proof that the foreign-vs-overlapping fold
/// (`rules/native/rules-http/src/unprovided_consume.rs`) reaches the real engine wiring: one aggregate
/// `unprovided-consume` finding, not three independent ones.
fn unprovided_consume_foreign_fold_fixture() -> TempDir {
    let dir = TempDir::new("zzop-unprovided-consume-foreign-fold");
    dir.write(
        "routes/settle.ts",
        "const apiRoutes = new Hono();\napiRoutes.get(\"/settle/a\", api.getA);\n",
    );
    dir.write(
        "src/client.ts",
        concat!(
            "export function loadOrder1() { return axios.get(\"/orders/1\"); }\n",
            "export function loadOrder2() { return axios.get(\"/orders/2\"); }\n",
            "export function loadOrder3() { return axios.get(\"/orders/3\"); }\n",
        ),
    );
    dir
}

#[test]
fn three_or_more_foreign_unmatched_consumes_fold_into_one_aggregate_finding_end_to_end() {
    let dir = unprovided_consume_foreign_fold_fixture();
    let out = scan(&dir);
    let found = hits(&out, "unprovided-consume");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(found[0].severity, zzop_core::Severity::Info);
    let data = found[0].data.as_ref().unwrap();
    assert_eq!(data["callCount"], 3);
    let routes = data["routes"].as_array().unwrap();
    assert_eq!(routes.len(), 3);
    for key in ["GET /orders/1", "GET /orders/2", "GET /orders/3"] {
        assert!(
            routes.iter().any(|r| r.as_str() == Some(key)),
            "missing {key} in routes: {routes:?}"
        );
        assert!(found[0].message.contains(key), "missing {key} in message");
    }
    assert!(found[0].message.contains("disabled_rules"));
}

// --- Python call-graph coverage (CALL_GRAPH_COVERED_EXTENSIONS lift) -------------------------
//
// Real-shape regressions distilled from the two FastAPI checkouts in `corpus/oss` — the ground truth
// `mutating_route_no_auth`'s module doc names for the Python `RawCall` wiring
// (`zzop_parser_python_3::lang::calls::parse_calls` + `run_callgraph_rules`'s Python loop) and for its
// inseparable other half, the `Depends(...)` guard producer
// (`zzop_parser_python_3::extract_fastapi_guarded_lines`).
//
// MEASURED, not assumed: with the guard producer neutralized, `corpus/oss/be-fastapi` reports 9
// `mutating-route-no-auth` findings and `corpus/oss/be-fastapi-fs` reports 14; with it, 0 and 3. Every
// one of the 20 suppressed routes carries a real `Depends` guard. The fixtures below are the shapes
// behind those numbers.

/// `articles_resource.py`-shaped (`corpus/oss/be-fastapi/app/api/routes/articles/articles_resource.py`):
/// a `POST` route guarded ONLY by a signature `Depends(get_current_user_authorizer())` and a `PUT` route
/// guarded ONLY by a decorator `dependencies=[Depends(check_article_modification_permissions)]`. Neither
/// guard is a call the handler body makes, so ONLY the `decorator_guarded` exemption can clear them.
fn python_fastapi_guarded_fixture(dir: &TempDir) {
    dir.write(
        "app/api/routes/articles/articles_resource.py",
        concat!(
            "from fastapi import APIRouter, Body, Depends\n",
            "from app.api.dependencies.articles import check_article_modification_permissions\n",
            "from app.api.dependencies.authentication import get_current_user_authorizer\n\n",
            "router = APIRouter()\n\n",
            "@router.post(\"\")\n",
            "async def create_new_article(\n",
            "    user: User = Depends(get_current_user_authorizer()),\n",
            ") -> ArticleInResponse:\n",
            "    slug = get_slug_for_article(article_create.title)\n",
            "    return slug\n\n",
            "@router.put(\n",
            "    \"/{slug}\",\n",
            "    dependencies=[Depends(check_article_modification_permissions)],\n",
            ")\n",
            "async def update_article_by_slug(\n",
            "    articles_repo: ArticlesRepository = Depends(get_repository(ArticlesRepository)),\n",
            ") -> ArticleInResponse:\n",
            "    return None\n"
        ),
    );
}

/// `private.py`-shaped (`corpus/oss/be-fastapi-fs/backend/app/api/routes/private.py`): the ONE mutating
/// FastAPI route in either checkout with no auth evidence anywhere — a `POST /private/users` that
/// creates a user. The true positive the lift exists to find.
fn python_fastapi_unguarded_fixture(dir: &TempDir) {
    dir.write(
        "backend/app/api/routes/private.py",
        concat!(
            "from fastapi import APIRouter\n",
            "from app.core.security import get_password_hash\n\n",
            "router = APIRouter(tags=[\"private\"], prefix=\"/private\")\n\n",
            "@router.post(\"/users/\")\n",
            "def create_user(user_in: PrivateUserCreate, session: SessionDep):\n",
            "    user = User(email=user_in.email)\n",
            "    session.add(user)\n",
            "    session.commit()\n",
            "    return user\n"
        ),
    );
}

#[test]
fn python_fastapi_route_with_no_auth_evidence_is_flagged() {
    // Proves the LIFT half on its own: before `.py` entered `CALL_GRAPH_COVERED_EXTENSIONS` this route
    // was exempted before the BFS and the rule was structurally silent on every Python tree.
    let dir = TempDir::new("zzop-mutating-no-auth-python-private");
    python_fastapi_unguarded_fixture(&dir);
    let out = scan(&dir);
    let found = hits(&out, "mutating-route-no-auth");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(found[0].file, "backend/app/api/routes/private.py");
    assert_eq!(found[0].data.as_ref().unwrap()["method"], "POST");
}

#[test]
fn python_fastapi_depends_guards_exempt_both_the_signature_and_the_decorator_shape() {
    // Proves the GUARD half end-to-end through the real engine (parser producer + `run_callgraph_rules`
    // wiring + the rule's `decorator_guarded` exemption). Neither the signature `Depends` nor the
    // decorator `dependencies=[...]` is a call edge — the BFS structurally cannot see either — so if
    // this producer regresses, BOTH routes fire. This is the Python parallel of the Java `@PreAuthorize`
    // and NestJS `@UseGuards` end-to-end tests.
    let dir = TempDir::new("zzop-mutating-no-auth-python-depends");
    python_fastapi_guarded_fixture(&dir);
    let out = scan(&dir);
    assert!(
        hits(&out, "mutating-route-no-auth").is_empty(),
        "FastAPI Depends guards should exempt both routes: {:?}",
        out.findings
    );
}

/// The `deps.py` half of `corpus/oss/be-fastapi-fs/backend/app/api/deps.py`, written at `path` — the
/// tree-wide `Annotated` guard alias that shape 4 of the FastAPI guard producer resolves across files.
fn python_fastapi_deps_alias(dir: &TempDir, path: &str, guarded: bool) {
    let injected = if guarded {
        "get_current_user"
    } else {
        "get_db"
    };
    dir.write(
        path,
        &format!(
            concat!(
                "from typing import Annotated\n",
                "from fastapi import Depends\n\n",
                "SessionDep = Annotated[Session, Depends(get_db)]\n",
                "CurrentUser = Annotated[User, Depends({injected})]\n"
            ),
            injected = injected
        ),
    );
}

/// A mutating route annotated `current_user: CurrentUser` — `items.py`-shaped
/// (`corpus/oss/be-fastapi-fs/backend/app/api/routes/items.py`). `import_from` is `None` for the case
/// where the route module never binds the name at all.
fn python_fastapi_alias_route(dir: &TempDir, path: &str, import_from: Option<&str>) {
    let import_line = import_from
        .map(|m| format!("from {m} import CurrentUser\n"))
        .unwrap_or_default();
    dir.write(
        path,
        &format!(
            concat!(
                "from fastapi import APIRouter\n",
                "{import_line}",
                "router = APIRouter(prefix=\"/items\")\n\n",
                "@router.post(\"/\")\n",
                "def create_item(current_user: CurrentUser, item_in: ItemCreate):\n",
                "    return item_in\n"
            ),
            import_line = import_line
        ),
    );
}

#[test]
fn python_fastapi_annotated_alias_resolves_across_files_and_clears_the_route() {
    // The positive path shape 4 exists for: the alias is declared once in `deps.py`, the route module
    // imports it, and nothing about the route itself says "guard" — only the cross-file resolution can
    // clear it.
    let dir = TempDir::new("zzop-mutating-no-auth-python-alias-ok");
    python_fastapi_deps_alias(&dir, "backend/app/api/deps.py", true);
    python_fastapi_alias_route(
        &dir,
        "backend/app/api/routes/items.py",
        Some("app.api.deps"),
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "mutating-route-no-auth").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn python_fastapi_alias_never_bound_in_the_route_module_clears_nothing() {
    // Same tree, minus the import: `CurrentUser` here is whatever THIS module means by it (typically a
    // same-named pydantic body model). A bare-name match against the tree-wide union cleared it anyway,
    // which made the union monotone toward suppression — one file's alias silencing the whole tree.
    let dir = TempDir::new("zzop-mutating-no-auth-python-alias-unbound");
    python_fastapi_deps_alias(&dir, "backend/app/api/deps.py", true);
    python_fastapi_alias_route(&dir, "backend/app/api/routes/items.py", None);
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "mutating-route-no-auth").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn python_fastapi_two_packages_disagreeing_about_one_alias_name_clear_nothing() {
    // The by-NAME join's conflict rule, FastAPI edition — the same drop the Django verdicts get. Two
    // packages both declare `CurrentUser`; one injects a guard, one injects `get_db`. The name is
    // undecidable tree-wide, so it resolves to nothing and BOTH routes fire (recall lost, precision
    // kept — the only safe direction for evidence that suppresses findings).
    let dir = TempDir::new("zzop-mutating-no-auth-python-alias-conflict");
    python_fastapi_deps_alias(&dir, "svc_a/api/deps.py", true);
    python_fastapi_deps_alias(&dir, "svc_b/api/deps.py", false);
    python_fastapi_alias_route(&dir, "svc_a/api/routes.py", Some("svc_a.api.deps"));
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "mutating-route-no-auth").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn python_call_graph_lift_leaves_the_unguarded_route_visible_alongside_guarded_ones() {
    // Both fixtures in one tree: the guarded routes stay silent and the unguarded one still fires, so
    // the guard producer's exemption is route-scoped, not tree-wide. Also proves `.py` joining the
    // covered set does not disturb the separate `resolve_handler_scoped` ambiguity gate — every handler
    // name here is unique tree-wide.
    let dir = TempDir::new("zzop-mutating-no-auth-python-both");
    python_fastapi_guarded_fixture(&dir);
    python_fastapi_unguarded_fixture(&dir);
    let out = scan(&dir);
    let found = hits(&out, "mutating-route-no-auth");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(found[0].file, "backend/app/api/routes/private.py");
}

#[test]
fn python_call_graph_lift_finds_a_guard_the_handler_body_actually_calls() {
    // The other half of the lift: a guard the BFS CAN see. `check_permissions` is called from the
    // handler body, so the Python `RawCall` extractor + `build_symbol_graph` must produce the edge —
    // with no `Depends` evidence anywhere, only the call graph can clear this route.
    let dir = TempDir::new("zzop-mutating-no-auth-python-bfs");
    dir.write(
        "app/routes.py",
        concat!(
            "from fastapi import APIRouter\n\n",
            "router = APIRouter()\n\n",
            "def check_permissions(user):\n",
            "    raise HTTPException(status_code=403)\n\n",
            "@router.delete(\"/widgets/{id}\")\n",
            "def remove_widget(id: str):\n",
            "    check_permissions(id)\n",
            "    return None\n"
        ),
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "mutating-route-no-auth").is_empty(),
        "a guard called from the handler body must be reachable through the Python call graph: {:?}",
        out.findings
    );
}

#[test]
fn call_graph_language_gap_is_disclosed_for_a_language_the_bfs_cannot_walk() {
    // The disclosure half of the same batch (tripwire S8): a tree whose routes are in a language with no
    // `RawCall` producer must SAY SO — naming the language, the silenced rule id, and a way to open it.
    // Python must NOT appear in that list any more, which is what makes this pin bidirectional.
    let dir = TempDir::new("zzop-callgraph-language-gap");
    dir.write(
        "internal/api/handler.go",
        concat!(
            "package api\n\n",
            "import \"github.com/gin-gonic/gin\"\n\n",
            "func Register(r *gin.Engine) {\n",
            "\tr.POST(\"/widgets\", createWidget)\n",
            "}\n"
        ),
    );
    dir.write("go.mod", "module example.com/app\n\ngo 1.22\n");
    python_fastapi_unguarded_fixture(&dir);
    let out = scan(&dir);
    let gap: Vec<&String> = out
        .warnings
        .iter()
        .filter(|w| w.starts_with("Call-graph coverage gap"))
        .collect();
    assert_eq!(gap.len(), 1, "warnings: {:?}", out.warnings);
    assert!(gap[0].contains(".go ("), "{}", gap[0]);
    assert!(gap[0].contains("mutating-route-no-auth"), "{}", gap[0]);
    assert!(
        !gap[0].contains(".py ("),
        "Python is call-graph-covered now and must not be listed as a gap: {}",
        gap[0]
    );
}
