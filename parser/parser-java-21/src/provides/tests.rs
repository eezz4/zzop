//! Ported parity fixtures from `zzop_parser_java::provides::tests` — every method-level mapping-
//! annotation shape (bare / positional string / `value=` / `path=`), class-level `@RequestMapping`
//! prefixing, `@RequestMapping` with an explicit `method` attribute, the ambiguous-no-method-attribute
//! skip, the `@RestController`/`@Controller` class gate (including the negative), and a multi-method
//! controller class shape end to end — same expectations as the old lexical crate, `extract_http_provides`
//! having the exact same signature/behavior contract.
use zzop_core::IoProvide;

use super::annotations::{METHOD_ANNOTATIONS, REQUEST_METHOD_NAMES};
use super::*;

fn keys(out: &[IoProvide]) -> Vec<String> {
    out.iter().map(|p| p.key.clone()).collect()
}

#[test]
fn bare_get_mapping_on_a_rest_controller_yields_an_empty_path_route() {
    let src = "@RestController\nclass C {\n  @GetMapping\n  void ping() {}\n}\n";
    let out = extract_http_provides("C.java", src);
    assert_eq!(keys(&out), vec!["GET /"]);
    assert_eq!(out[0].symbol.as_deref(), Some("ping"));
    assert_eq!(out[0].line, 3);
}

#[test]
fn positional_string_arg_is_the_path() {
    let src = "@RestController\nclass C {\n  @GetMapping(\"/x\")\n  void x() {}\n}\n";
    let out = extract_http_provides("C.java", src);
    assert_eq!(keys(&out), vec!["GET /x"]);
}

#[test]
fn value_named_attribute_is_the_path() {
    let src = "@RestController\nclass C {\n  @PostMapping(value = \"/x\")\n  void x() {}\n}\n";
    let out = extract_http_provides("C.java", src);
    assert_eq!(keys(&out), vec!["POST /x"]);
}

#[test]
fn path_named_attribute_is_the_path() {
    let src = "@RestController\nclass C {\n  @PutMapping(path = \"/x\")\n  void x() {}\n}\n";
    let out = extract_http_provides("C.java", src);
    assert_eq!(keys(&out), vec!["PUT /x"]);
}

#[test]
fn every_mapping_annotation_maps_to_its_own_verb() {
    let src = "@RestController\nclass C {\n  @GetMapping(\"/a\")\n  void a() {}\n  @PostMapping(\"/b\")\n  void b() {}\n  @PutMapping(\"/c\")\n  void c() {}\n  @DeleteMapping(\"/d\")\n  void d() {}\n  @PatchMapping(\"/e\")\n  void e() {}\n}\n";
    let out = extract_http_provides("C.java", src);
    let mut got = keys(&out);
    got.sort();
    assert_eq!(
        got,
        vec!["DELETE /d", "GET /a", "PATCH /e", "POST /b", "PUT /c"]
    );
}

#[test]
fn class_level_request_mapping_prefixes_every_method_path() {
    let src = "@RequestMapping(\"/authen\")\n@RestController\nclass CtrlAuthen {\n  @GetMapping(\"/getUserInfo\")\n  UserInfo getUserInfo() { return null; }\n}\n";
    let out = extract_http_provides("CtrlAuthen.java", src);
    assert_eq!(keys(&out), vec!["GET /authen/getUserInfo"]);
}

#[test]
fn class_level_prefix_via_value_attribute_also_works() {
    let src = "@RequestMapping(value = \"/authen\")\n@RestController\nclass C {\n  @GetMapping(\"/x\")\n  void x() {}\n}\n";
    let out = extract_http_provides("C.java", src);
    assert_eq!(keys(&out), vec!["GET /authen/x"]);
}

#[test]
fn request_mapping_with_an_explicit_method_attribute_resolves_the_verb() {
    let src = "@RestController\nclass C {\n  @RequestMapping(value=\"/x\", method = RequestMethod.GET)\n  void x() {}\n}\n";
    let out = extract_http_provides("C.java", src);
    assert_eq!(keys(&out), vec!["GET /x"]);
}

