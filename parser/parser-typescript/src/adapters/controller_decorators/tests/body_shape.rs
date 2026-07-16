//! `body-shape-v1`: `@Body()` request-body contract capture — never-guess fallthroughs and the
//! deferred-prefix fragment carrying its body too.

use crate::adapters::controller_decorators::{
    extract_controller_prefix_route_fragments, extract_controller_provides,
};
use zzop_core::{IoProvide, ProvideBodyShape};

fn body_of<'a>(out: &'a [IoProvide], symbol: &str) -> Option<&'a ProvideBodyShape> {
    out.iter()
        .find(|p| p.symbol.as_deref() == Some(symbol))
        .and_then(|p| p.body.as_ref())
}

#[test]
fn body_decorator_with_string_sub_key_and_dto_type_is_captured() {
    let src = concat!(
        "@Controller('users')\n",
        "class C {\n",
        "  @Post()\n",
        "  create(@Body('user') u: CreateUserDto) {}\n",
        "}\n"
    );
    let out = extract_controller_provides("c.ts", src);
    let body = body_of(&out, "create").expect("body must be captured");
    assert_eq!(body.sub_key.as_deref(), Some("user"));
    assert_eq!(body.dto_ref.as_deref(), Some("CreateUserDto"));
    assert!(body.fields.is_empty());
    assert!(!body.complete);
}

#[test]
fn bare_body_decorator_yields_whole_body_sub_key_none() {
    let src = concat!(
        "@Controller('users')\n",
        "class C {\n",
        "  @Post()\n",
        "  create(@Body() dto: CreateUserDto) {}\n",
        "}\n"
    );
    let out = extract_controller_provides("c.ts", src);
    let body = body_of(&out, "create").expect("body must be captured");
    assert_eq!(body.sub_key, None);
    assert_eq!(body.dto_ref.as_deref(), Some("CreateUserDto"));
}

#[test]
fn body_decorator_with_no_type_annotation_yields_no_body() {
    let src = concat!(
        "@Controller('users')\n",
        "class C {\n",
        "  @Post()\n",
        "  create(@Body() dto) {}\n",
        "}\n"
    );
    let out = extract_controller_provides("c.ts", src);
    assert_eq!(body_of(&out, "create"), None);
}

#[test]
fn two_body_decorators_on_one_method_yield_no_body() {
    let src = concat!(
        "@Controller('users')\n",
        "class C {\n",
        "  @Post()\n",
        "  create(@Body('a') a: A, @Body('b') b: B) {}\n",
        "}\n"
    );
    let out = extract_controller_provides("c.ts", src);
    assert_eq!(body_of(&out, "create"), None);
}

#[test]
fn non_literal_body_decorator_argument_yields_no_body() {
    let src = concat!(
        "@Controller('users')\n",
        "class C {\n",
        "  @Post()\n",
        "  create(@Body(v) dto: CreateUserDto) {}\n",
        "}\n"
    );
    let out = extract_controller_provides("c.ts", src);
    assert_eq!(body_of(&out, "create"), None);
}

#[test]
fn primitive_type_annotation_yields_no_body() {
    let src = concat!(
        "@Controller('users')\n",
        "class C {\n",
        "  @Post()\n",
        "  create(@Body('email') e: string) {}\n",
        "}\n"
    );
    let out = extract_controller_provides("c.ts", src);
    assert_eq!(body_of(&out, "create"), None);
}

#[test]
fn non_body_routes_keep_body_none() {
    let src = "@Controller('items')\nclass C {\n  @Get('a')\n  a() {}\n}\n";
    let out = extract_controller_provides("c.ts", src);
    assert_eq!(body_of(&out, "a"), None);
}

#[test]
fn prefix_ref_fragment_carries_body_too() {
    let src = concat!(
        "@Controller(RouteKey.Asset)\n",
        "class AssetController {\n",
        "  @Post()\n",
        "  create(@Body('user') u: CreateUserDto) {}\n",
        "}\n"
    );
    let frags = extract_controller_prefix_route_fragments("asset.controller.ts", src);
    let create = frags
        .iter()
        .find(|f| f.symbol.as_deref() == Some("create"))
        .expect("fragment must be emitted");
    let body = create.body.as_ref().expect("body must be captured");
    assert_eq!(body.sub_key.as_deref(), Some("user"));
    assert_eq!(body.dto_ref.as_deref(), Some("CreateUserDto"));
}
