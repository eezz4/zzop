//! Classification of one `.use(...)` call link (Express-vocabulary receivers only — the caller
//! gates on `is_express` before calling in). Split out of `build::FragmentBuilder::classify_call`
//! since `.use` alone carries the full middleware guard-name judgment (mount vs. `ScopedAttr`)
//! across 1-arg/2-arg/multi-arg call shapes. See the parent module doc for the recognizer spec.

use swc_core::ecma::ast::{CallExpr, Expr};
use zzop_core::{ImportMap, RouterMountEntry};

use super::build::string_lit_arg;
use super::chain::unwrap_expr;
use super::guard::{judge_guard_arg, AUTH_GUARDED_ATTR_KEY};

/// Express mounts sub-routers via `.use(prefixLit, subRouter)`, gated on Express vocabulary since
/// Hono's `.use` is always middleware. Known limit: a plain-ident second arg that is actually
/// middleware (e.g. `app.use('/api', logger)`) still mints a `Mount` that fails to resolve at
/// compose — an accepted cost of this recognizer's existing conservatism, now partially completed
/// by the guard-name judgment below (a RECOGNIZED guard name/callee at least gets an `attr_keys`
/// entry on that unresolving `Mount`, so the compose pass's PathScope fallback still fires).
///
/// A single-argument `.use(ident)` is the routes.ts aggregation idiom
/// (`Router().use(controllerA).use(controllerB)`) — a prefix-less mount at "/", which
/// `join_prefix` in the compose pass treats as a pure passthrough (no double slash). A single
/// non-identifier, non-call argument (`app.use(cors())` handled below,
/// `app.use(express.static(...))`) is middleware, not a mount, and is skipped. A BARE-identifier
/// middleware arg (`app.use(helmet)`, `app.use(errorHandler)`) still mints a Mount that fails to
/// resolve at compose (middleware modules are not router fragments) — the same accepted
/// conservatism cost as the two-arg middleware case below, now carrying `attr_keys` when the
/// name/callee judges as a guard.
///
/// A single CALL argument (`app.use(requireAuth())`) is judged for guard vocabulary: a
/// recognized guard emits a `ScopedAttr` at the router's root ("/"); an unjudged call
/// (`app.use(cors())`) is skipped exactly as before.
pub(super) fn classify_use_call(
    call: &CallExpr,
    line: u32,
    imports: &ImportMap,
) -> Vec<RouterMountEntry> {
    match call.args.len() {
        1 => {
            let arg = &call.args[0];
            match unwrap_expr(&arg.expr) {
                Expr::Ident(id) => {
                    let ident = id.sym.to_string();
                    let specifier = imports.get(&ident).map(|b| b.specifier.clone());
                    let attr_keys = if judge_guard_arg(unwrap_expr(&arg.expr)) {
                        vec![AUTH_GUARDED_ATTR_KEY.to_string()]
                    } else {
                        Vec::new()
                    };
                    vec![RouterMountEntry::Mount {
                        prefix: "/".to_string(),
                        ident,
                        specifier,
                        attr_keys,
                    }]
                }
                call_expr @ Expr::Call(_) => {
                    if judge_guard_arg(call_expr) {
                        vec![RouterMountEntry::ScopedAttr {
                            prefix: "/".to_string(),
                            key: AUTH_GUARDED_ATTR_KEY.to_string(),
                            line,
                        }]
                    } else {
                        Vec::new()
                    }
                }
                _ => Vec::new(),
            }
        }
        // Exactly 2 args with a literal first-arg prefix is the `.use(prefixLit, arg)` shape:
        // `arg` is either the mounted sub-router (an ident) or a single router-scoped guard (a
        // call) — never scanned as a multi-middleware list (that's the `_` arm below, which also
        // covers a literal-prefixed 2-arg call whose SECOND arg isn't ident/call, i.e. falls
        // through to `_ => Vec::new()` there too via this same distinction on args.len() == 2 with
        // a resolved literal prefix).
        2 if string_lit_arg(call.args.first()).is_some() => {
            // Re-fetched (cheap — a short string literal) rather than threading the `Option`
            // through the match guard.
            let prefix = string_lit_arg(call.args.first()).unwrap();
            let arg = &call.args[1];
            match unwrap_expr(&arg.expr) {
                Expr::Ident(id) => {
                    let ident = id.sym.to_string();
                    let specifier = imports.get(&ident).map(|b| b.specifier.clone());
                    let attr_keys = if judge_guard_arg(unwrap_expr(&arg.expr)) {
                        vec![AUTH_GUARDED_ATTR_KEY.to_string()]
                    } else {
                        Vec::new()
                    };
                    vec![RouterMountEntry::Mount {
                        prefix,
                        ident,
                        specifier,
                        attr_keys,
                    }]
                }
                call_expr @ Expr::Call(_) => {
                    if judge_guard_arg(call_expr) {
                        vec![RouterMountEntry::ScopedAttr {
                            prefix,
                            key: AUTH_GUARDED_ATTR_KEY.to_string(),
                            line,
                        }]
                    } else {
                        Vec::new()
                    }
                }
                _ => Vec::new(),
            }
        }
        // Every other shape: `.use(prefixLit, mw1, mw2)` (3+ args) or `.use(mw1, mw2)` (2+ args,
        // no literal-prefix first arg). Never mints a `Mount` from a multi-middleware list
        // (unchanged conservatism — this shape is never a single sub-router mount); each
        // CALL-shaped argument is independently judged, so `.use('/api', requireAuth(),
        // rateLimit())` emits exactly one `ScopedAttr` (for the judged `requireAuth()` call), not
        // zero or two.
        _ => {
            let prefix = string_lit_arg(call.args.first()).unwrap_or_else(|| "/".to_string());
            call.args
                .iter()
                .filter_map(|a| {
                    let e = unwrap_expr(&a.expr);
                    matches!(e, Expr::Call(_)).then_some(e)
                })
                .filter(|e| judge_guard_arg(e))
                .map(|_| RouterMountEntry::ScopedAttr {
                    prefix: prefix.clone(),
                    key: AUTH_GUARDED_ATTR_KEY.to_string(),
                    line,
                })
                .collect()
        }
    }
}
