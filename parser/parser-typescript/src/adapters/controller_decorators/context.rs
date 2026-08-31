//! Class-level controller-gate resolution: `@Controller`/`@RestController` detection and
//! prefix/version resolution into a [`ControllerCtx`] — see the parent module's doc.

use swc_core::ecma::ast::{
    ArrayLit, Decorator, Expr, Lit, MemberProp, ObjectLit, Prop, PropName, PropOrSpread,
};

use super::method_facts::{decorator_name, str_value};

/// A controller class's own routing context — either a fully resolved literal prefix (already
/// including the `v<version>` segment when `version` was a string literal — see module doc), or a
/// dotted member-expression reference DEFERRED to assemble-time resolution (`controller-prefix-ref-v1`
/// — see module doc's "Scope (v1)" exception).
pub(super) enum ControllerCtx {
    Literal {
        prefix: String,
        /// The `version` the class declared, as [`version_expr_text`] normalized it — the
        /// `route-version-v1` discriminator, carried whether or not it also reached `prefix`. `None`
        /// when no `version` key was present or its value is a shape this cannot spell.
        route_version: Option<String>,
    },
    DeferredRef {
        prefix_ref: String,
    },
}

/// Class-level decorator names that gate a class as a controller — see module doc "Scope (v1)" for
/// why both are recognized.
const CONTROLLER_CLASS_GATES: &[&str] = &["Controller", "RestController"];

/// Scans a class's own decorators for `@Controller`/`@RestController` and returns its resolved
/// `ControllerCtx`, or `None` when neither is present or the prefix is unresolvable (see module doc).
pub(super) fn controller_context(decorators: &[Decorator]) -> Option<ControllerCtx> {
    for d in decorators {
        let Some(name) = decorator_name(&d.expr) else {
            continue;
        };
        if !CONTROLLER_CLASS_GATES.contains(&name.as_str()) {
            continue;
        }
        return controller_ctx_from_expr(&d.expr);
    }
    None
}

// Bare `@Controller` and empty-parens `@Controller()` both yield an empty prefix.
fn controller_ctx_from_expr(expr: &Expr) -> Option<ControllerCtx> {
    let Expr::Call(call) = expr else {
        return Some(ControllerCtx::Literal {
            prefix: String::new(),
            route_version: None,
        });
    };
    let Some(arg) = call.args.first() else {
        return Some(ControllerCtx::Literal {
            prefix: String::new(),
            route_version: None,
        });
    };
    match &*arg.expr {
        Expr::Lit(Lit::Str(s)) => Some(ControllerCtx::Literal {
            prefix: str_value(s),
            route_version: None,
        }),
        Expr::Object(obj) => object_controller_ctx(obj),
        // A dotted two-segment member expression (`RouteKey.Asset`) is deferred to assemble time
        // rather than skipped — see module doc's `controller-prefix-ref-v1` exception. Any deeper
        // chain (`A.B.C`) or computed member falls through `simple_member_ref` to `None`, which this
        // `.map` propagates — same skip-whole-controller outcome as any other unrecognized shape.
        Expr::Member(_) => {
            simple_member_ref(&arg.expr).map(|prefix_ref| ControllerCtx::DeferredRef { prefix_ref })
        }
        _ => None, // dynamic prefix arg (call/template/...) — never guess, skip the whole controller
    }
}

/// A dotted two-segment member-expression reference (`RouteKey.Asset`) — exactly the shape
/// `egress::const_map_fragment`/`flatten` key their constant-map entries by. `None` for anything
/// deeper (`A.B.C`), computed (`A[x]`), or not identifier-rooted.
fn simple_member_ref(expr: &Expr) -> Option<String> {
    let Expr::Member(m) = expr else { return None };
    let Expr::Ident(obj) = &*m.obj else {
        return None;
    };
    let MemberProp::Ident(prop) = &m.prop else {
        return None;
    };
    Some(format!("{}.{}", obj.sym, prop.sym))
}

fn object_controller_ctx(obj: &ObjectLit) -> Option<ControllerCtx> {
    let mut path = String::new();
    let mut path_seen = false;
    let mut path_dynamic = false;
    let mut version: Option<String> = None;
    let mut route_version: Option<String> = None;

    for prop in &obj.props {
        let PropOrSpread::Prop(p) = prop else {
            continue;
        };
        let Prop::KeyValue(kv) = &**p else {
            continue;
        };
        let Some(key) = prop_key_name(&kv.key) else {
            continue;
        };
        match key.as_str() {
            "path" => {
                path_seen = true;
                match first_literal_path(&kv.value) {
                    Some(p) => path = p,
                    None => path_dynamic = true,
                }
            }
            "version" => {
                if let Expr::Lit(Lit::Str(s)) = &*kv.value {
                    version = Some(str_value(s));
                }
                // A non-literal version still never enters the PATH — that would be a guess, and
                // under header/media-type versioning it would be a wrong one twice over, since the
                // version does not reach the URL at all. It is carried beside the key instead, as
                // the `route-version-v1` discriminator. See `version_expr_text`.
                route_version = version_expr_text(&kv.value);
            }
            _ => {}
        }
    }

    if path_seen && path_dynamic {
        return None; // an unresolvable `path` — never guess, skip the whole controller
    }

    let prefix = match version {
        Some(v) => format!("v{v}/{path}"),
        None => path,
    };
    Some(ControllerCtx::Literal {
        prefix,
        route_version,
    })
}

