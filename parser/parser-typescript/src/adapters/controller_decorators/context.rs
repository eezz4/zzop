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
    Literal { prefix: String },
    DeferredRef { prefix_ref: String },
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
        });
    };
    let Some(arg) = call.args.first() else {
        return Some(ControllerCtx::Literal {
            prefix: String::new(),
        });
    };
    match &*arg.expr {
        Expr::Lit(Lit::Str(s)) => Some(ControllerCtx::Literal {
            prefix: str_value(s),
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
                // non-literal version -> best-effort skip of just the version segment, not the
                // whole controller (see module doc).
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
    Some(ControllerCtx::Literal { prefix })
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
