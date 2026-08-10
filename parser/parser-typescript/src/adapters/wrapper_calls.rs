//! FE wrapper-function CONSUME resolution, stage 1 — the provide-side is `router_mounts.rs`/
//! `trpc_router.rs`'s sibling for the CONSUME half of the cross-layer IO join: a frontend codebase
//! often wraps its real HTTP-call sink (`fetch`/`axios`/`ky`) behind a project-local helper instead of
//! calling the sink directly at every use site — e.g. `makeRestApiRequest(context, method, endpoint,
//! data?)` forwarding to a SIBLING `request()` that calls `axios.request(...)`. Without this
//! projection those call sites' HTTP consume would either not exist (the plain-egress extractor in
//! `egress.rs` only recognizes direct sink calls) or anchor back to the wrapper's own definition site,
//! not the call site. This module only projects one file's local facts; the actual re-anchoring
//! (resolving a call's callee to a def fragment, possibly cross-file via `specifier`) is the engine's
//! assemble-time join — see `zzop_core::fragments`'s `WrapperDefFragment`/`WrapperCallFragment` doc.
//!
//! ## Def recognizer (`WrapperDefFragment`)
//! A top-level function/const-arrow (exported OR file-private — a wrapper often sits un-exported below
//! a `// --- private ---` line, called only from same-file callers) qualifies as a wrapper def when
//! ALL of:
//! - a parameter's name, case-insensitively, is or ENDS IN `endpoint`/`path`/`url` (e.g.
//!   `apiEndpoint`) -> its index becomes `path_param`. No type annotation required — same
//!   "name is the signal" tradeoff `router_mounts.rs`/`controller_decorators.rs` document. FALLBACK
//!   for a builtin-`fetch` sink: absent a path-like NAME, the param passed VERBATIM as a bare
//!   identifier first arg to `fetch(...)` (`fetch(p)`/`fetch(p, {...})`) is the path param — a
//!   composite first arg (`fetch(base + p)`) is left to the name signal, never guessed;
//! - a `method` (or `: Method`-typed) parameter -> `method_param`, OR — absent that — the reachable
//!   sink body contains exactly ONE distinct `method: 'VERB'` literal -> `fixed_method`. For a
//!   builtin-`fetch` sink with NO `method` key at all, bare `fetch(url)` defaults to a fixed GET;
//!   a present-but-dynamic `method` (`fetch(url, { method: verb })`) or ambiguous/zero-for-non-fetch
//!   verbs disqualify the function (never guess a computed verb);
//! - its body reaches a sink call `fetch(`/`axios.`/`axios(`/`ky.`, directly or one hop through a
//!   LOCAL top-level helper (declared or const arrow/fn, exported or not) whose body contains the
//!   sink. Exactly one hop — a helper forwarding to ANOTHER helper is not walked further.
//!
//! The sink check is a lexical substring test (`SourceMap::span_to_snippet`), not structural — cheap,
//! with precision carried by the param-signature gate above.
//!
//! ## Call recognizer (`WrapperCallFragment`)
//! Two callee shapes are candidates: a PLAIN identifier, and a member call whose receiver is a
//! NAMESPACE import of this file (`import * as api from './client'; api.get('/x')` — the member name
//! is the wrapper name, the receiver supplies the specifier). The second joined on 2026-08-08: it is
//! the dominant shape in the wild — 19 of 19 call sites on `corpus/oss/fe-svelte`, zero named imports
//! — so that whole file read as callless. A member call on anything else stays out, including a plain
//! local object (`svc.get('/a')`): treating any `x.get(...)` as a wrapper call would sweep in every
//! helper that happens to expose a `get`.
//!
//! A candidate qualifies when one of its first 6 args is an uppercase HTTP verb literal or a
//! string/template starting with `/` (volume guard, else every `helper(a, b)` call would qualify).
//! ⚠ That `/`-leading requirement means a BASE-RELATIVE argument (`api.get('articles')`) is not a
//! candidate, and relaxing it here would not help: `compose::wrapper_consumes` independently requires
//! a `/`-leading path, and the consumes it emits carry `client: None`, so `compose::client_base`'s
//! base prefix — which applies per client tag — can never reach them. Making base-relative wrapper
//! args resolve is therefore a DESIGN change across three layers, not a lexical widening here, and
//! doing it as one would mint `/articles` for a call whose real path is `/api/articles`.
//!
//! The call side does NOT resolve whether `callee` names a known def — defs often live in a different
//! file; the assemble-time join filters candidates down to real invocations.
//!
//! Each of the first 6 args is captured positionally: string literal verbatim, template literal with
//! `${...}` replaced by `{}` (same transform as `egress.rs`'s `resolve_url`), anything else `None` —
//! never guessed. `specifier` comes from the file's import map when `callee` is an imported binding;
//! `None` means local-or-unresolved (assemble only resolves same-file when `specifier` is `None`).
//!
//! Deterministic output: defs and calls in source (AST-walk) order; no matches -> two empty vecs.

