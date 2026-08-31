//! Runtime asset-URL reference extraction — string-literal paths passed to the browser asset loaders
//! that load a file by URL STRING rather than a static `import`: `AudioWorklet.addModule`,
//! `new Worker` / `new SharedWorker`, `importScripts`, `new URL(<path>, import.meta.url)`, and
//! `navigator.serviceWorker.register(<path>)`. Such a
//! reference is invisible to the static import graph, so a `public/`-served `.js` worklet/worker loaded
//! only this way reads as `fan_in == 0` and is a `dead-candidates` false positive. This pass captures the
//! path STRING verbatim (raw, unresolved); the engine resolves it against the tree's `public/`/`static/`
//! root (or a relative module path) and bumps the target's fan-in — mirroring the import pre-scan's fan-in bump — so
//! the file drops out of `dead-candidates` (and is seeded as an `unreachable` entrypoint). Only these
//! sinks are captured: they take an unambiguous file-reference string and target `.js`-family files.
//! `fetch`/`<img src>`/`<link href>` are deliberately NOT captured — they target non-eligible extensions
//! (images/CSS are never `dead-candidates`) or server routes, giving zero dead-candidates benefit and
//! real reachability-FP surface (rules-owner assessment).
//!
//! ## `serviceWorker.register` is rebased here, and the rebase is an ASSUMPTION
//! Four of the other five — `new Worker`, `new SharedWorker`, `importScripts` and
//! `new URL(_, import.meta.url)` — resolve their argument against the MODULE. A service-worker
//! registration does not:
//! the browser resolves it against the PAGE URL, so koel's
//! `resources/assets/js/app.ts` calling `navigator.serviceWorker?.register('./sw.js')` loads
//! served-root `/sw.js` — `public/sw.js` — and never `resources/assets/js/sw.js`. Handing the raw
//! string to a module-relative resolver would quietly resolve to nothing, which looks exactly like
//! "no such sink" and is why the rebase happens at CAPTURE: a registration path is emitted
//! served-absolute (a leading `/`), the one form the engine's resolver already reads as
//! `public/`/`static/`-rooted. A registration argument that is already absolute is unchanged, and a
//! string carrying any SCHEME (`https:`, `data:`, `blob:`) is not a file reference at all and is
//! skipped.
//!
//! What the rebase ASSUMES, stated because it is an assumption and not a declaration: that the
//! registering page is served at the site root. It usually is — a service worker's scope is bounded by
//! its own path, so an app that registers `./sw.js` is normally registering the worker that scopes the
//! whole origin — but an app served under a sub-path registers `/<sub>/sw.js`, and this pass emits
//! `/sw.js`. In a monorepo the engine's served-absolute resolver would then bump a DIFFERENT app's
//! `public/sw.js`. `../` is refused outright for the same unknown taken one step further, where no
//! served-absolute spelling exists at all. The sound version of this needs the io channel to carry the
//! reference's ORIGIN so the engine — which can see the tree — resolves it; that is a shape change and
//! is filed rather than guessed at here.
//!
//! `AudioWorklet.addModule` is the one whose origin this module does NOT claim to know. On the main
//! thread the platform resolves it against the document base URL, like a registration; the engine
//! resolves it module-relatively, like the other four. That asymmetry predates this sink, is not
//! changed by it, and is recorded rather than repaired: repairing it would move an existing behaviour
//! with no measurement behind the move.

use swc_core::ecma::ast::{
    CallExpr, Callee, Expr, ExprOrSpread, Lit, MemberExpr, MemberProp, NewExpr, OptCall,
    OptChainBase,
};
use swc_core::ecma::visit::{Visit, VisitWith};

use crate::parse_module;

/// Captures each runtime asset-loader reference's static string path, in source-visit order
/// (deterministic — no map, no sort). See the module doc for the sinks and the raw-capture contract,
/// including the one that is rebased rather than captured raw.
pub fn parse_asset_refs(file: &str, source: &str) -> Vec<String> {
    let Some(module) = parse_module(file, source) else {
        return Vec::new();
    };
    let mut collector = AssetRefCollector { out: Vec::new() };
    module.visit_with(&mut collector);
    collector.out
}

struct AssetRefCollector {
    out: Vec<String>,
}

