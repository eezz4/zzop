//! Basic prefix/path shapes, per-verb mapping, the `@All` skip, and the dynamic-prefix skip.

use super::keys;
use crate::adapters::controller_decorators::extract_controller_provides;

#[test]
fn bare_controller_and_bare_get_yield_a_root_route() {
    let src = "@Controller()\nclass C {\n  @Get()\n  ping() {}\n}\n";
    let out = extract_controller_provides("c.ts", src);
    assert_eq!(keys(&out), vec!["GET /"]);
    assert_eq!(out[0].symbol.as_deref(), Some("ping"));
    assert_eq!(out[0].line, 3);
}

#[test]
fn truly_bare_controller_with_no_parens_also_gates() {
    let src = "@Controller\nclass C {\n  @Get('x')\n  x() {}\n}\n";
    let out = extract_controller_provides("c.ts", src);
    assert_eq!(keys(&out), vec!["GET /x"]);
}

#[test]
fn controller_string_prefix_and_method_path_join() {
    let src = "@Controller('users')\nclass C {\n  @Get('active')\n  active() {}\n}\n";
    let out = extract_controller_provides("c.ts", src);
    assert_eq!(keys(&out), vec!["GET /users/active"]);
}

#[test]
fn controller_object_path_attribute_is_the_prefix() {
    let src = "@Controller({ path: 'users' })\nclass C {\n  @Get('active')\n  active() {}\n}\n";
    let out = extract_controller_provides("c.ts", src);
    assert_eq!(keys(&out), vec!["GET /users/active"]);
}

#[test]
fn controller_object_version_prefixes_a_v_segment() {
    let src = "@Controller({ path: 'users', version: '1' })\nclass C {\n  @Get('active')\n  active() {}\n}\n";
    let out = extract_controller_provides("c.ts", src);
    assert_eq!(keys(&out), vec!["GET /v1/users/active"]);
}

#[test]
fn every_method_decorator_maps_to_its_own_verb() {
    let src = concat!(
        "@Controller('items')\n",
        "class C {\n",
        "  @Get('a') a() {}\n",
        "  @Post('b') b() {}\n",
        "  @Put('c') c() {}\n",
        "  @Delete('d') d() {}\n",
        "  @Patch('e') e() {}\n",
        "}\n"
    );
    let out = extract_controller_provides("c.ts", src);
    let mut got = keys(&out);
    got.sort();
    assert_eq!(
        got,
        vec![
            "DELETE /items/d",
            "GET /items/a",
            "PATCH /items/e",
            "POST /items/b",
            "PUT /items/c",
        ]
    );
}

#[test]
fn path_param_is_normalized_by_http_interface_key() {
    let src = "@Controller('users')\nclass C {\n  @Get(':id')\n  x() {}\n}\n";
    let out = extract_controller_provides("c.ts", src);
    assert_eq!(keys(&out), vec!["GET /users/{}"]);
}

#[test]
fn array_of_paths_yields_one_provide_per_entry() {
    let src = "@Controller('items')\nclass C {\n  @Get(['a', 'b'])\n  x() {}\n}\n";
    let out = extract_controller_provides("c.ts", src);
    let mut got = keys(&out);
    got.sort();
    assert_eq!(got, vec!["GET /items/a", "GET /items/b"]);
}

#[test]
fn all_decorator_is_skipped_not_guessed() {
    let src = "@Controller('items')\nclass C {\n  @All('x')\n  x() {}\n}\n";
    let out = extract_controller_provides("c.ts", src);
    assert!(
        out.is_empty(),
        "@All must never guess-emit a verb, got: {out:?}"
    );
}

#[test]
fn other_decorators_alongside_the_route_decorator_do_not_block_it() {
    let src =
        "@Controller('items')\nclass C {\n  @UseGuards(AuthGuard)\n  @Get('a')\n  a() {}\n}\n";
    let out = extract_controller_provides("c.ts", src);
    assert_eq!(keys(&out), vec!["GET /items/a"]);
}

#[test]
fn a_class_without_controller_emits_nothing() {
    let src = "class C {\n  @Get('a')\n  a() {}\n}\n";
    let out = extract_controller_provides("c.ts", src);
    assert!(out.is_empty());
}

#[test]
fn a_method_with_no_route_decorator_emits_nothing() {
    let src = "@Controller('items')\nclass C {\n  helper() {}\n}\n";
    let out = extract_controller_provides("c.ts", src);
    assert!(out.is_empty());
}

#[test]
fn dynamic_controller_prefix_skips_the_whole_controller() {
    let src = "@Controller(PREFIX)\nclass C {\n  @Get('a')\n  a() {}\n}\n";
    let out = extract_controller_provides("c.ts", src);
    assert!(
        out.is_empty(),
        "a dynamic class prefix must never guess a path, got: {out:?}"
    );
}