/// The `route-version-v1` discriminator for a `@Controller({ version: ... })` value: the expression
/// TEXT, normalized so that one version scope has exactly one spelling. `None` for a shape this cannot
/// spell and for anything that would normalize to the empty string — absence must read as absence.
///
/// **It resolves nothing, on purpose.** The corpus shapes are identifiers and arrays of them
/// (`API_VERSIONS_VALUES`, `[VERSION_2024_04_15, VERSION_2024_06_11]`) whose values sit two hops away
/// across workspace packages behind `as unknown as` casts. A per-file extractor cannot follow that
/// honestly, and the project-wide const map that could see the last hop
/// (`egress::const_map_fragment`) deliberately refuses bare top-level string consts because it is
/// scope-insensitive. So the consumer is told what the source SAID, and `zzop_core::IoProvide`'s
/// `route_version` doc states exactly what may and may not be concluded from two texts differing.
///
/// **One scope gets exactly one spelling, and that invariant has TWO halves.** Element ORDER is
/// normalized by sorting (`[B, A]` and `[A, B]` are one set written two ways), and the BRACKET FORM is
/// normalized by collapsing a one-element array to its scalar — Nest declares `version` as
/// `string | string[]` and wraps a scalar itself, so `version: [X]` and `version: X` are the same
/// `VersionValue`. Missing either half reports two version scopes where there is one, and a wrong
/// difference here buys a wrong severity demotion on the consuming rule.
fn version_expr_text(expr: &Expr) -> Option<String> {
    let Expr::Array(ArrayLit { elems, .. }) = expr else {
        return version_scalar_text(expr);
    };
    let mut parts = Vec::with_capacity(elems.len());
    for elem in elems {
        // A hole (`[, X]`) or a spread is not an element this can name — the whole set is unspellable
        // rather than silently short by one.
        let e = elem.as_ref()?;
        if e.spread.is_some() {
            return None;
        }
        parts.push(version_scalar_text(&e.expr)?);
    }
    if parts.is_empty() {
        return None;
    }
    // Nest's `version` is `string | string[]` and it WRAPS a scalar, so `version: [X]` and
    // `version: X` are the same `VersionValue`. Spelling the one-element array with brackets would
    // make one scope read as two texts and buy a demotion on a pair that declares the SAME version —
    // exactly the case the consuming rule keeps at `warning`. The bracket form is therefore reserved
    // for a set that actually has more than one member.
    if parts.len() == 1 {
        return parts.pop();
    }
    parts.sort();
    Some(format!("[{}]", parts.join(",")))
}

/// One element of a version value: a string literal's VALUE, or an identifier-rooted dotted chain
/// (`VERSION_2024_08_13_VALUE`, `ApiVersions.V2`) spelled as written. Whitespace is stripped so the
/// spelling does not depend on source formatting.
fn version_scalar_text(expr: &Expr) -> Option<String> {
    let raw = match expr {
        Expr::Lit(Lit::Str(s)) => str_value(s),
        _ => dotted_ident_chain(expr)?,
    };
    let stripped: String = raw.split_whitespace().collect();
    (!stripped.is_empty()).then_some(stripped)
}

/// An identifier-rooted dotted chain of any depth (`A`, `A.B`, `A.B.C`), spelled back out. `None` for
/// a computed member (`A[x]`), a call, a template, or anything not rooted in an identifier. Distinct
/// from [`simple_member_ref`], which is capped at two segments because it must match the const map's
/// own key shape; a version discriminator matches nothing and is only ever compared to itself.
fn dotted_ident_chain(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Ident(i) => Some(i.sym.to_string()),
        Expr::Member(m) => {
            let MemberProp::Ident(prop) = &m.prop else {
                return None;
            };
            Some(format!("{}.{}", dotted_ident_chain(&m.obj)?, prop.sym))
        }
        _ => None,
    }
}

/// A `path` attribute's value: a bare string literal, or ("first wins" — see module doc) the first
/// string-literal element of an array. `None` for anything else.
fn first_literal_path(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Lit(Lit::Str(s)) => Some(str_value(s)),
        Expr::Array(ArrayLit { elems, .. }) => {
            let first = elems.first()?.as_ref()?;
            match &*first.expr {
                Expr::Lit(Lit::Str(s)) => Some(str_value(s)),
                _ => None,
            }
        }
        _ => None,
    }
}

fn prop_key_name(key: &PropName) -> Option<String> {
    match key {
        PropName::Ident(i) => Some(i.sym.to_string()),
        PropName::Str(s) => Some(str_value(s)),
        _ => None,
    }
}