impl Visit for AssetRefCollector {
    fn visit_call_expr(&mut self, call: &CallExpr) {
        if let Callee::Expr(callee) = &call.callee {
            if let Some(m) = callee_member(callee) {
                self.member_call(m, &call.args);
            }
            // `importScripts("<a>", "<b>", ...)` — variadic; every string-literal arg is a load, and
            // the only sink whose callee is a bare identifier rather than a member.
            if matches!(unwrap_expr(callee), Expr::Ident(id) if id.sym == "importScripts") {
                for a in &call.args {
                    if let Some(p) = static_str_arg(a) {
                        self.out.push(p);
                    }
                }
            }
        }
        call.visit_children_with(self);
    }

    /// `a?.b(...)` is an `OptCall`, NOT a `CallExpr`, so the visitor above never sees it — and the
    /// guarded spelling is the one real code uses for this sink: koel writes
    /// `navigator.serviceWorker?.register('./sw.js')`, because `navigator.serviceWorker` is undefined
    /// outside a secure context. Capturing the unguarded spelling alone would have looked like a
    /// working sink while missing the case it was built for.
    fn visit_opt_call(&mut self, n: &OptCall) {
        if let Some(m) = callee_member(&n.callee) {
            self.member_call(m, &n.args);
        }
        n.visit_children_with(self);
    }

    fn visit_new_expr(&mut self, new: &NewExpr) {
        if let Expr::Ident(id) = unwrap_expr(&new.callee) {
            let args = new.args.as_deref().unwrap_or(&[]);
            if id.sym == "Worker" || id.sym == "SharedWorker" {
                // `new Worker("<path>")` / `new SharedWorker("<path>")` — first arg, any-arity (a 2nd
                // `{ type: "module" }` options arg is common).
                self.push_first_str(args);
            } else if id.sym == "URL"
                && args
                    .get(1)
                    .map(|a| is_import_meta_url(&a.expr))
                    .unwrap_or(false)
            {
                // `new URL("<path>", import.meta.url)` — the Vite/bundler asset pattern. Gated on the
                // `import.meta.url` 2nd arg so a real `new URL("https://…")` is never captured.
                self.push_first_str(args);
            }
        }
        new.visit_children_with(self);
    }
}

impl AssetRefCollector {
    /// Two member-call sinks, one place, because both spellings of a call (`x.f()` and `x?.f()`)
    /// arrive here. Each is gated on its RECEIVER, not on the method name alone — the precision
    /// discipline this pass keeps everywhere: a same-named `.addModule` on a registry, or
    /// `.register` on a DI container, is not an asset load. `addModule`'s 2nd argument is an
    /// options object, so first arg only.
    fn member_call(&mut self, m: &swc_core::ecma::ast::MemberExpr, args: &[ExprOrSpread]) {
        let prop = match &m.prop {
            MemberProp::Ident(p) => p.sym.as_str(),
            _ => "",
        };
        let recv = unwrap_expr(&m.obj);
        if prop == "addModule" && is_audio_worklet_receiver(recv) {
            self.push_first_str(args);
        } else if prop == "register" && is_service_worker_receiver(recv) {
            // The one sink whose argument is PAGE-relative rather than module-relative, rebased to
            // served-root here — module doc owns why.
            if let Some(served) = args
                .first()
                .and_then(static_str_arg)
                .as_deref()
                .and_then(served_root_path)
            {
                self.out.push(served);
            }
        }
    }

    fn push_first_str(&mut self, args: &[ExprOrSpread]) {
        if let Some(p) = args.first().and_then(static_str_arg) {
            self.out.push(p);
        }
    }
}

/// A static string path from one argument — a plain string literal or a no-substitution template
/// (`` `/x.js` ``). `None` for a spread, a computed/interpolated arg, or any non-string expr.
fn static_str_arg(arg: &ExprOrSpread) -> Option<String> {
    if arg.spread.is_some() {
        return None;
    }
    match unwrap_expr(&arg.expr) {
        Expr::Lit(Lit::Str(s)) => Some(s.value.as_str().unwrap_or_default().to_string()),
        Expr::Tpl(t) if t.exprs.is_empty() && t.quasis.len() == 1 => t.quasis[0]
            .cooked
            .as_ref()
            .and_then(|a| a.as_str())
            .map(str::to_string),
        _ => None,
    }
}

/// True if `e` is a member access whose property is `audioWorklet` (e.g. `ctx.audioWorklet`,
/// `captureCtx.audioWorklet`) — the receiver of an AudioWorklet `.addModule(...)` call.
fn is_audio_worklet_receiver(e: &Expr) -> bool {
    member_prop_is(e, "audioWorklet")
}

