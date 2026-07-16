//! HTTP call-shape recognizers: which call sites count as egress at all, and which client family
//! (`axios`/`ky`/`fetch`/`$fetch`) matched — see the module doc on [`super`] for the recognized shapes.

use swc_core::ecma::ast::{
    CallExpr, Callee, Expr, ExprOrSpread, Lit, MemberProp, Prop, PropName, PropOrSpread,
};

use super::body_shape::BodyStyle;

pub(super) struct HttpCall<'a> {
    /// One method (the common case) or two, cons-arm first then alt-arm, when the callee was a
    /// computed member with a two-literal ternary bracket expression (`cond-literal-fanout-v1`).
    pub(super) methods: Vec<String>,
    pub(super) arg: &'a Expr,
    /// How `call.args.get(1)` maps to a request body, for `witnessed_body_shape` — set per matched
    /// call shape (`body-shape-v1`).
    pub(super) body_style: BodyStyle,
    /// Which client recognizer matched this call site (`axios-defaults-base-v1`) — carried onto every
    /// `IoConsume` this call site emits as `IoConsume::client`, so a client-scoped normalization seam
    /// (e.g. `axios.defaults.baseURL`) can tell an axios consume from a fetch/ky one in the same tree.
    pub(super) client: &'static str,
}

pub(super) fn match_http_call(call: &CallExpr) -> Option<HttpCall<'_>> {
    let arg = &*call.args.first()?.expr;
    let Callee::Expr(callee) = &call.callee else {
        return None;
    };
    match &**callee {
        Expr::Member(m) => {
            let Expr::Ident(obj) = &*m.obj else {
                return None;
            };
            let obj = obj.sym.to_string();
            match &m.prop {
                MemberProp::Ident(name) => {
                    let name = name.sym.to_string();
                    if (obj == "axios" || obj == "ky") && is_http_method(&name) {
                        Some(HttpCall {
                            methods: vec![name],
                            arg,
                            body_style: BodyStyle::DirectArg,
                            client: if obj == "axios" { "axios" } else { "ky" },
                        })
                    } else {
                        None
                    }
                }
                // Computed member callee — `axios['post'](url)` / `axios[cond ? 'delete' : 'post'](url)`.
                // Only `axios`/`ky`.
                MemberProp::Computed(c) => {
                    if obj != "axios" && obj != "ky" {
                        return None;
                    }
                    let methods = methods_from_computed_prop(&c.expr)?;
                    Some(HttpCall {
                        methods,
                        arg,
                        body_style: BodyStyle::DirectArg,
                        client: if obj == "axios" { "axios" } else { "ky" },
                    })
                }
                MemberProp::PrivateName(_) => None,
            }
        }
        Expr::Ident(id) => {
            let n = id.sym.to_string();
            if n == "fetch" || n == "$fetch" {
                let method = method_from_options(call.args.get(1)).unwrap_or_else(|| "GET".into());
                Some(HttpCall {
                    methods: vec![method],
                    arg,
                    body_style: BodyStyle::OptionsBodyProp,
                    client: if n == "fetch" { "fetch" } else { "$fetch" },
                })
            } else if n == "axios" {
                Some(HttpCall {
                    methods: vec!["GET".into()],
                    arg,
                    body_style: BodyStyle::DirectArg,
                    client: "axios",
                })
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Whether `m` is a lowercase spelling of a `zzop_core::HTTP_KEY_VERBS` verb — the member-callee /
/// computed-member vocabulary (T1: the verb SET lives in core; the exact-lowercase comparison is this
/// vocabulary's own spelling rule, so `axios.GET(...)` — not a real client API — stays unrecognized).
pub(super) fn is_http_method(m: &str) -> bool {
    zzop_core::HTTP_KEY_VERBS
        .iter()
        .any(|v| v.to_ascii_lowercase() == m)
}

/// Resolve a computed member-access bracket expression (`axios[<expr>](url)`) to one or two HTTP
/// methods, or `None` if not a recognized shape — never guessed. A bare string literal in the verb set
/// is one method; a ternary whose cons AND alt are BOTH string literals AND BOTH in the verb set is two
/// methods, cons first (`cond-literal-fanout-v1`). An identifier, a literal outside the verb set on
/// either arm, or any other shape (including a one-literal-one-dynamic ternary) rejects the whole call
/// site, matching the "never guess" convention: no method is invented, and no half-known site is
/// silently narrowed to just its literal arm.
fn methods_from_computed_prop(expr: &Expr) -> Option<Vec<String>> {
    match expr {
        Expr::Lit(Lit::Str(s)) => {
            let v = s.value.as_str().unwrap_or_default();
            is_http_method(v).then(|| vec![v.to_string()])
        }
        Expr::Cond(c) => {
            let Expr::Lit(Lit::Str(cons)) = &*c.cons else {
                return None;
            };
            let Expr::Lit(Lit::Str(alt)) = &*c.alt else {
                return None;
            };
            let cons_v = cons.value.as_str().unwrap_or_default();
            let alt_v = alt.value.as_str().unwrap_or_default();
            if is_http_method(cons_v) && is_http_method(alt_v) {
                Some(vec![cons_v.to_string(), alt_v.to_string()])
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Read `{ method: "post" }` from a fetch/axios options object. Only a literal `method: "..."` key is
/// read; `...opts` spreads are silently skipped.
fn method_from_options(opts: Option<&ExprOrSpread>) -> Option<String> {
    let expr = &*opts?.expr;
    let Expr::Object(obj) = expr else {
        return None;
    };
    for prop in &obj.props {
        if let PropOrSpread::Prop(p) = prop {
            if let Prop::KeyValue(kv) = &**p {
                if let (PropName::Ident(name), Expr::Lit(Lit::Str(s))) = (&kv.key, &*kv.value) {
                    if name.sym == "method" {
                        return Some(s.value.as_str().unwrap_or_default().to_uppercase());
                    }
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use crate::adapters::egress::{clients, extract_http_egress, files, keys};

    #[test]
    fn computed_member_ternary_callee_fans_out_the_method() {
        let out = extract_http_egress(&files(&[(
            "conduit.ts",
            "axios[favorited ? 'delete' : 'post'](`/articles/${slug}/favorite`);",
        )]));
        assert_eq!(
            keys(&out),
            vec![
                Some("DELETE /articles/{}/favorite".to_string()),
                Some("POST /articles/{}/favorite".to_string()),
            ]
        );
    }

    #[test]
    fn computed_member_string_literal_callee_is_a_single_method() {
        let out = extract_http_egress(&files(&[("a.ts", "axios['post']('/a');")]));
        assert_eq!(keys(&out), vec![Some("POST /a".to_string())]);
    }

    #[test]
    fn computed_member_identifier_callee_is_not_recognized() {
        let out = extract_http_egress(&files(&[("a.ts", "axios[verb]('/a');")]));
        assert!(out.is_empty());
    }

    #[test]
    fn computed_member_ternary_with_an_arm_outside_the_verb_set_rejects_the_whole_site() {
        // `head` is not a recognized verb — one bad arm rejects the site entirely rather than silently
        // narrowing to just the `get` arm (never guess).
        let out = extract_http_egress(&files(&[("a.ts", "axios[cond ? 'get' : 'head']('/a');")]));
        assert!(out.is_empty());
    }

    #[test]
    fn computed_member_ternary_callee_with_unresolved_url_carries_both_methods() {
        let out = extract_http_egress(&files(&[(
            "a.ts",
            "axios[cond ? 'delete' : 'post'](buildUrl(x));",
        )]));
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|c| c.key.is_none()));
        assert!(out.iter().all(|c| c.raw.as_deref() == Some("buildUrl(x)")));
        assert_eq!(
            out.iter().map(|c| c.method.clone()).collect::<Vec<_>>(),
            vec![Some("DELETE".to_string()), Some("POST".to_string())]
        );
    }

    // --- client provenance tag (`axios-defaults-base-v1`) ---

    #[test]
    fn axios_member_call_is_tagged_axios() {
        let out = extract_http_egress(&files(&[("a.ts", r#"axios.get("/a");"#)]));
        assert_eq!(clients(&out), vec![Some("axios".to_string())]);
    }

    #[test]
    fn bare_axios_call_is_tagged_axios() {
        let out = extract_http_egress(&files(&[("a.ts", r#"axios("/a");"#)]));
        assert_eq!(clients(&out), vec![Some("axios".to_string())]);
    }

    #[test]
    fn axios_computed_member_call_is_tagged_axios() {
        let out = extract_http_egress(&files(&[("a.ts", "axios['post']('/a');")]));
        assert_eq!(clients(&out), vec![Some("axios".to_string())]);
    }

    #[test]
    fn axios_computed_member_fanout_is_tagged_axios_for_every_variant() {
        let out = extract_http_egress(&files(&[(
            "a.ts",
            "axios[favorited ? 'delete' : 'post'](`/articles/${slug}/favorite`);",
        )]));
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|c| c.client.as_deref() == Some("axios")));
    }

    #[test]
    fn ky_member_call_is_tagged_ky() {
        let out = extract_http_egress(&files(&[("a.ts", r#"ky.get("/a");"#)]));
        assert_eq!(clients(&out), vec![Some("ky".to_string())]);
    }

    #[test]
    fn bare_fetch_call_is_tagged_fetch() {
        let out = extract_http_egress(&files(&[("a.ts", r#"fetch("/a");"#)]));
        assert_eq!(clients(&out), vec![Some("fetch".to_string())]);
    }

    #[test]
    fn dollar_fetch_call_is_tagged_dollar_fetch() {
        let out = extract_http_egress(&files(&[("a.ts", r#"$fetch("/a");"#)]));
        assert_eq!(clients(&out), vec![Some("$fetch".to_string())]);
    }

    #[test]
    fn unresolved_consume_still_carries_its_client_tag() {
        // Dynamic URL (unresolved) — `client` is still set from the matcher, independent of whether the
        // URL itself resolved to a key.
        let out = extract_http_egress(&files(&[("a.ts", "axios.get(buildUrl(x));")]));
        assert_eq!(out.len(), 1);
        assert!(out[0].key.is_none());
        assert_eq!(out[0].client.as_deref(), Some("axios"));
    }
}
