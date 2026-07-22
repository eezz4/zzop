//! Per-file router-mount fragments — the provide-side sibling of `trpc_router.rs`'s
//! `RouterFragment`. A code-registered router is often split across files: verb registrations
//! live on sub-routers (`twoFactorRoute.post('/setup', h)`), sub-routers are mounted with a
//! prefix (`.route('/two-factor', twoFactorRoute)`), and the app itself is mounted again
//! (`app.route('/api/auth', auth)`). The real URL only exists once every hop is joined, which no
//! per-file pass can do — so each file projects a fragment here, and the engine composes
//! whole-tree `http` provides at assemble time (`compose_router_mount_provides`).
//!
//! The fragment SHAPE (`Verb`, `Mount`) is framework-agnostic; only the RECOGNIZER is
//! framework-specific. Hono (`new Hono()`, `.route()`) and Express (`express()` /
//! `express.Router()` / an import-gated bare `Router()`, `.use()` as mount) are independent
//! vocabularies feeding the same types through the same compose pass — a new framework costs
//! vocabulary only. Name-dependence stays confined to the recognizer gate, same precision
//! discipline as `trpc_router.rs`'s factory gate: `.get(...)` alone is far too common (axios, Map,
//! cache clients) to act on without a structural router signal — likewise a bare `Router()` call
//! is only trusted once this file's `ImportMap` confirms the callee resolves to the imported name
//! `Router` from module specifier `'express'` (see `is_express_router_import_call`); this is what
//! lets `import { Router } from 'express'; const router = Router();` — the canonical
//! controller-file idiom in Express codebases (e.g. gothinkster's node-express-realworld,
//! dogfood round 9) — join the same Express vocabulary as `express.Router()`.
//!
//! Recognition is swc-AST-based, so chained builders — including ones spanning several router
//! hops in a large real-world monorepo — are first-class, unlike a line-anchored regex.
//!
//! ## Implementation notes
//! - Two passes: pass 1 (`ReceiverCollector`, in `chain`) finds every receiver identifier — bound
//!   to `new Hono(...)` (bare or chain root), typed `: Hono`, an import-gated `Router()` call, or
//!   configured by name. Pass 2 (`FragmentBuilder`, in `build`) walks again in source order,
//!   classifying each var-decl chain, statement, and `export default` chain onto the right
//!   fragment.
//! - `walk_chain` recurses a call chain down to its root; recursing before pushing the current
//!   call naturally yields calls in source order. It takes the file's `ImportMap` so the
//!   import-gated `Router()` chain root (`is_express_router_import_call`) can be recognized at
//!   any depth — bare receiver, chain root (`Router().use(a).use(b)`), or `export default`.
//! - `Verb::line` uses the `.get`/`.post`/... identifier's own span, not the call's: swc gives a
//!   chained call the same start position as the chain's root, which would misreport the line on
//!   a multi-line chain otherwise.
//! - Verb entries carry two independent producer-judged `attr_keys`: a middleware guard-name
//!   judgment (`guard.rs`, `AUTH_GUARDED_ATTR_KEY`) and an inline-handler idempotency-key-read
//!   judgment (`idempotency.rs`, `IDEMPOTENCY_GUARDED_ATTR_KEY`) — both can be present on the
//!   same entry.

use std::collections::{HashMap, HashSet};

use swc_core::ecma::visit::VisitWith;
use zzop_core::RouterMountFragment;

mod build;
mod chain;
mod guard;
#[cfg(test)]
mod guard_tests;
mod idempotency;
#[cfg(test)]
mod idempotency_tests;
#[cfg(test)]
mod tests_express;
#[cfg(test)]
mod tests_hono;
mod use_classify;

use build::FragmentBuilder;
use chain::ReceiverCollector;

/// Extract one file's router-mount fragments. Pure; parses `text` with the crate's swc pipeline.
/// Returns an empty vec for files with no recognized router.
///
/// Recognizer spec (Hono vocabulary + Express vocabulary + configured names):
/// - **Receivers**: an identifier bound to `new Hono(...)` (bare or chain root, any generics); a
///   function parameter typed `: Hono`; an identifier bound to `express()`, `express.Router()`,
///   or a bare `Router()` call whose callee resolves via this file's `ImportMap` to the imported
///   name `Router` from module specifier `'express'` (aliases like `import { Router as R } from
///   'express'` included; a `Router()` call with no such import is NOT a receiver — never
///   bare-name-matched) — all tracked as EXPRESS vocabulary, which matters for the `.use` mount
///   rules below; any identifier in `router_names` (config allowlist, vocabulary-agnostic); or
///   `export default new Hono()...` / `export default express()...` / `export default Router()...`
///   chains with no binding → fragment name `"default"`.
/// - **Entries** collected from both chained calls and separate statements (`recv.get('/a', h);`)
///   where `recv` is a receiver.
/// - `.get|post|put|patch|delete(pathLit, ...)` → `Verb` (method uppercased), requiring ≥2
///   arguments. A non-string-literal path skips just that entry. `.all`/`.on`/other members are
///   ignored; `.use` is ignored unless the receiver is Express vocabulary.
/// - `.route(prefixLit, identArg)` → `Mount` (any receiver). For an Express-vocabulary receiver,
///   `.use(prefixLit, identArg)` → `Mount` with that prefix, and `.use(identArg)` (exactly one
///   identifier argument) → `Mount` with prefix `"/"` (a prefix-less "mount at root", e.g.
///   `Router().use(subRouter)` in a `routes.ts`-style aggregation file). A non-identifier single
///   argument (`app.use(cors())`, `app.use(bodyParser.json())`, `app.use(express.static(...))`)
///   is SKIPPED, not mistaken for a mount. Non-literal prefix or a non-identifier second arg (in
///   the 2-argument form) also skips the entry. `specifier` resolves from this file's imports
///   when `identArg`'s name is an imported binding — same mechanism `.route` already uses.
/// - A receiver with zero surviving entries produces no fragment. Output order: fragments in
///   first-appearance order, entries in source order.
pub fn extract_router_mount_fragments(
    rel: &str,
    text: &str,
    router_names: &[&str],
) -> Vec<RouterMountFragment> {
    let Some((cm, module)) = crate::parse_with_cm(rel, text) else {
        return Vec::new();
    };
    let imports = crate::parse_imports(rel, text);

    let mut receivers: HashSet<String> = router_names.iter().map(|s| s.to_string()).collect();
    let mut rc = ReceiverCollector {
        names: HashSet::new(),
        express_names: HashSet::new(),
        imports: &imports,
    };
    module.visit_with(&mut rc);
    receivers.extend(rc.names.iter().cloned());
    let express_receivers = rc.express_names;

    let mut builder = FragmentBuilder {
        cm: &cm,
        imports: &imports,
        receivers: &receivers,
        express_receivers: &express_receivers,
        fragments: Vec::new(),
        index: HashMap::new(),
    };
    module.visit_with(&mut builder);
    builder.fragments
}
