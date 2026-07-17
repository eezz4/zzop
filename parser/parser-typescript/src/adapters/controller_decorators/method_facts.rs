//! Method-level route-fact resolution: verb/path decorators, the `@Body()` request-body contract
//! (`body-shape-v1`), and the shared decorator/name helpers — see the parent module's doc.

use swc_core::common::SourceMap;
use swc_core::ecma::ast::{
    ArrayLit, Callee, ClassMethod, Decorator, Expr, ExprOrSpread, Lit, Param, Pat, PropName, Str,
    TsEntityName, TsType,
};
use zzop_core::ProvideBodyShape;

/// Method-level route-decorator name -> the HTTP verb it implies. `All` is intentionally absent —
/// see module doc.
const METHOD_DECORATORS: &[(&str, &str)] = &[
    ("Get", "GET"),
    ("Post", "POST"),
    ("Put", "PUT"),
    ("Delete", "DELETE"),
    ("Patch", "PATCH"),
];

/// `(verb, handler name, anchor line, resolved path(s), `@Body()` request-body contract)` — see
/// `method_route_facts`'s doc.
pub(super) type MethodRouteFacts = (String, String, u32, Vec<String>, Option<ProvideBodyShape>);

/// One method's route facts, independent of its class's own prefix resolution — shared substrate for
/// both `ControllerCollector::emit_method` (literal-prefix provides) and
/// `ControllerPrefixFragmentCollector::emit_fragment` (deferred-prefix fragments): the handler name,
/// verb, anchor line, resolved path(s), and (`body-shape-v1`) the `@Body()` request-body contract. The
/// first four are `None` for the same reasons `method_route`/`method_name` individually return `None`
/// (no recognizable method key, no recognized route decorator, `@All`, or an unresolvable path); `body`
/// is independently `None`/`Some` per `method_body_shape`'s own never-guess rules and never vetoes the
/// rest of the tuple.
pub(super) fn method_route_facts(cm: &SourceMap, m: &ClassMethod) -> Option<MethodRouteFacts> {
    let name = method_name(&m.key)?;
    let (verb, decorator, paths) = method_route(&m.function.decorators)?;
    let line = crate::line_of(cm, decorator.span.lo);
    let body = method_body_shape(&m.function.params);
    Some((verb, name, line, paths, body))
}

/// `body-shape-v1`: resolves a method's `@Body()` request-body contract from its parameter list.
/// `None` (never-guess) unless EXACTLY ONE parameter carries an `@Body`/`@Body(...)` decorator AND
/// that one param is fully capturable — a literal-or-absent decorator argument (`body_sub_key`) AND a
/// plain single-identifier type annotation (`capturable_dto_ref`). Zero `@Body` params, 2+ `@Body`
/// params, a non-literal decorator argument, or a non-capturable type annotation (qualified name,
/// generic, array, primitive keyword, missing annotation) all fall through to `None` — this method's
/// route keeps `body: None` rather than guessing a partial shape.
fn method_body_shape(params: &[Param]) -> Option<ProvideBodyShape> {
    let mut found: Option<&Param> = None;
    for p in params {
        let has_body = p
            .decorators
            .iter()
            .any(|d| decorator_name(&d.expr).as_deref() == Some("Body"));
        if has_body {
            if found.is_some() {
                return None; // 2+ @Body params on one method -- ambiguous, never guess
            }
            found = Some(p);
        }
    }
    let param = found?; // zero @Body params -- nothing to capture
    let body_decorator = param
        .decorators
        .iter()
        .find(|d| decorator_name(&d.expr).as_deref() == Some("Body"))?;
    let sub_key = body_sub_key(&body_decorator.expr)?;
    let dto_ref = capturable_dto_ref(&param.pat)?;
    Some(ProvideBodyShape {
        sub_key,
        dto_ref: Some(dto_ref),
        fields: Vec::new(),
        complete: false,
    })
}

/// An `@Body(...)` decorator's sub-key, tri-state: `Some(None)` = capturable, whole-body (bare `@Body`
/// or empty-parens `@Body()`); `Some(Some(key))` = capturable, keyed under `key` (first argument is a
/// string literal, e.g. `@Body('user')`); `None` = NOT capturable — a non-literal first argument
/// (`@Body(x)`) — never guess which sub-key it would have resolved to.
fn body_sub_key(expr: &Expr) -> Option<Option<String>> {
    let Expr::Call(call) = expr else {
        return Some(None); // bare `@Body` (not even called)
    };
    let Some(arg) = call.args.first() else {
        return Some(None); // `@Body()` -- whole body
    };
    match &*arg.expr {
        Expr::Lit(Lit::Str(s)) => Some(Some(str_value(s))),
        _ => None, // non-literal first arg -- never guess
    }
}

