//! Generated-client HTTP recognition (`generated-request-object-v1`) — the openapi/swagger codegen idiom
//! where the request URL and verb are OBJECT PROPERTIES of a request-descriptor argument rather than
//! positional arguments. Two generator families, both covered:
//! - swagger-typescript-api: `<recv>.request({ path: "/x", method: "POST", ... })` (descriptor = arg 0);
//! - openapi-typescript-codegen: `__request(OpenAPI, { url: "/x", method: "GET", ... })` (descriptor = arg 1).
//!
//! Sibling to [`super::matchers::match_http_call`]; see this module's own fn doc for the exact evidence gate.

use swc_core::ecma::ast::{CallExpr, Callee, Expr, Lit, MemberProp, Prop, PropName, PropOrSpread};

use super::body_shape::BodyStyle;
use super::matchers::HttpCall;
use super::unwrap_expr;

/// Request-descriptor-object matcher (`generated-request-object-v1`). The callee is either a
/// `<recv>.request(...)` member call (`<recv>` = `this` or a bare identifier — the generated `Api`/
/// `HttpClient` instance) or a `request`-named free-function call (`__request(config, opts)`; axios/ky/
/// fetch/`$fetch` are already claimed by [`super::matchers::match_http_call`] earlier in the chain). The
/// bare-ident callee is gated on a `request`-suffixed name ([`is_request_ident`]) precisely so it does NOT
/// swallow a server-side route-BUILDER DSL of the same argument shape (`createRoute`/`defineRoute`/`addRoute`
/// with `{ method, path }`), which is a route definition, not egress. SOME argument MUST then be an object
/// literal carrying BOTH a `method:` string literal naming a `zzop_core::HTTP_KEY_VERBS` verb AND a `path:`/
/// `url:` property — that pairing IS the evidence gate: a `.request(query)` with neither is left alone (a
/// GraphQL/RPC request never carries an HTTP verb literal + a path key). The descriptor's position varies by
/// generator, so it is SCANNED for rather than fixed to an index. The `path`/`url` value becomes
/// [`HttpCall::arg`], so the shared URL resolver reads a string literal or a `` `/articles/${slug}` ``
/// template identically to every other call shape. A computed/absent `method`/`path` yields `None`.
pub(super) fn match_generated_client_call(call: &CallExpr) -> Option<HttpCall<'_>> {
    let Callee::Expr(callee) = &call.callee else {
        return None;
    };
    match &**callee {
        Expr::Member(m) => {
            let MemberProp::Ident(name) = &m.prop else {
                return None;
            };
            if name.sym != "request" || !matches!(&*m.obj, Expr::This(_) | Expr::Ident(_)) {
                return None;
            }
        }
        Expr::Ident(id) if is_request_ident(&id.sym) => {}
        _ => return None,
    }
    let (method, path) = call
        .args
        .iter()
        .find_map(|a| descriptor_from_object(&a.expr))?;
    Some(HttpCall {
        methods: vec![method],
        arg: path,
        body_style: BodyStyle::SelfObjectBodyProp,
        client: "generated",
    })
}

/// A bare free-function callee name is treated as a generated request wrapper only when it is
/// `request`-suffixed (`request`, `__request`, `apiRequest`, …), case-insensitively. This keeps the
/// verb+path object gate from claiming a same-shaped server-side route-builder DSL (`createRoute`,
/// `defineRoute`, `addRoute`) whose `{ method, path }` describes a route to REGISTER, not one to CALL.
fn is_request_ident(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with("request")
}

/// Reads a request descriptor `(VERB, path-expr)` out of `arg` when it is an object literal carrying BOTH
/// a verb-literal `method:` AND a `path:`/`url:` property; `None` otherwise. The verb+path pairing is the
/// HTTP-specificity gate (see the matcher doc).
fn descriptor_from_object(arg: &Expr) -> Option<(String, &Expr)> {
    let Expr::Object(obj) = unwrap_expr(arg) else {
        return None;
    };
    let mut method: Option<String> = None;
    let mut path: Option<&Expr> = None;
    for prop in &obj.props {
        let PropOrSpread::Prop(p) = prop else {
            continue;
        };
        let Prop::KeyValue(kv) = &**p else { continue };
        let PropName::Ident(key) = &kv.key else {
            continue;
        };
        match &*key.sym {
            "method" => {
                if let Expr::Lit(Lit::Str(s)) = &*kv.value {
                    let v = s.value.as_str().unwrap_or_default().to_uppercase();
                    if zzop_core::HTTP_KEY_VERBS.contains(&v.as_str()) {
                        method = Some(v);
                    }
                }
            }
            // First `path`/`url` wins; a generated client declares exactly one.
            "path" | "url" => path = path.or(Some(&kv.value)),
            _ => {}
        }
    }
    Some((method?, path?))
}

#[cfg(test)]
mod tests {
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
}
