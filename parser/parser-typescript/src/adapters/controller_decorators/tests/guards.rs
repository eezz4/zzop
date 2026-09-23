//! `extract_controller_guarded_lines` coverage — class-level vs method-level `@UseGuards`, the
//! non-controller gate, and the deliberate non-widening to `@GlobalScope`/`@Licensed`.

use crate::adapters::controller_decorators::{
    extract_controller_guarded_lines, extract_controller_provides,
};

#[test]
fn class_level_use_guards_covers_every_route_in_the_controller() {
    let src = concat!(
        "@Controller('items')\n",
        "@UseGuards(JwtAuthGuard)\n",
        "class C {\n",
        "  @Get('a')\n",
        "  a() {}\n\n",
        "  @Post('b')\n",
        "  b() {}\n",
        "}\n"
    );
    let out = extract_controller_provides("c.ts", src);
    let a_line = out
        .iter()
        .find(|p| p.symbol.as_deref() == Some("a"))
        .unwrap()
        .line;
    let b_line = out
        .iter()
        .find(|p| p.symbol.as_deref() == Some("b"))
        .unwrap()
        .line;
    let guarded = extract_controller_guarded_lines("c.ts", src);
    assert!(guarded.contains(&a_line), "{guarded:?}");
    assert!(guarded.contains(&b_line), "{guarded:?}");
}

#[test]
fn method_level_use_guards_covers_only_that_route() {
    let src = concat!(
        "@Controller('items')\n",
        "class C {\n",
        "  @UseGuards(JwtAuthGuard)\n",
        "  @Get('a')\n",
        "  a() {}\n\n",
        "  @Post('b')\n",
        "  b() {}\n",
        "}\n"
    );
    let out = extract_controller_provides("c.ts", src);
    let a_line = out
        .iter()
        .find(|p| p.symbol.as_deref() == Some("a"))
        .unwrap()
        .line;
    let b_line = out
        .iter()
        .find(|p| p.symbol.as_deref() == Some("b"))
        .unwrap()
        .line;
    let guarded = extract_controller_guarded_lines("c.ts", src);
    assert!(guarded.contains(&a_line), "{guarded:?}");
    assert!(
        !guarded.contains(&b_line),
        "sibling unguarded route must not be in the guarded set: {guarded:?}"
    );
}

#[test]
fn no_use_guards_anywhere_yields_an_empty_set() {
    let src = "@Controller('items')\nclass C {\n  @Get('a')\n  a() {}\n}\n";
    let guarded = extract_controller_guarded_lines("c.ts", src);
    assert!(guarded.is_empty(), "{guarded:?}");
}

#[test]
fn a_non_controller_class_with_use_guards_yields_an_empty_set() {
    let src = "class C {\n  @UseGuards(JwtAuthGuard)\n  @Get('a')\n  a() {}\n}\n";
    let guarded = extract_controller_guarded_lines("c.ts", src);
    assert!(guarded.is_empty(), "{guarded:?}");
}

#[test]
fn class_level_guards_cover_a_wildcard_post_route() {
    // Covers a wildcard POST handler whose own body never calls anything guard-named.
    let src = concat!(
        "@Controller('rest')\n",
        "@UseGuards(JwtAuthGuard, WorkspaceAuthGuard)\n",
        "export class RestApiCoreController {\n",
        "  @Post('*path')\n",
        "  async handleApiPost() {}\n",
        "}\n"
    );
    let out = extract_controller_provides("rest.controller.ts", src);
    let post_line = out
        .iter()
        .find(|p| p.symbol.as_deref() == Some("handleApiPost"))
        .unwrap()
        .line;
    let guarded = extract_controller_guarded_lines("rest.controller.ts", src);
    assert!(guarded.contains(&post_line), "{guarded:?}");
}

#[test]
fn rest_controller_gate_participates_in_guarded_lines_the_same_as_controller() {
    let src = "@RestController('/items')\n@UseGuards(JwtAuthGuard)\nclass C {\n  @Post('/a')\n  a() {}\n}\n";
    let out = extract_controller_provides("c.ts", src);
    let a_line = out[0].line;
    let guarded = extract_controller_guarded_lines("c.ts", src);
    assert!(guarded.contains(&a_line), "{guarded:?}");
}

#[test]
fn global_scope_and_licensed_decorators_are_not_recognized_as_use_guards() {
    // Deliberate non-widening (module doc "Known residual"): only a literal `@UseGuards` counts.
    let src = concat!(
        "@RestController('/users')\n",
        "class C {\n",
        "  @Post('/:id/role')\n",
        "  @GlobalScope('user:changeRole')\n",
        "  @Licensed('feat:advancedPermissions')\n",
        "  changeRole() {}\n",
        "}\n"
    );
    let out = extract_controller_provides("users.controller.ts", src);
    let route_line = out[0].line;
    let guarded = extract_controller_guarded_lines("users.controller.ts", src);
    assert!(
        !guarded.contains(&route_line),
        "GlobalScope/Licensed must not be treated as UseGuards coverage: {guarded:?}"
    );
}