#[test]
fn request_mapping_split_across_lines_still_resolves() {
    let src = "@RestController\nclass C {\n  @RequestMapping(value=\"/x\",\n    method = RequestMethod.POST)\n  void x() {}\n}\n";
    let out = extract_http_provides("C.java", src);
    assert_eq!(keys(&out), vec!["POST /x"]);
}

#[test]
fn method_annotation_verbs_are_pinned_to_the_core_verb_set() {
    let table: std::collections::BTreeSet<&str> =
        METHOD_ANNOTATIONS.iter().map(|(_, verb)| *verb).collect();
    let core: std::collections::BTreeSet<&str> =
        zzop_core::HTTP_KEY_VERBS.iter().copied().collect();
    assert_eq!(
        table, core,
        "METHOD_ANNOTATIONS' verb column drifted from zzop_core::HTTP_KEY_VERBS — change both \
         deliberately or neither"
    );
}

#[test]
fn bare_request_method_names_are_a_deliberate_superset_of_the_core_verb_set() {
    for verb in zzop_core::HTTP_KEY_VERBS {
        assert!(
            REQUEST_METHOD_NAMES.contains(verb),
            "core keying verb {verb} missing from Spring's RequestMethod name set"
        );
    }
    assert_eq!(
        REQUEST_METHOD_NAMES.len(),
        8,
        "REQUEST_METHOD_NAMES must stay exactly Spring's RequestMethod enum, not drift toward \
         the core keying set"
    );
}

#[test]
fn request_mapping_with_a_statically_imported_bare_method_constant_resolves() {
    let src = "@RestController\nclass C {\n  @RequestMapping(path = \"/users\", method = POST)\n  void register() {}\n}\n";
    let out = extract_http_provides("C.java", src);
    assert_eq!(keys(&out), vec!["POST /users"]);
}

#[test]
fn a_bare_method_token_outside_the_request_method_enum_stays_ambiguous() {
    let src = "@RestController\nclass C {\n  @RequestMapping(path = \"/x\", method = CUSTOM)\n  void x() {}\n}\n";
    let out = extract_http_provides("C.java", src);
    assert!(
        out.is_empty(),
        "a non-RequestMethod bare token must stay ambiguous, got: {out:?}"
    );
}

#[test]
fn request_mapping_without_a_method_attribute_is_skipped_not_guessed() {
    let src = "@RestController\nclass C {\n  @RequestMapping(\"/x\")\n  void x() {}\n}\n";
    let out = extract_http_provides("C.java", src);
    assert!(
        out.is_empty(),
        "an ambiguous @RequestMapping (no method attribute) must never guess-emit a verb, got: {out:?}"
    );
}

#[test]
fn a_plain_class_with_the_controller_annotation_is_also_recognized() {
    let src = "@Controller\nclass C {\n  @GetMapping(\"/x\")\n  void x() {}\n}\n";
    let out = extract_http_provides("C.java", src);
    assert_eq!(keys(&out), vec!["GET /x"]);
}

#[test]
fn a_class_without_rest_controller_or_controller_emits_nothing() {
    let src = "class C {\n  @GetMapping(\"/x\")\n  void x() {}\n}\n";
    let out = extract_http_provides("C.java", src);
    assert!(
        out.is_empty(),
        "non-controller class must emit no provides, got: {out:?}"
    );
}

#[test]
fn a_method_with_no_mapping_annotation_at_all_emits_nothing() {
    let src = "@RestController\nclass C {\n  void helper() {}\n}\n";
    let out = extract_http_provides("C.java", src);
    assert!(out.is_empty());
}

#[test]
fn path_variables_are_normalized_by_http_interface_key() {
    let src = "@RestController\nclass C {\n  @GetMapping(\"/users/{id}\")\n  void x() {}\n}\n";
    let out = extract_http_provides("C.java", src);
    assert_eq!(keys(&out), vec!["GET /users/{}"]);
}

