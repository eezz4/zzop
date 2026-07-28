//! Generated-client HTTP recognition (`generated-request-object-v1`) — the openapi/swagger codegen idiom
//! where the request URL and verb are OBJECT PROPERTIES of a request-descriptor argument rather than
//! positional arguments. Two generator families, both covered:
//! - swagger-typescript-api: `<recv>.request({ path: "/x", method: "POST", ... })` (descriptor = arg 0);
//! - openapi-typescript-codegen: `__request(OpenAPI, { url: "/x", method: "GET", ... })` (descriptor = arg 1);
//! - `@hey-api/openapi-ts` (`generated-verb-member-v1`): `<recv>.get({ url: "/x", ... })` — the verb is the
//!   METHOD NAME, so there is no `method:` property to pair with, and `<recv>` is routinely an expression
//!   (`(options?.client ?? client)`) rather than a name. See [`verb_named_prop`] / [`url_descriptor`] for
//!   why this arm's evidence gate is `url:`-only.
//!
//! Which module the client is IMPORTED from is irrelevant to all three — the generated client's own source
//! is in the tree, so its calls are read at their own sites. A relative-path import (`./client`) is
//! therefore not a blind spot by itself; what blinds the join is a call shape none of the three arms
//! recognize.
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
            if let Some(verb) = verb_named_prop(&name.sym) {
                // `generated-verb-member-v1`, the second generator family shape — see below.
                let path = call.args.iter().find_map(|a| url_descriptor(&a.expr))?;
                return Some(HttpCall {
                    methods: vec![verb],
                    arg: path,
                    body_style: BodyStyle::SelfObjectBodyProp,
                    client: "generated",
                });
            }
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

/// The upper-cased verb a member-call property names, when it names one — the `generated-verb-member-v1`
/// arm's first half. `@hey-api/openapi-ts` (what `full-stack-fastapi-template` ships) writes the verb as
/// the METHOD NAME and the URL as a descriptor property:
/// `(options?.client ?? client).get({ url: '/api/v1/items', ...options })`. Neither existing arm sees it —
/// the `request`-named arm wants the prop spelled `request`, and
/// [`super::matchers::match_http_call`]'s member arm only accepts an `axios`/`ky` receiver, while this
/// receiver is a `??` expression. Measured before the arm existed: a tree whose whole API surface flows
/// through such a client extracted ZERO consumes and produced no tripwire warning at all.
fn verb_named_prop(name: &str) -> Option<String> {
    let upper = name.to_ascii_uppercase();
    zzop_core::HTTP_KEY_VERBS
        .contains(&upper.as_str())
        .then_some(upper)
}

/// The `url:` value of an object-literal argument — the `generated-verb-member-v1` arm's evidence gate,
/// and the whole of it, since the verb already came from the method name.
///
/// `url:` ONLY, never `path:`, unlike [`descriptor_from_object`]. That arm can afford `path` because it
/// additionally requires a `method:` verb LITERAL, which a route-builder DSL's `{ method, path }` also has
/// but whose bare callee it rejects by name. Here the callee is a verb-named member, which a server-side
/// router (`router.get({ path, handler })`) matches exactly — so the discriminator has to be the key
/// spelling, and `url` is what the generators write. Accepted residual: a non-HTTP `.get({ url })` on some
/// unrelated receiver would be claimed; a call passing a `url` to a verb-named method is HTTP in every
/// shape measured, and the alternative (a receiver-name vocabulary) is the guess this module avoids.
fn url_descriptor(arg: &Expr) -> Option<&Expr> {
    let Expr::Object(obj) = unwrap_expr(arg) else {
        return None;
    };
    obj.props.iter().find_map(|prop| {
        let PropOrSpread::Prop(p) = prop else {
            return None;
        };
        let Prop::KeyValue(kv) = &**p else {
            return None;
        };
        let PropName::Ident(key) = &kv.key else {
            return None;
        };
        (key.sym == "url").then_some(&*kv.value)
    })
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
mod tests;
