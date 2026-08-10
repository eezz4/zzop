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
                let method = match method_from_options(call.args.get(1)) {
                    OptionsMethod::SpecDefault => "GET".to_string(),
                    OptionsMethod::Literal(m) => m,
                    OptionsMethod::Unknowable => return None,
                };
                Some(HttpCall {
                    methods: vec![method],
                    arg,
                    body_style: BodyStyle::OptionsBodyProp,
                    client: if n == "fetch" { "fetch" } else { "$fetch" },
                })
            } else if n == "axios" {
                // Bare `axios(url, config)` reads its verb from `config.method` — the SAME
                // options-object position the `fetch` arm above reads, so it gets the same
                // three-way answer (axios's own default is GET when no config states a verb).
                // This arm used to hardcode GET and never look at `args[1]`, which mis-keyed
                // `axios(url, { method: 'POST' })` in BOTH directions: it invented a GET consume
                // no route provides and erased the POST consume the mutating-route rules read.
                let method = match method_from_options(call.args.get(1)) {
                    OptionsMethod::SpecDefault => "GET".to_string(),
                    OptionsMethod::Literal(m) => m,
                    OptionsMethod::Unknowable => return None,
                };
                Some(HttpCall {
                    methods: vec![method],
                    arg,
                    // NOT `DirectArg`: `args[1]` at this shape is the CONFIG object, never the body
                    // itself — now that the verb can resolve to a body-position one, `DirectArg`
                    // would witness the whole config as a request body. See `BodyStyle::NoWitness`.
                    body_style: BodyStyle::NoWitness,
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

/// What a `fetch(url, …)` second argument says about the verb. Three answers, not two — collapsing
/// [`OptionsMethod::Unknowable`] into the spec default was a measured defect: `fetch(url, opts)` and
/// `fetch(url, { method: verb })` were keyed as `GET`, which mis-keys the call in BOTH directions —
/// it invents a GET consume no route provides (false `unprovidedConsumes`) and erases the real verb's
/// consume, so the mutating-route rules stop seeing it. The sibling `wrapper_calls` channel already
/// refuses these two shapes by name (`fetch_has_opaque_options`, `mentions_method_key`); this type is
/// the same judgment on the egress side.
enum OptionsMethod {
    /// No options argument, or an inline object stating no `method` — `fetch` IS a GET here, by spec.
    SpecDefault,
    /// A visible string literal verb.
    Literal(String),
    /// The source says a verb may exist and does not show its value. Never guessed; the call site is
    /// dropped instead. Under-report, not mis-key.
    Unknowable,
}

fn method_from_options(opts: Option<&ExprOrSpread>) -> OptionsMethod {
    let Some(opts) = opts else {
        return OptionsMethod::SpecDefault;
    };
    // A spread argument hides everything it carries.
    if opts.spread.is_some() {
        return OptionsMethod::Unknowable;
    }
    let Expr::Object(obj) = &*opts.expr else {
        // An identifier, call, conditional — anything not written out here. The verb may be inside.
        return OptionsMethod::Unknowable;
    };
    for prop in &obj.props {
        match prop {
            // `{ ...cfg }` can carry `method` too.
            PropOrSpread::Spread(_) => return OptionsMethod::Unknowable,
            PropOrSpread::Prop(p) => {
                let Prop::KeyValue(kv) = &**p else {
                    // Shorthand `{ method }` names the key while hiding the value.
                    if let Prop::Shorthand(id) = &**p {
                        if id.sym == "method" {
                            return OptionsMethod::Unknowable;
                        }
                    }
                    continue;
                };
                let names_method = match &kv.key {
                    PropName::Ident(name) => name.sym == "method",
                    PropName::Str(s) => s.value.as_str().unwrap_or_default() == "method",
                    // A computed key can spell `method` (`{ ["method"]: verb }` does so literally,
                    // and `{ [k]: verb }` can at runtime) — whether it does is not decidable here,
                    // so the whole site is unknowable, never consumed as a spec-default GET.
                    PropName::Computed(_) => return OptionsMethod::Unknowable,
                    _ => false,
                };
                if !names_method {
                    continue;
                }
                return match &*kv.value {
                    Expr::Lit(Lit::Str(s)) => {
                        OptionsMethod::Literal(s.value.as_str().unwrap_or_default().to_uppercase())
                    }
                    // `method: verb` — stated to exist, value not visible.
                    _ => OptionsMethod::Unknowable,
                };
            }
        }
    }
    OptionsMethod::SpecDefault
}