/// The HOUSE decorator, and the whole reason the vocabulary widened (2026-09-05). A NestJS codebase that
/// has wrapped its guard once writes `@Authenticated()`, not `@UseGuards(AuthGuard)` — and a
/// measurement that opened every first-screen row of three real projects found
/// `http/protected-path-no-auth-evidence` wrong on 4 of 4, with this as one of the three shapes it could
/// not see. The line reaches that rule through `decorator_guarded` -> `mint_auth_guarded`, which turns
/// a guarded line into the `auth-guarded` attribute the rule's `attr_absent` gate reads.
#[test]
fn a_house_decorator_whose_name_says_it_authenticates_covers_its_controller() {
    for name in [
        "Authenticated",
        "AuthGuard",
        "Authorized",
        "RequireAuthentication",
    ] {
        let src = format!(
            "@Controller('admin')
@{name}()
export class AdminController {{
  @Get('users')
               listUsers() {{}}
}}
"
        );
        let out = extract_controller_provides("admin.controller.ts", &src);
        let route_line = out[0].line;
        let guarded = extract_controller_guarded_lines("admin.controller.ts", &src);
        assert!(
            guarded.contains(&route_line),
            "@{name} names the ACT of authenticating, so it is auth evidence: {guarded:?}"
        );
    }
}

/// The two families the stem requirement keeps out WITHOUT a veto list, which is why the accept side is
/// a stem rather than the bare word `auth`.
///
/// DOCUMENTATION: every `@nestjs/swagger` security decorator ends in an auth word and enforces nothing
/// at all — it writes an OpenAPI document. Reading one as a guard would clear a route whose only "auth"
/// is a line in a spec file.
///
/// NEGATION, and the worse of the two: an opt-OUT marker carries the same word and means its opposite,
/// so it sits on precisely the route a reader most needs told about. A vocabulary that read one as
/// evidence would go silent exactly where it matters most.
#[test]
fn documentation_and_opt_out_decorators_are_not_auth_evidence() {
    for name in [
        // @nestjs/swagger — documents a scheme, enforces nothing.
        "ApiBearerAuth",
        "ApiOAuth2",
        "ApiSecurity",
        "ApiCookieAuth",
        // opt-OUT markers — the same words, the opposite meaning.
        "SkipAuth",
        "NoAuth",
        "OptionalAuth",
        "BypassAuth",
        // the ONE spelling that gets past the stem and still negates, so it is vetoed outright.
        "Unauthenticated",
        "Unauthorized",
    ] {
        let src = format!(
            "@Controller('admin')
@{name}()
export class AdminController {{
  @Get('users')
               listUsers() {{}}
}}
"
        );
        let out = extract_controller_provides("admin.controller.ts", &src);
        let route_line = out[0].line;
        let guarded = extract_controller_guarded_lines("admin.controller.ts", &src);
        assert!(
            !guarded.contains(&route_line),
            "@{name} enforces nothing (or negates); reading it as evidence silences the route that              needs the finding: {guarded:?}"
        );
    }
}

/// The NARROWING that makes this vocabulary its own rather than a reuse of the middleware one. NestJS
/// puts authorization METADATA beside the guard, not instead of it — `@Roles('admin')` carries the
/// policy and a `@UseGuards(RolesGuard)` enforces it — so a controller with the metadata and no guard
/// is exactly the shape the consuming rules exist to report. `router_mounts::guard`'s middleware
/// vocabulary accepts `permission`, `acl`, `token` and `loggedin`; every one of them would clear this.
#[test]
fn authorization_metadata_with_no_guard_beside_it_is_not_auth_evidence() {
    for name in [
        "Roles('admin')",
        "Permissions('user:write')",
        "Scopes('admin')",
        "RequiresAcl('admin')",
    ] {
        let src = format!(
            "@Controller('admin')
@{name}
export class AdminController {{
  @Get('users')
               listUsers() {{}}
}}
"
        );
        let out = extract_controller_provides("admin.controller.ts", &src);
        let route_line = out[0].line;
        let guarded = extract_controller_guarded_lines("admin.controller.ts", &src);
        assert!(
            !guarded.contains(&route_line),
            "{name} is policy metadata a guard consumes, not a guard: {guarded:?}"
        );
    }
}

/// The ACCEPTED COST, pinned so it is a decision rather than a surprise: names that mean a guard but do
/// not carry the stem go unrecognized, and the finding keeps firing. Under-recognition is the direction
/// this predicate chooses on purpose — a human still looks, where a false guard would let nobody.
#[test]
fn an_auth_named_decorator_without_the_stem_is_deliberately_not_recognized() {
    for name in ["RequireAuth", "JwtAuth", "Auth"] {
        let src = format!(
            "@Controller('admin')
@{name}()
export class AdminController {{
  @Get('users')
               listUsers() {{}}
}}
"
        );
        let out = extract_controller_provides("admin.controller.ts", &src);
        let route_line = out[0].line;
        let guarded = extract_controller_guarded_lines("admin.controller.ts", &src);
        assert!(
            !guarded.contains(&route_line),
            "@{name} is under-recognized by design; if this ever flips, the doc's accepted-cost              paragraph is what has to change with it: {guarded:?}"
        );
    }
}
