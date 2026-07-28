//! `match_generated_client_call` coverage — the three generator families in the module doc, plus the
//! negatives that keep a server-side route-builder DSL out of egress.
use crate::adapters::egress::{clients, extract_http_egress, files, keys};

#[test]
fn generated_request_object_static_path_is_recognized() {
    let out = extract_http_egress(&files(&[(
        "api.ts",
        "class Api { login() { return this.request({ path: `/users/login`, method: \"POST\", body: data }); } }",
    )]));
    assert_eq!(keys(&out), vec![Some("POST /users/login".to_string())]);
    assert_eq!(clients(&out), vec![Some("generated".to_string())]);
}

#[test]
fn generated_request_object_template_path_normalizes() {
    let out = extract_http_egress(&files(&[(
        "api.ts",
        "class Api { del(slug) { return this.request({ path: `/articles/${slug}`, method: \"DELETE\" }); } }",
    )]));
    assert_eq!(keys(&out), vec![Some("DELETE /articles/{}".to_string())]);
}

#[test]
fn generated_request_object_url_key_alias_is_recognized() {
    // openapi-generator variants key the path as `url` rather than `path`.
    let out = extract_http_egress(&files(&[(
        "api.ts",
        "const api = { go() { return http.request({ url: \"/tags\", method: \"GET\" }); } };",
    )]));
    assert_eq!(keys(&out), vec![Some("GET /tags".to_string())]);
}

#[test]
fn openapi_codegen_free_function_request_with_descriptor_as_second_arg() {
    // openapi-typescript-codegen: `__request(OpenAPI, { method, url })` — free-function callee, the
    // request descriptor is the SECOND argument.
    let out = extract_http_egress(&files(&[(
        "sdk.gen.ts",
        "import { request as __request } from './core/request';\nexport const readItems = () => __request(OpenAPI, { method: \"GET\", url: `/api/v1/items` });",
    )]));
    assert_eq!(keys(&out), vec![Some("GET /api/v1/items".to_string())]);
    assert_eq!(clients(&out), vec![Some("generated".to_string())]);
}

#[test]
fn free_function_call_without_a_descriptor_object_is_ignored() {
    // A bare free-function call with no `{method, url}` object arg is not an HTTP call — left alone.
    let out = extract_http_egress(&files(&[(
        "a.ts",
        "function f() { return compute(a, b); }",
    )]));
    assert!(out.is_empty());
}

#[test]
fn server_side_route_builder_dsl_with_the_same_shape_is_not_egress() {
    // `createRoute({ method, path })` (hono zod-openapi and friends) has the exact verb+path object
    // shape, but a bare non-`request` callee names a route DEFINITION to register, not a client call.
    let out = extract_http_egress(&files(&[(
        "routes.ts",
        "export const r = createRoute({ method: \"get\", path: \"/users\", responses: {} });",
    )]));
    assert!(out.is_empty(), "{:?}", out);
}

#[test]
fn request_object_without_a_method_verb_is_not_http() {
    // A GraphQL/RPC `.request({ query })` has no HTTP verb literal + path pairing — left alone.
    let out = extract_http_egress(&files(&[(
        "a.ts",
        "class C { run() { return this.request({ query: `{ me }`, variables: v }); } }",
    )]));
    assert!(out.is_empty());
}

#[test]
fn request_object_with_non_verb_method_string_is_rejected() {
    let out = extract_http_egress(&files(&[(
        "a.ts",
        "class C { run() { return this.request({ path: \"/x\", method: \"SUBSCRIBE\" }); } }",
    )]));
    assert!(out.is_empty());
}

#[test]
fn generated_request_post_body_object_literal_is_witnessed() {
    let out = extract_http_egress(&files(&[(
        "api.ts",
        "class Api { create() { return this.request({ path: \"/articles\", method: \"POST\", body: { title } }); } }",
    )]));
    let body = out[0].body.as_ref().unwrap();
    assert_eq!(body.keys, vec!["title".to_string()]);
}

// --- verb-named member call with a `url:` descriptor (`generated-verb-member-v1`) ---

#[test]
fn hey_api_verb_named_member_call_with_a_url_descriptor_is_recognized() {
    // `@hey-api/openapi-ts` sdk.gen.ts, verbatim shape: the verb is the METHOD NAME and the
    // receiver is a `??` expression, so neither the `request`-named arm nor `match_http_call`
    // (axios/ky receivers only) saw it. Measured: this left the whole tree with ZERO consumes.
    let out = extract_http_egress(&files(&[(
        "src/client/sdk.gen.ts",
        "export const readItems = (options) => (options?.client ?? client).get({ url: '/api/v1/items', ...options });",
    )]));
    assert_eq!(keys(&out), vec![Some("GET /api/v1/items".to_string())]);
    assert_eq!(clients(&out), vec![Some("generated".to_string())]);
}

#[test]
fn hey_api_write_verb_and_template_url_normalize() {
    let out = extract_http_egress(&files(&[(
        "sdk.gen.ts",
        "export const del = (o) => client.delete({ url: `/api/v1/items/${o.id}` });\nexport const add = (o) => client.post({ url: '/api/v1/items', body: o.body });",
    )]));
    assert_eq!(
        keys(&out),
        vec![
            Some("DELETE /api/v1/items/{}".to_string()),
            Some("POST /api/v1/items".to_string())
        ]
    );
}

#[test]
fn a_verb_named_member_call_with_a_path_key_is_not_claimed() {
    // Deliberately `url:`-only on this arm: `{ method, path }` is the route-BUILDER vocabulary the
    // `request`-named arm already guards against, and a verb-named `.get({ path })` carries no
    // second piece of evidence to tell a route definition from a call.
    let out = extract_http_egress(&files(&[(
        "routes.ts",
        "export const r = router.get({ path: '/users', handler });",
    )]));
    assert!(out.is_empty(), "{out:?}");
}

#[test]
fn a_verb_named_member_call_without_an_object_descriptor_is_not_claimed() {
    let out = extract_http_egress(&files(&[(
        "routes.ts",
        "app.get('/users', handler); cache.get(key);",
    )]));
    assert!(out.is_empty(), "{out:?}");
}

#[test]
fn a_non_verb_member_call_with_a_url_descriptor_is_not_claimed() {
    let out = extract_http_egress(&files(&[(
        "a.ts",
        "export const q = loader.load({ url: '/x' });",
    )]));
    assert!(out.is_empty(), "{out:?}");
}
