//! `response-shape-v1`: declared return-type response-contract capture — the capturable shapes
//! (`Promise<X>` unwrapped, plain identifier), every never-guess fallthrough, the NO-ANNOTATION
//! sentinel, and the deferred-prefix fragment carrying its response too.

use crate::adapters::controller_decorators::{
    extract_controller_prefix_route_fragments, extract_controller_provides,
};
use zzop_core::{IoProvide, ProvideResponseShape};

fn response_of<'a>(out: &'a [IoProvide], symbol: &str) -> Option<&'a ProvideResponseShape> {
    out.iter()
        .find(|p| p.symbol.as_deref() == Some(symbol))
        .and_then(|p| p.response.as_ref())
}

fn one_route(ret: &str) -> String {
    format!(
        "@Controller('users')\nclass C {{\n  @Get(':id')\n  findOne(){ret} {{ return null as any; }}\n}}\n"
    )
}

#[test]
fn promise_wrapped_dto_return_type_is_unwrapped_and_captured() {
    let out = extract_controller_provides("c.ts", &one_route(": Promise<UserDto>"));
    let resp = response_of(&out, "findOne").expect("response must be captured");
    assert_eq!(resp.dto_ref.as_deref(), Some("UserDto"));
    assert!(resp.fields.is_empty());
    assert!(!resp.complete);
}

#[test]
fn plain_identifier_return_type_is_captured() {
    let out = extract_controller_provides("c.ts", &one_route(": UserDto"));
    let resp = response_of(&out, "findOne").expect("response must be captured");
    assert_eq!(resp.dto_ref.as_deref(), Some("UserDto"));
}

#[test]
fn missing_return_type_yields_the_undeclared_sentinel() {
    // The zero-information shape (dto_ref None + fields empty) IS the "handler declared no return
    // type" sentinel — assemble strips it and discloses "declare a return type to enable this
    // analysis" (`zzop_core::ProvideResponseShape`'s doc). Never a silent absence.
    let out = extract_controller_provides("c.ts", &one_route(""));
    let resp = response_of(&out, "findOne").expect("sentinel must be present");
    assert_eq!(resp.dto_ref, None);
    assert!(resp.fields.is_empty());
}

#[test]
fn uncapturable_annotations_yield_no_response_never_guessed() {
    // Declared-but-unreadable shapes are None (never-guess), NOT the sentinel: the sentinel's
    // disclosure says "declare a return type", which would be wrong advice for a handler that did.
    for ret in [
        ": Promise<string>",         // keyword payload
        ": Promise<UserDto[]>",      // array payload
        ": Promise<UserDto | null>", // union payload
        ": Promise<Foo<T>>",         // generic payload
        ": Promise<A.B>",            // qualified payload
        ": Observable<UserDto>",     // non-Promise wrapper — unwrapping it would guess
        ": Partial<UserDto>",        // ditto (fields would wrongly read as required)
        ": UserDto[]",               // array
        ": string",                  // keyword
        ": Promise",                 // bare Promise names no payload
        ": Promise<UserDto, Extra>", // arity != 1
    ] {
        let out = extract_controller_provides("c.ts", &one_route(ret));
        assert_eq!(
            response_of(&out, "findOne"),
            None,
            "expected never-guess None for `{ret}`"
        );
    }
}

#[test]
fn deferred_prefix_fragment_carries_the_response_too() {
    let src = concat!(
        "@Controller(RouteKey.Asset)\n",
        "class C {\n",
        "  @Get()\n",
        "  list(): Promise<AssetDto> { return null as any; }\n",
        "}\n"
    );
    let frags = extract_controller_prefix_route_fragments("c.ts", src);
    assert_eq!(frags.len(), 1);
    let resp = frags[0].response.as_ref().expect("fragment response");
    assert_eq!(resp.dto_ref.as_deref(), Some("AssetDto"));
}

#[test]
fn body_and_response_are_independent_axes() {
    let src = concat!(
        "@Controller('users')\n",
        "class C {\n",
        "  @Post()\n",
        "  create(@Body() dto: CreateUserDto): Promise<UserDto> { return null as any; }\n",
        "}\n"
    );
    let out = extract_controller_provides("c.ts", src);
    let p = out
        .iter()
        .find(|p| p.symbol.as_deref() == Some("create"))
        .unwrap();
    assert_eq!(
        p.body.as_ref().unwrap().dto_ref.as_deref(),
        Some("CreateUserDto")
    );
    assert_eq!(
        p.response.as_ref().unwrap().dto_ref.as_deref(),
        Some("UserDto")
    );
}