/// A `@Body()` param's type annotation as a capturable single-identifier DTO ref: `Pat::Ident` whose
/// type annotation is a plain `TsTypeRef` (`TsEntityName::Ident`, no type params). `None` for a
/// qualified name (`A.B`), a generic (`Foo<T>`), an array/tuple/primitive-keyword annotation, or a
/// missing annotation entirely — any of these makes the DTO shape unresolvable from this file alone.
fn capturable_dto_ref(pat: &Pat) -> Option<String> {
    let Pat::Ident(bi) = pat else { return None };
    let ann = bi.type_ann.as_deref()?;
    let TsType::TsTypeRef(tr) = &*ann.type_ann else {
        return None;
    };
    if tr.type_params.is_some() {
        return None; // a generic (`Foo<T>`) -- not a plain DTO reference
    }
    let TsEntityName::Ident(id) = &tr.type_name else {
        return None; // a qualified name (`A.B`) -- not a plain DTO reference
    };
    Some(id.sym.to_string())
}

/// Scans one method's decorators for a route-verb decorator (or `@All`) — see module doc for why
/// every decorator is scanned rather than just the first. Returns `(VERB, the matching Decorator,
/// resolved paths)`, or `None` when nothing recognized, `@All` is present, or the path is dynamic.
pub(super) fn method_route(decorators: &[Decorator]) -> Option<(String, &Decorator, Vec<String>)> {
    for d in decorators {
        let Some(name) = decorator_name(&d.expr) else {
            continue;
        };
        if name == "All" {
            return None; // ambiguous verb — see module doc, mirrors Java's bare @RequestMapping skip
        }
        let Some((_, verb)) = METHOD_DECORATORS.iter().find(|(n, _)| *n == name) else {
            continue; // some other decorator (@UseGuards, @ApiTags, ...) — keep scanning
        };
        let paths = decorator_paths(&d.expr)?;
        return Some((verb.to_string(), d, paths));
    }
    None
}

/// A route decorator's resolved path(s): bare/`()` yields one empty-string path; a string literal
/// yields one path; an array yields one path per string-literal element (Nest registers each as its
/// own route). `None` for anything else — the caller treats that as "unresolvable, skip" (never guess).
// Bare `@Get` and empty-parens `@Get()` both yield one empty-string path.
fn decorator_paths(expr: &Expr) -> Option<Vec<String>> {
    let Expr::Call(call) = expr else {
        return Some(vec![String::new()]);
    };
    let Some(arg) = call.args.first() else {
        return Some(vec![String::new()]);
    };
    match &*arg.expr {
        Expr::Lit(Lit::Str(s)) => Some(vec![str_value(s)]),
        Expr::Array(ArrayLit { elems, .. }) => {
            let mut out = Vec::with_capacity(elems.len());
            for el in elems {
                let ExprOrSpread { spread: None, expr } = el.as_ref()? else {
                    return None; // a spread element — not a plain string-literal array
                };
                let Expr::Lit(Lit::Str(s)) = &**expr else {
                    return None; // a non-literal element — never guess the whole array
                };
                out.push(str_value(s));
            }
            Some(out)
        }
        _ => None, // dynamic path arg — never guess
    }
}

/// The decorator's callee/identifier name: `Get` from both bare `@Get` and called `@Get(...)`.
/// `None` for any unrecognized shape (a member expression, a non-identifier callee, ...).
pub(super) fn decorator_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Ident(id) => Some(id.sym.to_string()),
        Expr::Call(call) => match &call.callee {
            Callee::Expr(callee) => match &**callee {
                Expr::Ident(id) => Some(id.sym.to_string()),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    }
}

fn method_name(key: &PropName) -> Option<String> {
    match key {
        PropName::Ident(i) => Some(i.sym.to_string()),
        PropName::Str(s) => Some(str_value(s)),
        _ => None,
    }
}

pub(super) fn str_value(s: &Str) -> String {
    s.value.as_str().unwrap_or_default().to_string()
}

#[cfg(test)]
mod verb_pin {
    // T2 subset pin: every verb METHOD_DECORATORS emits must be a core join verb — a decorator
    // mapped to a verb outside `zzop_core::HTTP_KEY_VERBS` would produce permanently unjoinable
    // provides (the cross-layer key vocabulary is exactly that set).
    #[test]
    fn method_decorator_verbs_are_a_subset_of_core_http_key_verbs() {
        for (_, verb) in super::METHOD_DECORATORS {
            assert!(
                zzop_core::HTTP_KEY_VERBS.contains(verb),
                "METHOD_DECORATORS emits {verb}, not a core HTTP_KEY_VERBS member"
            );
        }
    }
}