#[test]
fn empty_file_yields_no_provides() {
    assert!(extract_http_provides("E.java", "").is_empty());
}

#[test]
fn user_controller_shape_with_annotated_and_header_params_yields_get_and_put_user() {
    let src = concat!(
        "@RestController\n",
        "@RequestMapping(path = \"/user\")\n",
        "class UserController {\n\n",
        "    @GetMapping\n",
        "    User currentUser(@AuthenticationPrincipal User u, @RequestHeader(value = \"Authorization\") String h) {\n",
        "        return null;\n    }\n\n",
        "    @PutMapping\n",
        "    User updateUser(@AuthenticationPrincipal User u, @RequestHeader(value = \"Authorization\") String h) {\n",
        "        return null;\n    }\n}\n",
    );
    let out = extract_http_provides("UserController.java", src);
    let mut got = keys(&out);
    got.sort();
    assert_eq!(got, vec!["GET /user", "PUT /user"]);
}

#[test]
fn articles_api_shape_with_request_param_default_value_params_yields_all_three_routes() {
    let src = concat!(
        "@RestController\n",
        "@RequestMapping(path = \"/articles\")\n",
        "class ArticlesApi {\n\n",
        "    @PostMapping\n",
        "    Article create(@Valid @RequestBody ArticleCreateRequest req) {\n",
        "        return null;\n    }\n\n",
        "    @GetMapping(path = \"feed\")\n",
        "    List<Article> feed(@RequestParam(value = \"offset\", defaultValue = \"0\") int offset) {\n",
        "        return null;\n    }\n\n",
        "    @GetMapping\n",
        "    List<Article> list(@RequestParam(value = \"offset\", defaultValue = \"0\") int offset) {\n",
        "        return null;\n    }\n}\n",
    );
    let out = extract_http_provides("ArticlesApi.java", src);
    let mut got = keys(&out);
    got.sort();
    assert_eq!(
        got,
        vec!["GET /articles", "GET /articles/feed", "POST /articles"]
    );
}

#[test]
fn ctrl_authen_shape_yields_three_get_routes_under_the_authen_prefix() {
    let src = concat!(
        "package com.example.app.controllers;\n\n",
        "import org.springframework.web.bind.annotation.GetMapping;\n",
        "import org.springframework.web.bind.annotation.RequestMapping;\n",
        "import org.springframework.web.bind.annotation.RestController;\n\n",
        "@RequestMapping(\"/authen\")\n",
        "@RestController\n",
        "public class CtrlAuthen {\n\n",
        "    @GetMapping(\"/getGoogleRedirect\")\n",
        "    public String getGoogleRedirect() {\n        return \"\";\n    }\n\n",
        "    @GetMapping(\"/getUserInfo\")\n",
        "    public UserInfo getUserInfo() {\n        return null;\n    }\n\n",
        "    @GetMapping(\"/getSignout\")\n",
        "    public boolean getSignout() {\n        return true;\n    }\n}\n",
    );
    let out = extract_http_provides("CtrlAuthen.java", src);
    let mut got = keys(&out);
    got.sort();
    assert_eq!(
        got,
        vec![
            "GET /authen/getGoogleRedirect",
            "GET /authen/getSignout",
            "GET /authen/getUserInfo",
        ]
    );
    let user_info = out
        .iter()
        .find(|p| p.symbol.as_deref() == Some("getUserInfo"))
        .unwrap();
    assert_eq!(user_info.key, "GET /authen/getUserInfo");
}

// --- AST-grade precision gains beyond the old lexical crate (documented in `annotations`' module doc)

#[test]
fn a_deeply_nested_annotation_argument_no_longer_defeats_extraction() {
    // Two levels of paren nesting inside a mapping annotation's own args — the old lexical crate's
    // module doc calls this out as a still-unhandled limit; the real grammar has no such limit.
    let src = "@RestController\nclass C {\n  @GetMapping(value = \"/x\", headers = \"Accept=application/json\")\n  void x() {}\n}\n";
    let out = extract_http_provides("C.java", src);
    assert_eq!(keys(&out), vec!["GET /x"]);
}

