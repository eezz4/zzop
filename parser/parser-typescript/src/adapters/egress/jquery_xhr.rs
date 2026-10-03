//! jQuery and `XMLHttpRequest` egress shapes — the pre-module-era client vocabulary.
//!
//! ## Why these, and why now
//! Every client [`super::matchers::match_http_call`] recognizes arrives as a MODULE: `axios`/`ky` are
//! imported, `fetch`/`$fetch` are globals a bundler-era file calls directly. A jQuery-era frontend has
//! neither — jQuery enters through a `<script>` tag, so there is no import to anchor on, and its call
//! idioms are not the ones that matcher spells. Measured 2026-10-03 on `corpus/legacy/web`, a
//! hand-built jQuery-era tree: 22 real HTTP call sites, ONE extracted — and that one was the tree's
//! single builtin `fetch(`, the only shape this build already knew.
//!
//! The loss is worse than it looks, which is why it stayed invisible: an unrecognized call shape does
//! not become an unresolved consume, it produces no fact at all, so none of the fields that count
//! what a run could not resolve can see it. `framework_silence`'s S19 now reports that denominator;
//! this module is the other half — the extraction that makes the denominator shrink.
//!
//! ## The three shapes, and why they could not live in `matchers.rs`
//! That module's entry binds the URL to `args[0]` once, for every arm. Two of these three carry the
//! URL somewhere else, so they get their own matcher rather than bending that invariant:
//!
//! | Shape | URL | Verb |
//! |---|---|---|
//! | `$.get` / `$.getJSON` / `$.post` | `args[0]` | the method name itself |
//! | `$.ajax({ url, type })` / `$.ajax(url, settings)` | a `url:` property, or `args[0]` | `method:` or `type:` |
//! | `xhr.open('POST', url)` | **`args[1]`** | **`args[0]`** |
//!
//! `type:` is jQuery's pre-1.9 spelling of `method:`; both are read, and `method:` wins when a
//! settings object carries both, which is jQuery's own precedence.
//!
//! ## Under-report, never mis-key
//! The convention this file inherits from its sibling: a shape that STATES a verb exists without
//! showing its value drops the whole call site rather than defaulting it. A settings object that is an
//! identifier, carries a spread, uses a computed key, or gives `method`/`type` a non-literal value is
//! [`Verb::Unknowable`] — mis-keying invents a consume no route provides AND erases the real one, so it
//! is strictly worse than silence. An ABSENT verb is different and is not a guess: jQuery's documented
//! default for `$.ajax` is GET, the same standing the `fetch` arm's spec-default GET has.
//!
//! Bodies are deliberately not witnessed for `$.ajax` or `xhr.open`: jQuery carries its payload at
//! `settings.data` and XHR passes it to a LATER `.send(body)` call, neither of which any reader here
//! extracts. `$.post(url, data)` does put the payload at `args[1]`, the [`BodyStyle::DirectArg`]
//! position, and the body-shape layer only reads it for body-position verbs, so that arm says so.

use swc_core::ecma::ast::{CallExpr, Callee, Expr, Lit, MemberProp, Prop, PropName, PropOrSpread};

use super::body_shape::BodyStyle;
use super::matchers::{is_http_method, HttpCall};

/// The two global names jQuery installs. Exact-match, like the `axios`/`ky` receiver test it mirrors:
/// a local named `jquery` is not jQuery, and `$` rebound to something else is a risk this vocabulary
/// accepts for the same reason the sibling accepts a local named `axios` — the verb and shape checks
/// below carry the rest of the evidence.
fn is_jquery_receiver(obj: &str) -> bool {
    obj == "$" || obj == "jQuery"
}

/// What a settings object says about the verb. Three answers for the same reason
/// `matchers::OptionsMethod` has three — see this module's doc.
enum Verb {
    /// No settings object, or one stating no `method`/`type`. `$.ajax`'s documented default is GET.
    Default,
    Literal(String),
    /// A verb may exist and its value is not visible. The call site is dropped.
    Unknowable,
}

pub(super) fn match_jquery_or_xhr_call(call: &CallExpr) -> Option<HttpCall<'_>> {
    let Callee::Expr(callee) = &call.callee else {
        return None;
    };
    let Expr::Member(m) = &**callee else {
        return None;
    };
    let Expr::Ident(obj) = &*m.obj else {
        return None;
    };
    let MemberProp::Ident(prop) = &m.prop else {
        return None;
    };

    if is_jquery_receiver(obj.sym.as_str()) {
        return match prop.sym.as_str() {
            // `$.getJSON` is `$.get` with a dataType — same verb, same argument position.
            "get" | "getJSON" => positional_jquery(call, "GET"),
            "post" => positional_jquery(call, "POST"),
            "ajax" => jquery_ajax(call),
            _ => None,
        };
    }
    if prop.sym.as_str() == "open" {
        return xhr_open(call);
    }
    None
}

/// `$.get(url, …)` / `$.getJSON(url, …)` / `$.post(url, data)` — the URL sits where every other
/// recognized client puts it, so these need nothing but a verb and a receiver check.
fn positional_jquery<'a>(call: &'a CallExpr, method: &str) -> Option<HttpCall<'a>> {
    let first = call.args.first()?;
    if first.spread.is_some() {
        return None;
    }
    Some(HttpCall {
        methods: vec![method.to_string()],
        arg: &first.expr,
        body_style: BodyStyle::DirectArg,
        client: "jquery",
    })
}

