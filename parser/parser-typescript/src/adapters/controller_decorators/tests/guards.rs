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