#[test]
fn a_blank_line_between_annotation_and_declaration_no_longer_drops_the_annotation() {
    let src = "@RestController\n\nclass C {\n  @GetMapping(\"/x\")\n\n  void x() {}\n}\n";
    let out = extract_http_provides("C.java", src);
    assert_eq!(keys(&out), vec!["GET /x"]);
}

// --- BUG FIX: route path keyed on the NAMED value=/path= attribute, not the first quoted string.

#[test]
fn a_string_attribute_preceding_value_no_longer_wins_the_path_at_method_level() {
    // `produces` is a string-valued attribute that happens to appear BEFORE `value` — the old
    // first-quoted-string keying would wrongly take "application/json" as the path.
    let src = "@RestController\nclass C {\n  @GetMapping(produces = \"application/json\", value = \"/users\")\n  void x() {}\n}\n";
    let out = extract_http_provides("C.java", src);
    assert_eq!(keys(&out), vec!["GET /users"]);
}

#[test]
fn a_string_attribute_preceding_path_no_longer_wins_the_path_at_method_level() {
    let src = "@RestController\nclass C {\n  @PutMapping(headers = \"X-Api-Version=1\", path = \"/users\")\n  void x() {}\n}\n";
    let out = extract_http_provides("C.java", src);
    assert_eq!(keys(&out), vec!["PUT /users"]);
}

#[test]
fn a_string_attribute_preceding_path_no_longer_wins_the_class_level_prefix() {
    // Same mis-keying risk, but at the class-level @RequestMapping prefix (extract::walk_type).
    let src = "@RequestMapping(headers = \"X-Api-Version=1\", path = \"/api\")\n@RestController\nclass C {\n  @GetMapping(\"/x\")\n  void x() {}\n}\n";
    let out = extract_http_provides("C.java", src);
    assert_eq!(keys(&out), vec!["GET /api/x"]);
}

#[test]
fn the_positional_single_string_arg_form_still_works() {
    // No named value=/path= attribute at all — the path is genuinely the first (and only) quoted
    // string, so the positional fallback must still apply.
    let src = "@RestController\nclass C {\n  @GetMapping(\"/x\")\n  void x() {}\n}\n";
    let out = extract_http_provides("C.java", src);
    assert_eq!(keys(&out), vec!["GET /x"]);
}

// --- BUG FIX: multi-verb `method = {RequestMethod.GET, RequestMethod.POST}` emits one route per verb.

#[test]
fn request_mapping_with_multiple_verbs_emits_a_route_for_every_verb() {
    let src = "@RestController\nclass C {\n  @RequestMapping(method = {RequestMethod.GET, RequestMethod.POST}, value=\"/x\")\n  void x() {}\n}\n";
    let out = extract_http_provides("C.java", src);
    let mut got = keys(&out);
    got.sort();
    assert_eq!(got, vec!["GET /x", "POST /x"]);
}

#[test]
fn positional_array_brace_form_captures_the_first_path() {
    // `@GetMapping({"/a", "/b"})` — multi-path array; only the first path is captured (future work),
    // but crucially NOT an empty path (that would be a worse-than-partial provide key).
    let src = "@RestController\nclass C {\n  @GetMapping({\"/a\", \"/b\"})\n  void x() {}\n}\n";
    let out = extract_http_provides("C.java", src);
    assert_eq!(keys(&out), vec!["GET /a"]);
}

#[test]
fn named_value_array_brace_form_captures_the_first_path_as_the_class_prefix() {
    let src = "@RequestMapping(value = {\"/a\", \"/b\"})\n@RestController\nclass C {\n  @GetMapping(\"/x\")\n  void x() {}\n}\n";
    let out = extract_http_provides("C.java", src);
    assert_eq!(keys(&out), vec!["GET /a/x"]);
}