/// Whether `e` is a member access whose property is `name`, through EITHER spelling of the access.
/// `navigator?.serviceWorker` is an `OptChain`, not a `Member`, and requiring the bare node was a
/// silent hole in exactly the code most likely to write it: SSR and universal bundles guard `navigator`
/// itself because it does not exist on the server.
fn member_prop_is(e: &Expr, name: &str) -> bool {
    let member = match e {
        Expr::Member(m) => Some(m),
        Expr::OptChain(o) => match &*o.base {
            OptChainBase::Member(m) => Some(m),
            OptChainBase::Call(_) => None,
        },
        _ => None,
    };
    member.is_some_and(|m| matches!(&m.prop, MemberProp::Ident(p) if p.sym == *name))
}

/// The member access a call is calling THROUGH, if it is one. Three spellings reach the two visitors
/// for the same call and they must not disagree about it: `x.f(a)` puts the member in callee position,
/// `x?.f(a)` wraps it in an optional-chain node, and `x.f?.(a)` is already an `OptCall` whose callee is
/// a plain member. The koel line this pass was built for is the middle one. An optional-chain CALL in
/// callee position (`a?.b()(c)`) is a different thing and is deliberately not unwrapped — the same
/// distinction `call_sites::callee_member` draws, for the same reason.
fn callee_member(expr: &Expr) -> Option<&MemberExpr> {
    match unwrap_expr(expr) {
        Expr::Member(m) => Some(m),
        Expr::OptChain(o) => match &*o.base {
            OptChainBase::Member(m) => Some(m),
            OptChainBase::Call(_) => None,
        },
        _ => None,
    }
}

/// True if `e` is a member access whose property is `serviceWorker` — `navigator.serviceWorker`, and
/// equally `navigator?.serviceWorker` after optional-chain unwrapping, which is how the guarded form
/// koel writes (`navigator.serviceWorker?.register(...)`) arrives here.
fn is_service_worker_receiver(e: &Expr) -> bool {
    member_prop_is(e, "serviceWorker")
}

/// A service-worker registration path rebased to served-root, or `None` when it is not a file
/// reference this pass can place.
///
/// `./sw.js` -> `/sw.js`, `sw.js` -> `/sw.js`, `/sw.js` unchanged. An absolute URL is skipped
/// outright — it names another origin's file, and there is nothing in this tree to bump. `../` is
/// skipped too rather than guessed at: page-relative `..` depends on the page's own served path,
/// which this pass does not know, and a wrong resolution here would silently exempt some OTHER file
/// from `dead-candidates`.
fn served_root_path(raw: &str) -> Option<String> {
    let clean = raw.split(['?', '#']).next().unwrap_or(raw);
    // Any SCHEME at all, not just `://`: `data:` and `blob:` name no file in this tree, and rewriting
    // one to `/data:…` would hand the engine a served-ABSOLUTE string, routing it around the very
    // `data:`/`blob:` skip its resolver applies to bare specifiers.
    let scheme_end = clean.find(':');
    let first_slash = clean.find('/');
    if clean.is_empty()
        || clean.starts_with("//")
        || scheme_end.is_some_and(|c| first_slash.is_none_or(|s| c < s))
    {
        return None;
    }
    if let Some(rest) = clean.strip_prefix("./") {
        return (!rest.is_empty()).then(|| format!("/{rest}"));
    }
    if clean.starts_with("../") {
        return None;
    }
    if clean.starts_with('/') {
        return Some(clean.to_string());
    }
    Some(format!("/{clean}"))
}

/// True if `e` is exactly `import.meta.url` — the `new URL(<path>, import.meta.url)` asset marker.
/// Matches `Expr::MetaProp(_)` structurally (`import.meta` is the only meta-property one takes `.url`
/// of), avoiding a hard dependency on the `MetaPropKind` enum surface.
fn is_import_meta_url(e: &Expr) -> bool {
    matches!(
        unwrap_expr(e),
        Expr::Member(m)
            if matches!(&m.prop, MemberProp::Ident(p) if p.sym == "url")
                && matches!(unwrap_expr(&m.obj), Expr::MetaProp(_))
    )
}

/// Strip wrappers between an expression and its real value: `(...)`, `... as const`, `... satisfies T`,
/// `...!` — mirrors the adapter modules' own `unwrap_expr` (e.g. `adapters::hono_client::scan`).
fn unwrap_expr(e: &Expr) -> &Expr {
    let mut n = e;
    loop {
        n = match n {
            Expr::TsAs(a) => &a.expr,
            Expr::TsConstAssertion(c) => &c.expr,
            Expr::Paren(p) => &p.expr,
            Expr::TsSatisfies(s) => &s.expr,
            Expr::TsNonNull(nn) => &nn.expr,
            other => return other,
        };
    }
}

#[cfg(test)]
mod tests;
