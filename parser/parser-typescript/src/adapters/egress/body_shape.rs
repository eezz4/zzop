//! Request-body shape extraction (`body-shape-v1`): how a matched call site's `args[1]` is read as a
//! request body, gated by [`BodyStyle`] and the body-position verb set — evidence-only, never guessed.

use swc_core::ecma::ast::{CallExpr, Callee, Expr, ExprOrSpread, MemberProp, Prop, PropOrSpread};
use zzop_core::ConsumeBodyShape;

use super::matchers::HttpCall;
use super::object_shape::{prop_name_str, shape_from_object_lit};
use super::unwrap_expr;

/// How the second call argument relates to a request body, at a matched HTTP call site
/// (`body-shape-v1`). See `witnessed_body_shape`'s doc for how each style is read.
#[derive(Clone, Copy)]
pub(super) enum BodyStyle {
    /// `args[1]` (if present) IS the body value directly — the axios/ky/Angular-HttpClient idiom.
    /// Only actually read when every resolved method is a body-position verb (`is_body_position_verb`).
    DirectArg,
    /// `args[1]` is an options object; the body lives at its own `body:` property — the bare
    /// `fetch`/`$fetch` idiom (mirrors how `method_from_options` reads `method:` from the same
    /// object).
    OptionsBodyProp,
}

/// The body-position HTTP verb set (`body-shape-v1`) — a `BodyStyle::DirectArg` call's `args[1]` is only
/// treated as a request body when EVERY resolved method is one of these (a `GET`/`DELETE`/`HEAD` verb's
/// second argument is a config/options object, never a body; a mixed-verb computed-member fanout with
/// even ONE non-body-position verb rejects the whole site rather than guessing). Uppercase, matching
/// `zzop_core::HTTP_KEY_VERBS`'s spelling; comparisons uppercase the candidate first since `HttpCall::methods`
/// mixes lowercase (axios/ky/Angular verb-name spellings) and uppercase (fetch/`$fetch`
/// `method_from_options` output) depending on which matcher produced it.
///
/// Do not unify with `zzop_rules_http`'s `WRITE_HTTP_METHODS` (policy inventory T3): that set is a
/// WRITE-SEMANTICS vocabulary and includes DELETE; this one is an axios/ky/Angular CALL-SIGNATURE
/// fact (`.delete(url, config)` puts a config object at `args[1]`, never a body), so the DELETE
/// divergence is the point.
const BODY_POSITION_VERBS: &[&str] = &["POST", "PUT", "PATCH"];

/// Whether `m` (any case) names a [`BODY_POSITION_VERBS`] member.
fn is_body_position_verb(m: &str) -> bool {
    BODY_POSITION_VERBS.contains(&m.to_ascii_uppercase().as_str())
}

/// Extract the statically witnessed request-body shape at a matched HTTP call site, or `None` when no
/// static object literal is visible at the expected position — evidence-only, never guessed (module doc
/// / `zzop_core::ConsumeBodyShape`'s doc). Reads `call.args.get(1)` differently depending on `hc.body_style`:
/// - [`BodyStyle::DirectArg`]: `args[1]` IS the body, gated to sites where EVERY resolved method is a
///   [`BODY_POSITION_VERBS`] member (a GET/DELETE/HEAD second argument is a config object, never a body;
///   a mixed-verb fanout with even one non-body-position verb never guesses which arm the object belongs
///   to, so the whole site yields `None`). A missing `args[1]` (`axios.post(url)`) also yields `None` —
///   an interceptor may inject a body invisibly; absence is not evidence either way.
/// - [`BodyStyle::OptionsBodyProp`]: `args[1]` is an options object; the body lives at that object's own
///   `body:` property, which may be a direct object literal, `JSON.stringify(<object literal>)`, or
///   anything else (`None`).
pub(super) fn witnessed_body_shape(call: &CallExpr, hc: &HttpCall<'_>) -> Option<ConsumeBodyShape> {
    match hc.body_style {
        BodyStyle::DirectArg => {
            if !hc.methods.iter().all(|m| is_body_position_verb(m)) {
                return None;
            }
            let arg = unwrap_expr(&call.args.get(1)?.expr);
            let Expr::Object(obj) = arg else {
                return None;
            };
            Some(shape_from_object_lit(obj))
        }
        BodyStyle::OptionsBodyProp => {
            let body_expr = unwrap_expr(body_prop_from_options(call.args.get(1))?);
            match body_expr {
                Expr::Object(obj) => Some(shape_from_object_lit(obj)),
                Expr::Call(c) if is_json_stringify_call(c) => {
                    let inner = unwrap_expr(&c.args.first()?.expr);
                    let Expr::Object(obj) = inner else {
                        return None;
                    };
                    Some(shape_from_object_lit(obj))
                }
                _ => None,
            }
        }
    }
}

