//! Runtime asset-URL reference extraction — string-literal paths passed to the browser asset loaders
//! that load a file by URL STRING rather than a static `import`: `AudioWorklet.addModule`,
//! `new Worker` / `new SharedWorker`, `importScripts`, and `new URL(<path>, import.meta.url)`. Such a
//! reference is invisible to the static import graph, so a `public/`-served `.js` worklet/worker loaded
//! only this way reads as `fan_in == 0` and is a `dead-candidates` false positive. This pass captures the
//! path STRING verbatim (raw, unresolved); the engine resolves it against the tree's `public/`/`static/`
//! root (or a relative module path) and bumps the target's fan-in — mirroring the SFC fan-in bump — so
//! the file drops out of `dead-candidates` (and is seeded as an `unreachable` entrypoint). Only these
//! five sinks are captured: they take an unambiguous file-reference string and target `.js`-family files.
//! `fetch`/`<img src>`/`<link href>` are deliberately NOT captured — they target non-eligible extensions
//! (images/CSS are never `dead-candidates`) or server routes, giving zero dead-candidates benefit and
//! real reachability-FP surface (rules-owner assessment).

use swc_core::ecma::ast::{CallExpr, Callee, Expr, ExprOrSpread, Lit, MemberProp, NewExpr};
use swc_core::ecma::visit::{Visit, VisitWith};

use crate::parse_module;

/// Captures each runtime asset-loader reference's static string path, in source-visit order
/// (deterministic — no map, no sort). See the module doc for the five sinks and the raw-capture contract.
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
            match unwrap_expr(callee) {
                // `<expr>.audioWorklet.addModule("<path>")` — the receiver must be an `.audioWorklet`
                // member (the precision gate: a same-named `.addModule` on an unrelated object is not an
                // asset load). AudioWorklet.addModule's 2nd arg is an options object — first arg only.
                Expr::Member(m) => {
                    if matches!(&m.prop, MemberProp::Ident(p) if p.sym == "addModule")
                        && is_audio_worklet_receiver(unwrap_expr(&m.obj))
                    {
                        self.push_first_str(&call.args);
                    }
                }
                // `importScripts("<a>", "<b>", ...)` — variadic; every string-literal arg is a load.
                Expr::Ident(id) if id.sym == "importScripts" => {
                    for a in &call.args {
                        if let Some(p) = static_str_arg(a) {
                            self.out.push(p);
                        }
                    }
                }
                _ => {}
            }
        }
        call.visit_children_with(self);
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
    matches!(e, Expr::Member(m) if matches!(&m.prop, MemberProp::Ident(p) if p.sym == "audioWorklet"))
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
mod tests {
    use super::parse_asset_refs;

    fn refs(src: &str) -> Vec<String> {
        parse_asset_refs("x.ts", src)
    }

    #[test]
    fn audio_worklet_add_module_is_captured() {
        assert_eq!(
            refs("await ctx.audioWorklet.addModule(\"/noise/worklet.js\");\n"),
            vec!["/noise/worklet.js".to_string()]
        );
    }

    #[test]
    fn add_module_on_non_audioworklet_receiver_is_skipped() {
        // A same-named `.addModule` on an unrelated object is not an asset load.
        assert!(refs("registry.addModule(\"/x.js\");\n").is_empty());
    }

    #[test]
    fn new_worker_and_shared_worker_first_arg() {
        assert_eq!(
            refs("const w = new Worker(\"/w/a.js\", { type: \"module\" });\n"),
            vec!["/w/a.js".to_string()]
        );
        assert_eq!(
            refs("new SharedWorker(\"./b.js\");\n"),
            vec!["./b.js".to_string()]
        );
    }

    #[test]
    fn import_scripts_is_variadic() {
        assert_eq!(
            refs("importScripts(\"/a.js\", \"/b.js\");\n"),
            vec!["/a.js".to_string(), "/b.js".to_string()]
        );
    }

    #[test]
    fn new_url_with_import_meta_url_is_captured() {
        assert_eq!(
            refs("const u = new URL(\"./worker.ts\", import.meta.url);\n"),
            vec!["./worker.ts".to_string()]
        );
    }

    #[test]
    fn new_url_without_import_meta_url_is_skipped() {
        // A bare `new URL("https://…")` is a real URL, not an asset reference.
        assert!(refs("const u = new URL(\"https://api.example.com/v1\");\n").is_empty());
        assert!(refs("const u = new URL(\"/x.js\", someBase);\n").is_empty());
    }

    #[test]
    fn non_literal_arg_is_skipped() {
        assert!(refs("const p = getPath(); new Worker(p);\n").is_empty());
    }

    #[test]
    fn no_substitution_template_is_captured_but_interpolated_is_not() {
        assert_eq!(
            refs("new Worker(`/w/a.js`);\n"),
            vec!["/w/a.js".to_string()]
        );
        assert!(refs("new Worker(`/w/${name}.js`);\n").is_empty());
    }
}