use swc_core::common::Spanned;
use swc_core::ecma::ast::{BlockStmtOrExpr, Decl, Expr, ModuleDecl, ModuleItem, Pat, Stmt};
use swc_core::ecma::visit::VisitWith;
use zzop_core::{WrapperCallFragment, WrapperDefFragment};

mod calls;
mod defs;
#[cfg(test)]
mod tests;

use calls::CallCollector;
use defs::{classify_def, collect_absolute_url_consts, collect_top_level_functions};

/// Extract one file's wrapper-def and wrapper-call fragments — see module doc for the recognizer spec.
pub fn extract_wrapper_fragments(
    rel: &str,
    text: &str,
) -> (Vec<WrapperDefFragment>, Vec<WrapperCallFragment>) {
    let Some((cm, module)) = crate::parse_with_cm(rel, text) else {
        return (Vec::new(), Vec::new());
    };
    let imports = crate::parse_imports(rel, text);
    let local_fns = collect_top_level_functions(&module, &cm);
    let url_consts = collect_absolute_url_consts(&module, &cm);

    let mut defs = Vec::new();
    for item in &module.body {
        // Both `export function request(...)` and a file-private `function request(...)` (a
        // `Stmt::Decl`) qualify: a wrapper often lives un-exported below a `// --- private ---` line,
        // called only by same-file callers. This walks the same top-level item set `collect_top_level_
        // functions` (the sink one-hop scan) already does, so def- and sink-collection stay in sync.
        let decl = match item {
            ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(export)) => &export.decl,
            ModuleItem::Stmt(Stmt::Decl(d)) => d,
            _ => continue,
        };
        match decl {
            Decl::Fn(f) => {
                let pats: Vec<&Pat> = f.function.params.iter().map(|p| &p.pat).collect();
                let body_span = f.function.body.as_ref().map(|b| b.span);
                if let Some(d) =
                    classify_def(&f.ident.sym, &pats, body_span, &cm, &local_fns, &url_consts)
                {
                    defs.push(d);
                }
            }
            Decl::Var(v) => {
                for d in &v.decls {
                    let Pat::Ident(bi) = &d.name else { continue };
                    let Some(Expr::Arrow(arrow)) = d.init.as_deref() else {
                        continue;
                    };
                    let pats: Vec<&Pat> = arrow.params.iter().collect();
                    let body_span = Some(match &*arrow.body {
                        BlockStmtOrExpr::BlockStmt(b) => b.span,
                        BlockStmtOrExpr::Expr(e) => e.span(),
                    });
                    if let Some(frag) =
                        classify_def(&bi.id.sym, &pats, body_span, &cm, &local_fns, &url_consts)
                    {
                        defs.push(frag);
                    }
                }
            }
            _ => {}
        }
    }

    let mut calls = Vec::new();
    let mut collector = CallCollector {
        cm: &cm,
        imports: &imports,
        out: &mut calls,
    };
    module.visit_with(&mut collector);

    (defs, calls)
}
