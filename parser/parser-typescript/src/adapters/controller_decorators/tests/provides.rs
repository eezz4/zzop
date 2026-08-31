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

// ---------------------------------------------------------------------------------------------
// `route_version` — the NON-URI version discriminator (`route-version-v1`). A `version:` whose value
// is not a string literal is header/media-type versioning: it never reaches the URL, so it must not
// touch the key, and it is carried verbatim so a consuming rule can tell two version scopes apart.
// ---------------------------------------------------------------------------------------------

#[test]
fn non_literal_controller_version_never_becomes_a_path_segment() {
    let src = "@Controller({ path: 'bookings', version: VERSION_2024_08_13_VALUE })\nclass C {\n  @Get()\n  list() {}\n}\n";
    let out = extract_controller_provides("c.ts", src);
    assert_eq!(keys(&out), vec!["GET /bookings"]);
    assert_eq!(
        out[0].route_version.as_deref(),
        Some("VERSION_2024_08_13_VALUE")
    );
}

#[test]
fn an_array_controller_version_is_sorted_and_whitespace_stripped() {
    let src = "@Controller({ path: 'bookings', version: [VERSION_2024_06_14, VERSION_2024_04_15] })\nclass C {\n  @Get()\n  list() {}\n}\n";
    let out = extract_controller_provides("c.ts", src);
    assert_eq!(keys(&out), vec!["GET /bookings"]);
    assert_eq!(
        out[0].route_version.as_deref(),
        Some("[VERSION_2024_04_15,VERSION_2024_06_14]")
    );
}

#[test]
fn a_literal_version_both_prefixes_the_path_and_records_the_discriminator() {
    let src = "@Controller({ path: 'users', version: '1' })\nclass C {\n  @Get()\n  list() {}\n}\n";
    let out = extract_controller_provides("c.ts", src);
    assert_eq!(keys(&out), vec!["GET /v1/users"]);
    assert_eq!(out[0].route_version.as_deref(), Some("1"));
}

#[test]
fn a_controller_with_no_version_carries_no_discriminator_at_all() {
    for src in [
        "@Controller('users')\nclass C {\n  @Get()\n  list() {}\n}\n",
        "@Controller({ path: 'users' })\nclass C {\n  @Get()\n  list() {}\n}\n",
    ] {
        let out = extract_controller_provides("c.ts", src);
        assert_eq!(keys(&out), vec!["GET /users"]);
        assert_eq!(
            out[0].route_version, None,
            "absence must be absence, never Some(\"\") — src: {src}"
        );
    }
}

/// The class scope MASKS a method-level `@Version()` — pinned as the value that lands, not as an
/// absence. The previous version of this test used a controller with no `version` at all, so
/// `route_version` was `None` for a reason that had nothing to do with the method decorator and the
/// test would have passed against a hard-coded `None`.
///
/// This is the documented behaviour and it is a HAZARD, not a safe default: `VERSION_NEUTRAL` makes a
/// handler answer at every version, so stamping it with the class scope under-reports its reach — see
/// the module doc's "Known limits" for the cal.com measurement that leaves it unread in v1 anyway.
#[test]
fn a_method_level_version_decorator_is_masked_by_the_class_scope() {
    let src = "@Controller({ path: 'x', version: VERSION_2024_04_15_VALUE })\nclass C {\n  @Version(VERSION_NEUTRAL)\n  @Get()\n  list() {}\n}\n";
    let out = extract_controller_provides("c.ts", src);
    assert_eq!(keys(&out), vec!["GET /x"]);
    assert_eq!(
        out[0].route_version.as_deref(),
        Some("VERSION_2024_04_15_VALUE"),
        "the CLASS scope must land; the method-level override is not read"
    );
}

/// The bracket form is normalized as well as the element order. Nest declares `version` as
/// `string | string[]` and wraps a scalar itself, so `version: [X]` and `version: X` are the same
/// `VersionValue` — and two controllers at ONE path spelling one scope the two ways must produce ONE
/// discriminator text. A `"[X]"`/`"X"` split would read as a difference and buy the demotion that
/// `duplicate_route::tests::the_same_version_on_both_sides_stays_a_warning` exists to prevent, on a
/// pair that declares the same version.
#[test]
fn a_single_element_array_version_is_the_same_discriminator_as_the_scalar() {
    let wrapped = extract_controller_provides(
        "a.ts",
        "@Controller({ path: 'bookings', version: [VERSION_2024_08_13_VALUE] })\nclass A {\n  @Get()\n  list() {}\n}\n",
    );
    let bare = extract_controller_provides(
        "b.ts",
        "@Controller({ path: 'bookings', version: VERSION_2024_08_13_VALUE })\nclass B {\n  @Get()\n  list() {}\n}\n",
    );
    assert_eq!(keys(&wrapped), keys(&bare), "the pair must share one key");
    assert_eq!(
        wrapped[0].route_version.as_deref(),
        Some("VERSION_2024_08_13_VALUE")
    );
    assert_eq!(
        wrapped[0].route_version, bare[0].route_version,
        "one scope, two spellings, one discriminator"
    );
}
