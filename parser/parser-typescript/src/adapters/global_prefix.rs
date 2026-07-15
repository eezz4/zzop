//! NestJS `app.setGlobalPrefix('api')` marker — a project/tree-level HTTP-route prefix that NestJS
//! applies to every controller route, but which the per-file controller-decorator extractor
//! (`controller_decorators.rs`) has no way to see (it operates one file at a time, and the prefix is
//! usually declared in a bootstrap file like `main.ts` that has no `@Controller` at all).
//!
//! Rather than bump `CACHE_SCHEMA_VERSION` to add a new field somewhere, this rides the existing
//! (already-cached) `IoFacts.provides` channel with a sentinel `IoProvide { kind: "nest-global-prefix",
//! key: <the literal>, ... }` — `IoKind` is an open `String` by design (`zzop_core::IoProvide`'s own
//! doc: "an adapter may introduce its own kind"). `zzop-engine`'s tree-assembly pass
//! (`crates/engine/src/analyze.rs`) collects every such sentinel, uses it to rewrite every `http`
//! provide's key, and then strips the sentinel itself so it never reaches output or the cross-layer
//! join.
//!
//! Only a string-literal argument is recognized — `app.setGlobalPrefix(cfg.prefix)` (or any other
//! non-literal expression) emits nothing, per this repo's "never guess" IO convention (see
//! `egress.rs`'s `resolve_url`): a wrong prefix would mis-key every route in the tree, which is worse
//! than emitting none.

use swc_core::common::SourceMap;
use swc_core::ecma::ast::{CallExpr, Callee, Expr, Lit, MemberProp};
use swc_core::ecma::visit::{Visit, VisitWith};
use zzop_core::IoProvide;

/// The sentinel `IoProvide::kind` — `zzop-engine`'s tree assembly collects and strips it (never joined,
/// never output).
pub const NEST_GLOBAL_PREFIX_KIND: &str = "nest-global-prefix";

/// Scans one TS file's raw source for a `<expr>.setGlobalPrefix(<stringLiteral>)` call and, if found,
/// returns a sentinel `IoProvide { kind: "nest-global-prefix", key: <the literal, verbatim>, ... }`.
/// Returns `None` when the file has no such call, the call's argument isn't a plain string literal
/// (identifier/template/concat/...), or the file fails to parse. Only the first matching call is
/// reported — `setGlobalPrefix` is called at most once per real NestJS app.
///
/// The match is receiver-agnostic (any `<expr>.setGlobalPrefix("literal")`, name-only), so its blast
/// radius is Nest-provide-wide: a single stray call anywhere in the tree rewrites EVERY Nest-controller
/// `http` provide's key at assembly time. This is accepted because `setGlobalPrefix` is a Nest-specific
/// method name — a same-named method on an unrelated object is vanishingly unlikely — but the tree-wide
/// reach is deliberate, not incidental.
pub fn extract_global_prefix_marker(rel: &str, text: &str) -> Option<IoProvide> {
    let (cm, module) = crate::parse_with_cm(rel, text)?;
    let cm_ref: &SourceMap = &cm;
    let mut c = GlobalPrefixCollector {
        cm: cm_ref,
        file: rel,
        out: None,
    };
    module.visit_with(&mut c);
    c.out
}

struct GlobalPrefixCollector<'a> {
    cm: &'a SourceMap,
    file: &'a str,
    out: Option<IoProvide>,
}

impl Visit for GlobalPrefixCollector<'_> {
    fn visit_call_expr(&mut self, call: &CallExpr) {
        if self.out.is_none() {
            if let Some(key) = match_set_global_prefix(call) {
                self.out = Some(IoProvide {
                    body: None,
                    kind: NEST_GLOBAL_PREFIX_KIND.to_string(),
                    key,
                    file: self.file.to_string(),
                    line: crate::line_of(self.cm, call.span.lo),
                    symbol: None,
                });
                return; // fully handled — no need to recurse into this call's own arguments
            }
        }
        call.visit_children_with(self); // recurse — the call may be nested inside another expression
    }
}

/// Matches `<expr>.setGlobalPrefix(<stringLiteral>)` — receiver is unchecked (any object; mirrors this
/// crate's other adapters' lexical-name-only matching, e.g. `controller_decorators.rs`'s decorator
/// gate), so `app.setGlobalPrefix(...)` and `nestApp.setGlobalPrefix(...)` both match. Returns the
/// literal's verbatim value, or `None` for any other method name or a non-literal/absent argument.
fn match_set_global_prefix(call: &CallExpr) -> Option<String> {
    let Callee::Expr(callee) = &call.callee else {
        return None;
    };
    let Expr::Member(m) = &**callee else {
        return None;
    };
    let MemberProp::Ident(name) = &m.prop else {
        return None;
    };
    if name.sym != "setGlobalPrefix" {
        return None;
    }
    let arg = call.args.first()?;
    match &*arg.expr {
        Expr::Lit(Lit::Str(s)) => Some(s.value.as_str().unwrap_or_default().to_string()),
        _ => None, // dynamic prefix arg — never guess (see module doc)
    }
}

#[cfg(test)]
mod tests {
    //! Coverage for `extract_global_prefix_marker`: the string-literal happy path (with and without a
    //! leading slash in source), the non-literal never-guess skip, and the no-call/empty-file cases.
    use super::*;

    #[test]
    fn bare_prefix_literal_yields_a_marker() {
        let src = "const app = await NestFactory.create(AppModule);\napp.setGlobalPrefix('api');\n";
        let marker = extract_global_prefix_marker("main.ts", src).expect("expected a marker");
        assert_eq!(marker.kind, "nest-global-prefix");
        assert_eq!(marker.key, "api");
        assert_eq!(marker.file, "main.ts");
        assert_eq!(marker.line, 2);
        assert!(marker.symbol.is_none());
    }

    #[test]
    fn leading_slash_literal_is_kept_verbatim() {
        let src = "app.setGlobalPrefix('/api/v1');\n";
        let marker = extract_global_prefix_marker("main.ts", src).expect("expected a marker");
        assert_eq!(marker.key, "/api/v1");
    }

    #[test]
    fn non_literal_argument_is_never_guessed() {
        let src = "app.setGlobalPrefix(cfg.prefix);\n";
        assert!(extract_global_prefix_marker("main.ts", src).is_none());
    }

    #[test]
    fn template_literal_argument_is_never_guessed() {
        let src = "app.setGlobalPrefix(`${prefix}`);\n";
        assert!(extract_global_prefix_marker("main.ts", src).is_none());
    }

    #[test]
    fn a_file_without_the_call_yields_none() {
        let src = "const app = await NestFactory.create(AppModule);\nawait app.listen(3000);\n";
        assert!(extract_global_prefix_marker("main.ts", src).is_none());
    }

    #[test]
    fn empty_file_yields_none() {
        assert!(extract_global_prefix_marker("main.ts", "").is_none());
    }

    #[test]
    fn call_nested_inside_another_expression_is_still_found() {
        let src = "void (async () => { app.setGlobalPrefix('api'); })();\n";
        let marker = extract_global_prefix_marker("main.ts", src).expect("expected a marker");
        assert_eq!(marker.key, "api");
    }
}