/// `$.ajax(settings)` and `$.ajax(url, settings)` — jQuery accepts both. The first form hides the URL
/// at a `url:` property; the second is positional with the settings following.
fn jquery_ajax(call: &CallExpr) -> Option<HttpCall<'_>> {
    let first = call.args.first()?;
    if first.spread.is_some() {
        return None;
    }
    let (url, settings) = match (&*first.expr, call.args.get(1)) {
        // `$.ajax({ … })` — the settings form. The URL is a property of that object.
        (Expr::Object(_), _) => (url_from_settings(&first.expr)?, Some(&*first.expr)),
        // A spread in the settings position hides the verb.
        (_, Some(second)) if second.spread.is_some() => return None,
        // `$.ajax(url, settings)` — two arguments, so the first is unambiguously the URL whatever
        // shape it has; `resolve_url_variants` does the rest.
        (_, Some(second)) => (&*first.expr, Some(&*second.expr)),
        // ONE argument that is not an object literal. jQuery's one-argument overload is
        // `$.ajax(settings)`, so `$.ajax(cfg)` is the SETTINGS form with a variable, NOT a URL —
        // reading that identifier as a path invents a consume out of a config object. Only a
        // written-out string, template or concatenation is unambiguous enough to be the url form,
        // which `$.ajax('/a')` is. 📏 Caught by this file's own test: the first draft keyed
        // `$.ajax(settings)` as a consume on the identifier `settings`.
        (Expr::Lit(Lit::Str(_)) | Expr::Tpl(_) | Expr::Bin(_), None) => (&*first.expr, None),
        _ => return None,
    };
    let method = match verb_from_settings(settings) {
        Verb::Default => "GET".to_string(),
        Verb::Literal(v) => v,
        Verb::Unknowable => return None,
    };
    Some(HttpCall {
        methods: vec![method],
        arg: url,
        // jQuery's payload is `settings.data`, which no reader here extracts — see the module doc.
        body_style: BodyStyle::NoWitness,
        client: "jquery",
    })
}

/// `xhr.open(verb, url)` — the one shape whose verb AND url are both positional and neither is where
/// the sibling matcher looks.
///
/// The receiver is any plain identifier rather than a tracked `new XMLHttpRequest()` binding: the
/// evidence that carries this arm is the FIRST ARGUMENT being a literal HTTP verb, which `window.open`
/// (a URL first) and the other common `.open(` idioms do not satisfy. A receiver that is not a bare
/// identifier (`this.xhr.open(…)`) is not matched — under-report, consistent with the rest of this file.
fn xhr_open(call: &CallExpr) -> Option<HttpCall<'_>> {
    let verb_arg = call.args.first()?;
    if verb_arg.spread.is_some() {
        return None;
    }
    let Expr::Lit(Lit::Str(s)) = &*verb_arg.expr else {
        return None;
    };
    let verb = s.value.as_str().unwrap_or_default();
    if !is_http_method(&verb.to_ascii_lowercase()) {
        return None;
    }
    let url = call.args.get(1)?;
    if url.spread.is_some() {
        return None;
    }
    Some(HttpCall {
        methods: vec![verb.to_uppercase()],
        arg: &url.expr,
        // XHR's body goes to a later `.send(body)`, not to this call.
        body_style: BodyStyle::NoWitness,
        client: "xhr",
    })
}

/// The `url:` property of a settings object, or `None` when it is absent or not written out here.
fn url_from_settings(settings: &Expr) -> Option<&Expr> {
    let Expr::Object(obj) = settings else {
        return None;
    };
    for prop in &obj.props {
        let PropOrSpread::Prop(p) = prop else {
            continue;
        };
        let Prop::KeyValue(kv) = &**p else {
            continue;
        };
        let names_url = match &kv.key {
            PropName::Ident(name) => name.sym == "url",
            PropName::Str(s) => s.value.as_str().unwrap_or_default() == "url",
            _ => false,
        };
        if names_url {
            return Some(&kv.value);
        }
    }
    None
}

/// Reads `method:` / `type:` off a settings object. `method:` wins when both appear, which is jQuery's
/// own precedence; a verb outside the known set is [`Verb::Unknowable`] rather than passed through, so
/// a typo cannot mint a channel nobody provides.
fn verb_from_settings(settings: Option<&Expr>) -> Verb {
    let Some(settings) = settings else {
        return Verb::Default;
    };
    let Expr::Object(obj) = settings else {
        return Verb::Unknowable;
    };
    let mut from_type: Option<String> = None;
    for prop in &obj.props {
        match prop {
            PropOrSpread::Spread(_) => return Verb::Unknowable,
            PropOrSpread::Prop(p) => {
                let Prop::KeyValue(kv) = &**p else {
                    if let Prop::Shorthand(id) = &**p {
                        if id.sym == "method" || id.sym == "type" {
                            return Verb::Unknowable;
                        }
                    }
                    continue;
                };
                let key = match &kv.key {
                    PropName::Ident(name) => name.sym.to_string(),
                    PropName::Str(s) => s.value.as_str().unwrap_or_default().to_string(),
                    // A computed key can spell either name; not decidable here.
                    PropName::Computed(_) => return Verb::Unknowable,
                    _ => continue,
                };
                if key != "method" && key != "type" {
                    continue;
                }
                let Expr::Lit(Lit::Str(s)) = &*kv.value else {
                    return Verb::Unknowable;
                };
                let v = s.value.as_str().unwrap_or_default();
                if !is_http_method(&v.to_ascii_lowercase()) {
                    return Verb::Unknowable;
                }
                if key == "method" {
                    return Verb::Literal(v.to_uppercase());
                }
                from_type = Some(v.to_uppercase());
            }
        }
    }
    match from_type {
        Some(v) => Verb::Literal(v),
        None => Verb::Default,
    }
}
