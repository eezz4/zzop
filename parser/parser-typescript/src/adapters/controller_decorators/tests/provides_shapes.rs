//! The `controller-prefix-ref-v1` deferred-prefix fragments, the remaining never-guess skips, the
//! Nest end-to-end shape, and the `@RestController` gate.

use super::keys;
use crate::adapters::controller_decorators::{
    extract_controller_prefix_route_fragments, extract_controller_provides,
};

// --- controller-prefix-ref-v1: dotted member-expression prefix (`@Controller(RouteKey.Asset)`) ---

#[test]
fn member_expression_controller_prefix_emits_fragments_not_provides() {
    let src = concat!(
        "@Controller(RouteKey.Asset)\n",
        "class AssetController {\n",
        "  @Get(':id')\n",
        "  getById() {}\n\n",
        "  @Delete()\n",
        "  remove() {}\n",
        "}\n"
    );
    // No direct provides — resolution is deferred to assemble time.
    let provides = extract_controller_provides("asset.controller.ts", src);
    assert!(
        provides.is_empty(),
        "a member-expression prefix must never guess a provide, got: {provides:?}"
    );

    let frags = extract_controller_prefix_route_fragments("asset.controller.ts", src);
    let mut got: Vec<(String, String, String, Option<String>)> = frags
        .iter()
        .map(|f| {
            (
                f.prefix_ref.clone(),
                f.verb.clone(),
                f.path.clone(),
                f.symbol.clone(),
            )
        })
        .collect();
    got.sort();
    assert_eq!(
        got,
        vec![
            (
                "RouteKey.Asset".to_string(),
                "DELETE".to_string(),
                String::new(),
                Some("remove".to_string())
            ),
            (
                "RouteKey.Asset".to_string(),
                "GET".to_string(),
                ":id".to_string(),
                Some("getById".to_string())
            ),
        ]
    );
    let get_by_id = frags
        .iter()
        .find(|f| f.symbol.as_deref() == Some("getById"));
    assert_eq!(get_by_id.unwrap().line, 3);
}

#[test]
fn literal_controller_prefix_contributes_no_fragments() {
    let src = "@Controller('users')\nclass C {\n  @Get('active')\n  active() {}\n}\n";
    let frags = extract_controller_prefix_route_fragments("c.ts", src);
    assert!(frags.is_empty(), "{frags:?}");
    // And its provides are byte-identical to before this change.
    let out = extract_controller_provides("c.ts", src);
    assert_eq!(keys(&out), vec!["GET /users/active"]);
}

#[test]
fn call_expression_prefix_still_skips_entirely() {
    let src = "@Controller(foo())\nclass C {\n  @Get('a')\n  a() {}\n}\n";
    assert!(extract_controller_provides("c.ts", src).is_empty());
    assert!(extract_controller_prefix_route_fragments("c.ts", src).is_empty());
}

#[test]
fn deeper_dotted_chain_prefix_still_skips_the_whole_controller() {
    // `A.B.C` is not the exact two-segment shape `const_map_fragment` keys by — still a full skip,
    // not a fragment (module doc: "any OTHER non-literal shape ... still skips the whole controller").
    let src = "@Controller(RouteKey.Nested.Asset)\nclass C {\n  @Get('a')\n  a() {}\n}\n";
    assert!(extract_controller_provides("c.ts", src).is_empty());
    assert!(extract_controller_prefix_route_fragments("c.ts", src).is_empty());
}

#[test]
fn dynamic_object_path_attribute_skips_the_whole_controller() {
    let src = "@Controller({ path: getPrefix() })\nclass C {\n  @Get('a')\n  a() {}\n}\n";
    let out = extract_controller_provides("c.ts", src);
    assert!(out.is_empty());
}

#[test]
fn dynamic_method_path_skips_only_that_method() {
    let src = "@Controller('items')\nclass C {\n  @Get(dynamicPath())\n  a() {}\n  @Get('b')\n  b() {}\n}\n";
    let out = extract_controller_provides("c.ts", src);
    assert_eq!(keys(&out), vec!["GET /items/b"]);
}

#[test]
fn dynamic_version_is_best_effort_skipped_not_the_whole_controller() {
    let src =
        "@Controller({ path: 'items', version: VERSION })\nclass C {\n  @Get('a')\n  a() {}\n}\n";
    let out = extract_controller_provides("c.ts", src);
    assert_eq!(keys(&out), vec!["GET /items/a"]);
}

#[test]
fn empty_file_yields_no_provides() {
    assert!(extract_controller_provides("e.ts", "").is_empty());
}

#[test]
fn nest_shape_end_to_end() {
    let src = concat!(
        "import { Controller, Get, Post, Param, Body } from '@nestjs/common';\n\n",
        "@Controller('users')\n",
        "export class UsersController {\n",
        "  @Get()\n",
        "  findAll() {\n    return [];\n  }\n\n",
        "  @Get(':id')\n",
        "  findOne(@Param('id') id: string) {\n    return id;\n  }\n\n",
        "  @Post()\n",
        "  create(@Body() dto: unknown) {\n    return dto;\n  }\n",
        "}\n"
    );
    let out = extract_controller_provides("users.controller.ts", src);
    let mut got = keys(&out);
    got.sort();
    assert_eq!(got, vec!["GET /users", "GET /users/{}", "POST /users"]);
    let find_one = out.iter().find(|p| p.symbol.as_deref() == Some("findOne"));
    assert_eq!(find_one.unwrap().key, "GET /users/{}");
}

#[test]
fn rest_controller_gate_yields_provides_same_as_controller() {
    // `@n8n/decorators`'s `@RestController` is structurally identical to `@Controller`, under its own name.
    let src = "@RestController('/users')\nclass C {\n  @Get('/')\n  findAll() {}\n}\n";
    let out = extract_controller_provides("users.controller.ts", src);
    assert_eq!(keys(&out), vec!["GET /users"]);
    assert_eq!(out[0].symbol.as_deref(), Some("findAll"));
}

#[test]
fn rest_controller_with_leading_slash_prefix_and_path() {
    // `@n8n/decorators` gives both the class prefix and the method path a leading slash, unlike
    // the no-leading-slash `@Controller('users')` convention — `http_interface_key`'s multi-slash
    // collapse must still produce a clean single-slash key.
    let src = concat!(
        "@RestController('/mfa')\n",
        "export class MFAController {\n",
        "  @Post('/enforce-mfa')\n",
        "  @GlobalScope('user:enforceMfa')\n",
        "  async enforceMFA() {}\n",
        "}\n"
    );
    let out = extract_controller_provides("mfa.controller.ts", src);
    assert_eq!(keys(&out), vec!["POST /mfa/enforce-mfa"]);
}

#[test]
fn bare_rest_controller_with_no_parens_also_gates() {
    let src = "@RestController\nclass C {\n  @Get('/x')\n  x() {}\n}\n";
    let out = extract_controller_provides("c.ts", src);
    assert_eq!(keys(&out), vec!["GET /x"]);
}

#[test]
fn a_class_with_neither_controller_gate_still_emits_nothing() {
    // Regression guard: widening the gate set must not turn this into an unconditional pass.
    let src = "@Injectable()\nclass C {\n  @Get('a')\n  a() {}\n}\n";
    let out = extract_controller_provides("c.ts", src);
    assert!(out.is_empty(), "{out:?}");
}