// --- BUG FIX: a NON-LITERAL method path is UNKNOWN (route dropped), not collapsed to the empty base.
// `route_path_state`'s literal / absent-base / non-literal tri-state — see `method_route`'s doc.

#[test]
fn a_non_literal_positional_method_path_drops_the_route_rather_than_keying_the_base() {
    // `@GetMapping(ApiPaths.USERS)` — the path is a constant reference this per-annotation pass cannot
    // resolve. The old `route_path_arg(..).unwrap_or_default()` keyed `GET /` (a phantom at the base AND a
    // loss of the real path); the tri-state now drops it entirely (honest under-report).
    let src = "@RestController\nclass C {\n  @GetMapping(ApiPaths.USERS)\n  void x() {}\n}\n";
    let out = extract_http_provides("C.java", src);
    assert!(keys(&out).is_empty(), "{:?}", keys(&out));
}

#[test]
fn a_non_literal_brace_array_method_path_drops_the_route() {
    // `@GetMapping({ApiPaths.USERS})` — the array/brace form of a non-literal path. `route_path_arg`'s
    // positional branch finds no quoted string inside the braces, so this must be classified NonLiteral
    // (dropped), NOT collapsed to the empty base like the literal `{"/a","/b"}` array form.
    let src = "@RestController\nclass C {\n  @GetMapping({ApiPaths.USERS})\n  void x() {}\n}\n";
    let out = extract_http_provides("C.java", src);
    assert!(keys(&out).is_empty(), "{:?}", keys(&out));
}

#[test]
fn a_non_literal_named_value_method_path_drops_the_route() {
    // `@RequestMapping(value = SOME_CONST, method = GET)` — a named but non-literal path, with a real verb.
    let src = "@RestController\nclass C {\n  @RequestMapping(value = ApiPaths.USERS, method = RequestMethod.GET)\n  void x() {}\n}\n";
    let out = extract_http_provides("C.java", src);
    assert!(keys(&out).is_empty(), "{:?}", keys(&out));
}

#[test]
fn a_non_path_attribute_only_method_annotation_is_still_the_base_route_not_dropped() {
    // `@PostMapping(produces = "json")` carries NO path-denoting argument — that is a legitimate
    // controller-prefix-only base route (`""`), NOT a non-literal path, so it must NOT be dropped.
    let src = "@RestController\n@RequestMapping(\"/api\")\nclass C {\n  @PostMapping(produces = \"application/json\")\n  void x() {}\n}\n";
    let out = extract_http_provides("C.java", src);
    assert_eq!(keys(&out), vec!["POST /api"]);
}

#[test]
fn a_non_literal_class_prefix_blocks_the_classes_own_routes() {
    // Per-file class-prefix parallel of the method-path fix (`extract::walk_type`'s `blocked`): a
    // non-literal `@RequestMapping(ApiPaths.BASE)` prefix can't be resolved here, so the class's direct
    // routes are blocked rather than keyed at the empty base under a wrong (missing) prefix.
    let src = "@RequestMapping(ApiPaths.BASE)\n@RestController\nclass C {\n  @GetMapping(\"/x\")\n  void x() {}\n}\n";
    let out = extract_http_provides("C.java", src);
    assert!(keys(&out).is_empty(), "{:?}", keys(&out));
}

#[test]
fn a_non_literal_class_prefix_does_not_block_an_independently_gated_nested_type() {
    // The class's OWN routes are blocked, but a nested @RestController gates independently on its own
    // (here literal) annotations — the block must not leak into it.
    let src = "@RequestMapping(ApiPaths.BASE)\n@RestController\nclass Outer {\n  @GetMapping(\"/x\")\n  void x() {}\n  @RequestMapping(\"/inner\")\n  @RestController\n  static class Inner {\n    @GetMapping(\"/y\")\n    void y() {}\n  }\n}\n";
    let out = extract_http_provides("C.java", src);
    assert_eq!(keys(&out), vec!["GET /inner/y"]);
}
