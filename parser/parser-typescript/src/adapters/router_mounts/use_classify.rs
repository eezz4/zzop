//! Classification of one `.use(...)` call link (Express-vocabulary receivers only — the caller
//! gates on `is_express` before calling in). Split out of `build::FragmentBuilder::classify_call`
//! since `.use` alone carries the full middleware guard-name judgment (mount vs. `ScopedAttr`)
//! across 1-arg/2-arg/multi-arg call shapes. See the parent module doc for the recognizer spec.

use swc_core::ecma::ast::{CallExpr, Callee, Expr};
use zzop_core::{ImportMap, RouterMountEntry};

use super::build::{route_path_lit_arg, string_lit_arg};
use super::chain::unwrap_expr;
use super::guard::{judge_guard_arg, AUTH_GUARDED_ATTR_KEY};
use super::RouterMountVocab;

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
/// `require('<literal>')` sitting where a sub-router identifier would sit — the CommonJS half of
/// the mount idiom, and the half this recognizer used to drop on the floor.
///
/// `app.use('/api/v1', require('./controllers/v1'))` is what `expressjs/express`'s own
/// `examples/multi-router` ships, and it reached the CALL arm below, judged as a guard, failed that,
/// and was skipped. Measured 2026-08-19 on a 3-file fixture: the mounted routes then composed at
/// their OWN keys (`GET /`, `GET /users`) instead of `GET /api/v1/...`, which is the mis-keying the
/// `MountRef` doc calls worse than not emitting — and two routers mounted at two prefixes therefore
/// collided into a false `duplicate-route`. The identifier spelling of the same program
/// (`const v1 = require('./controllers/v1'); app.use('/api/v1', v1)`) already composed correctly, so
/// the defect was the shape of the ARGUMENT, never the mount.
///
/// Deliberately narrow: exactly one string-literal argument to a callee spelled `require`. A dynamic
/// specifier is not a literal and stays skipped, because a specifier this pass cannot read is a
/// target it cannot resolve.
fn require_specifier(expr: &Expr) -> Option<String> {
    let Expr::Call(call) = expr else { return None };
    let Callee::Expr(callee) = &call.callee else {
        return None;
    };
    let Expr::Ident(id) = unwrap_expr(callee) else {
        return None;
    };
    if id.sym.as_str() != "require" || call.args.len() != 1 {
        return None;
    }
    string_lit_arg(call.args.first())
}

/// The binding name the author WOULD have written for this module, used only as the by-name lookup
/// key at compose. An inline `require` has no binding, and the composer needs some ident: it tries a
/// fragment of that name in the resolved file and otherwise falls back to "the file declares exactly
/// one router", which is the shape a controller module has. Getting this wrong therefore costs a
/// lookup miss, and a missed mount DROPS the child's routes rather than emitting them at the wrong
/// key — the direction this module already prefers.
fn conventional_ident(specifier: &str) -> String {
    let last = specifier.rsplit('/').next().unwrap_or(specifier);
    let stem = last.rsplit_once('.').map_or(last, |(before, _)| before);
    if stem.is_empty() || stem == "index" {
        // `./controllers/index` names the DIRECTORY, which is what a reader would have bound it to.
        let parent = specifier
            .trim_end_matches('/')
            .rsplit('/')
            .nth(1)
            .unwrap_or("");
        if !parent.is_empty() && parent != "." && parent != ".." {
            return parent.to_string();
        }
    }
    stem.to_string()
}

pub(super) fn classify_use_call(
    call: &CallExpr,
    line: u32,
    imports: &ImportMap,
    vocab: &RouterMountVocab<'_>,
) -> Vec<RouterMountEntry> {
    match call.args.len() {
        1 => {
            let arg = &call.args[0];
            match unwrap_expr(&arg.expr) {
                Expr::Ident(id) => {
                    let ident = id.sym.to_string();
                    let specifier = imports.get(&ident).map(|b| b.specifier.clone());
                    let attr_keys = if judge_guard_arg(unwrap_expr(&arg.expr), vocab) {
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
                    // A require FIRST: `app.use(require('./routes'))` is a prefix-less mount, and
                    // asking the guard vocabulary about it would only ever answer "not a guard".
                    if let Some(spec) = require_specifier(call_expr) {
                        vec![RouterMountEntry::Mount {
                            prefix: "/".to_string(),
                            ident: conventional_ident(&spec),
                            specifier: Some(spec),
                            attr_keys: Vec::new(),
                        }]
                    } else if judge_guard_arg(call_expr, vocab) {
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
            let prefix = route_path_lit_arg(call.args.first()).unwrap();
            let arg = &call.args[1];
            match unwrap_expr(&arg.expr) {
                Expr::Ident(id) => {
                    let ident = id.sym.to_string();
                    let specifier = imports.get(&ident).map(|b| b.specifier.clone());
                    let attr_keys = if judge_guard_arg(unwrap_expr(&arg.expr), vocab) {
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
                    if let Some(spec) = require_specifier(call_expr) {
                        vec![RouterMountEntry::Mount {
                            prefix,
                            ident: conventional_ident(&spec),
                            specifier: Some(spec),
                            attr_keys: Vec::new(),
                        }]
                    } else if judge_guard_arg(call_expr, vocab) {
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
            let prefix = route_path_lit_arg(call.args.first()).unwrap_or_else(|| "/".to_string());
            call.args
                .iter()
                .filter_map(|a| {
                    let e = unwrap_expr(&a.expr);
                    matches!(e, Expr::Call(_)).then_some(e)
                })
                .filter(|e| judge_guard_arg(e, vocab))
                .map(|_| RouterMountEntry::ScopedAttr {
                    prefix: prefix.clone(),
                    key: AUTH_GUARDED_ATTR_KEY.to_string(),
                    line,
                })
                .collect()
        }
    }
}