/// Read the `body:` property's value expression from a fetch/axios options object. Only a literal
/// `body:` key (ident or string prop name) is read; `...opts` spreads are silently skipped (never
/// guessed).
fn body_prop_from_options(opts: Option<&ExprOrSpread>) -> Option<&Expr> {
    let expr = &*opts?.expr;
    let Expr::Object(obj) = expr else {
        return None;
    };
    for prop in &obj.props {
        if let PropOrSpread::Prop(p) = prop {
            if let Prop::KeyValue(kv) = &**p {
                if let Some(name) = prop_name_str(&kv.key) {
                    if name == "body" {
                        return Some(&kv.value);
                    }
                }
            }
        }
    }
    None
}

/// `JSON.stringify(<expr>)` — callee must be exactly the member `JSON.stringify` (receiver matched
/// tightly, never a bare-name allowlist).
fn is_json_stringify_call(call: &CallExpr) -> bool {
    let Callee::Expr(callee) = &call.callee else {
        return false;
    };
    let Expr::Member(m) = &**callee else {
        return false;
    };
    matches!(&*m.obj, Expr::Ident(id) if id.sym == "JSON")
        && matches!(&m.prop, MemberProp::Ident(name) if name.sym == "stringify")
}

#[cfg(test)]
mod tests {
    use super::BODY_POSITION_VERBS;
    use crate::adapters::egress::{extract_http_egress, files};

    // --- request-body shape extraction (`body-shape-v1`) ---

    #[test]
    fn body_position_verbs_is_pinned_to_the_exact_set() {
        // T2 pin (rule-quality policy value inventory): a drift here (an added/removed verb) must fail
        // this test loudly rather than silently changing which call sites get body evidence.
        assert_eq!(BODY_POSITION_VERBS, &["POST", "PUT", "PATCH"]);
    }

    #[test]
    fn axios_put_with_identifier_body_is_none() {
        let out = extract_http_egress(&files(&[("a.tsx", "axios.put('/users/1', payload);")]));
        assert_eq!(out.len(), 1);
        assert!(out[0].body.is_none());
    }

    #[test]
    fn axios_get_with_options_object_body_is_none() {
        // GET is not a body-position verb — `args[1]` here is a config object, not a body, regardless
        // of it being a static object literal.
        let out = extract_http_egress(&files(&[(
            "a.tsx",
            "axios.get('/users', { params: { page: 1 } });",
        )]));
        assert_eq!(out.len(), 1);
        assert!(out[0].body.is_none());
    }

    #[test]
    fn missing_second_argument_is_never_a_witnessed_body() {
        // Absence is not evidence either way (an interceptor may inject a body invisibly).
        let out = extract_http_egress(&files(&[("a.tsx", "axios.post('/users');")]));
        assert_eq!(out.len(), 1);
        assert!(out[0].body.is_none());
    }

    #[test]
    fn fetch_json_stringify_body_is_unwrapped() {
        let out = extract_http_egress(&files(&[(
            "a.tsx",
            r#"fetch('/users', { method: 'POST', body: JSON.stringify({ name, email }) });"#,
        )]));
        let body = out[0].body.as_ref().unwrap();
        assert_eq!(body.keys, vec!["email".to_string(), "name".to_string()]);
        assert_eq!(body.complete_at, vec!["".to_string()]);
    }

    #[test]
    fn fetch_body_identifier_is_none() {
        let out = extract_http_egress(&files(&[(
            "a.tsx",
            "fetch('/users', { method: 'POST', body: someVar });",
        )]));
        assert!(out[0].body.is_none());
    }

    #[test]
    fn angular_http_client_post_body_is_witnessed() {
        let out = extract_http_egress(&files(&[(
            "article.service.ts",
            concat!(
                "import { HttpClient } from '@angular/common/http';\n",
                "export class ArticleService {\n",
                "  constructor(private readonly http: HttpClient) {}\n",
                "  create() {\n",
                "    this.http.post('/articles', { title });\n",
                "  }\n",
                "}\n",
            ),
        )]));
        let body = out[0].body.as_ref().unwrap();
        assert_eq!(body.keys, vec!["title".to_string()]);
        assert_eq!(body.complete_at, vec!["".to_string()]);
    }

    #[test]
    fn computed_member_mixed_verb_fanout_body_is_none() {
        // Mixed-verb fanout (`DELETE`/`POST`) never guesses which arm the object belongs to — `None`
        // for BOTH emitted consumes, even though a static object literal is visibly present.
        let out = extract_http_egress(&files(&[(
            "a.ts",
            "axios[c ? 'delete' : 'post'](`/articles/${slug}/favorite`, { note });",
        )]));
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|c| c.body.is_none()));
    }
}
