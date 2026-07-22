//! swc AST scan for Next.js `pages/api` default-export handlers: `export default <expr>` is
//! invisible to `parse_symbols` (which only surfaces `ExportDefaultDecl`), and one handler serves
//! every HTTP method via `req.method` `if`/`switch` checks or a `defaultHandler({ GET: …, POST: … })` map.
//! `zzop-engine`'s file-convention route composition calls this scan to learn whether a candidate
//! file default-exports a handler and which methods its body names.
//!
//! Verb signals are HANDLER-BODY-SCOPED, not whole-module: the scan first resolves the default
//! export down to its underlying function-like value — an inline `function`/arrow expression, or
//! (for `export default <ident>` / `export { <ident> as default }`) a same-file top-level
//! `function <ident>(…) {…}` or `const <ident> = (…) => …` binding — and walks ONLY that value's
//! body (including nested blocks/closures inside it). A `req.method`-shaped comparison or switch
//! anywhere ELSE in the file (an unrelated helper above the handler, a sibling function) contributes
//! nothing. This closes the false-positive class a whole-module walk still has: the `switch`
//! discriminant and `defaultHandler(…)` call-argument signals were already exact-node (brace-scoped
//! by construction to their own switch/call), but a bare `req.method === 'X'` comparison used to be
//! collected from anywhere in the file, not just the actual handler.
//!
//! The comparison's request-object identifier is the handler's ACTUAL first parameter name — never
//! hardcoded `req`/`request` — resolved once per file, so `(request, res) => …`, `(r, res) => …`, or
//! any other binding name is recognized exactly like `(req, res) => …`.
//!
//! An unresolvable default export (a re-export from another module, ANY HOF-wrapped call — even one
//! whose argument is an inline arrow, e.g. `export default withAuth((req, res) => …)`, since the
//! wrapped expression resolves no first-parameter witness — or a destructured first parameter) yields
//! an honest empty `verbs` — never a guess — which the engine maps to its documented UNKNOWN-verb
//! sentinel: the route degrades from exact verbs to serve-all disclosure, the safe direction.
//!
//! Split into [`resolve`] (default-export resolution down to a handler value) and [`collector`]
//! (verb-witness collection over that handler's body); this file keeps the public API and the
//! top-level orchestration.

mod collector;
mod resolve;

/// What `scan_pages_api_handler` learned about one candidate file.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PagesApiHandlerScan {
    /// 1-based line of the first `export default …` (or `export { x as default }`). `None` means the
    /// file has no default export (e.g. a config-only file) — or does not parse.
    pub default_export_line: Option<u32>,
    /// Sorted, deduped UPPERCASE verbs the RESOLVED handler's body names — see the module doc for the
    /// resolution + witness rules. Empty means either no method narrowing was witnessed (a genuine
    /// serve-all handler) or the handler's underlying function/parameter could not be resolved.
    pub verbs: Vec<String>,
}

/// Scan one `pages/api` candidate file's text. Parses via the shared swc entry (extension-driven
/// syntax); a parse failure yields the empty scan (no default export, no verbs).
pub fn scan_pages_api_handler(rel: &str, text: &str) -> PagesApiHandlerScan {
    let Some((cm, module)) = crate::parse_with_cm(rel, text) else {
        return PagesApiHandlerScan::default();
    };
    let Some(export) = resolve::find_default_export(&cm, &module) else {
        return PagesApiHandlerScan::default();
    };
    let verbs = export
        .handler
        .body
        .map(|body| collector::collect_verbs(&body, export.handler.param_name.as_deref()))
        .unwrap_or_default();
    PagesApiHandlerScan {
        default_export_line: Some(export.line),
        verbs,
    }
}

#[cfg(test)]
mod tests;
