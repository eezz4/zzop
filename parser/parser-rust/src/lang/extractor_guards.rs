//! Handler-signature TYPE evidence, projected as `RawCall` edges — the Rust half of the guard-vocabulary
//! requirement that makes lifting `.rs` into `CALL_GRAPH_COVERED_EXTENSIONS` honest.
//!
//! # The measurement that decided this module's shape
//! D8's rule is that a language's call graph and its guard evidence ship in the same batch, because
//! covering calls WITHOUT guards turns every guarded route into a false positive. So D18's first question
//! was "what is a Rust guard?", and the answer was not obvious a priori — Rust web frameworks have not
//! converged on decorators the way Spring and FastAPI have.
//!
//! `corpus/oss/be-axum` answers it by measurement. All five of its mutating routes are guarded, and every
//! one of them is guarded the same way:
//!
//! ```text
//! async fn create_article(
//!     auth_user: AuthUser,          // <- the guard
//!     ctx: Extension<ApiContext>,
//!     ...
//! ```
//!
//! Zero of them call a guard from the body. **Rust's guard evidence is a TYPE in the handler's parameter
//! list, not a call the handler makes** — because that is what the type system is for: an extractor
//! (`axum::extract::FromRequestParts`, Rocket's request guards, actix's `FromRequest`) runs BEFORE the
//! handler body and rejects the request outright, exactly like a decorator.
//!
//! # Why a `RawCall` rather than a new side-channel
//! Spring/FastAPI/DRF guard evidence rides a separate `(file, line)` "decorator guarded" set, because the
//! evidence sits at the ROUTE REGISTRATION and the BFS could never see it. Rust's sits somewhere the BFS
//! already goes — on the handler symbol itself — so projecting the parameter type as an outgoing edge
//! needs no new channel at all: `zzop_core::callgraph::build_symbol_graph` resolves the type name through
//! this file's own `ImportMap`, and the existing name vocabulary (`DEFAULT_AUTH_GUARD_PATTERN`) matches
//! `AuthUser` on its tail arm with nothing added to it.
//!
//! It is a PROJECTION, and it is stated as one rather than pretending to be a call site: `parse_calls`
//! stays call-sites-only, and the engine merges the two producers explicitly.
//!
//! # Blast radius, checked rather than assumed
//! The graph is shared with `unsafe-read-endpoint` and `non-idempotent-write`, whose BFS predicates read
//! `SourceSymbol::write_sites` — a fact NO Rust parser fills (`http_scan::WRITE_SITE_COVERED_EXTENSIONS`).
//! Extra edges out of a Rust handler are therefore structurally inert for those two rules; they can only
//! ever reach the auth-guard vocabulary check. That is why this module emits EVERY parameter type rather
//! than pre-filtering to auth-shaped names: which names prove auth is the rule's judgment, declared in
//! `vocabulary.authGuardPattern`, not this producer's.
//!
//! # The one veto, and why it is declarable rather than built in
//! An OPTIONAL extractor is not a gate. `be-axum` ships `MaybeAuthUser(pub Option<AuthUser>)` alongside
//! `AuthUser`, and a route taking it admits anonymous callers — yet its name contains `auth` and would
//! clear the route. What a project calls its optional extractor is a CONVENTION (`MaybeAuthUser`,
//! `OptionalUser`, `AuthUserOrAnon`), so the veto list is `vocabulary.rustOptionalExtractorPrefixes`
//! rather than a constant here — the same shape, and for the same reason, as Python's
//! `pythonGuardAnonymousVetoSubstrings`.
//!
//! # Declared boundaries
//! - **Router-level auth is invisible here.** `.route_layer(middleware::from_fn(require_auth))` and
//!   actix's `.wrap(HttpAuthentication::bearer(..))` guard a route without touching the handler's
//!   signature. This is the SAME blind spot the rule's own doc calls its "precision limit" for
//!   middleware, and the same remedy applies (inject `AUTH_GUARDED_ATTR` from an adapter). It is worth
//!   naming twice because in Rust it is a mainstream idiom rather than an edge case — see the engine's
//!   `framework_silence::rust_router_layer` tripwire, which discloses it per run.
//! - **A guard behind a type ALIAS is not followed.** `type Admin = AuthUser;` emits `Admin`; whether
//!   that resolves is `build_symbol_graph`'s business, and an unresolved name is dropped, never guessed.
//! - **Generic arguments are emitted too**, one level deep: `Extension<CurrentUser>` yields both
//!   `Extension` and `CurrentUser`, since either spelling is the real one in different codebases.

use syn::{FnArg, ImplItem, Item, Type};

use zzop_core::callgraph::RawCall;

use super::symbols::type_leaf_name;

/// The declared convention this producer needs — see the module doc's veto section. Mirrors
/// `zzop_parser_python_3::PythonGuardVocab`'s borrow-only shape so the engine can hand it a view of the
/// resolved config without allocating.
pub struct RustGuardVocab<'a> {
    /// Name prefixes marking an extractor that ADMITS an anonymous caller (`MaybeAuthUser`), matched
    /// case-insensitively against the type's leaf name.
    pub optional_extractor_prefixes: &'a [&'a str],
}

/// Built-in default for [`RustGuardVocab::optional_extractor_prefixes`] — the fallback when
/// `vocabulary.rustOptionalExtractorPrefixes` is not declared.
pub const RUST_OPTIONAL_EXTRACTOR_PREFIXES: &[&str] = &["maybe", "optional"];

/// One `RawCall` per handler-parameter type, attributed to the enclosing function's symbol id (the same
/// id `lang::calls` and `lang::symbols` mint, or every edge dangles). Empty when `syn` cannot parse.
pub fn parse_extractor_guards(rel: &str, text: &str, vocab: &RustGuardVocab) -> Vec<RawCall> {
    let Ok(file) = syn::parse_file(text) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in &file.items {
        walk_item(rel, item, vocab, &mut out);
    }
    out
}

fn walk_item(rel: &str, item: &Item, vocab: &RustGuardVocab, out: &mut Vec<RawCall>) {
    match item {
        Item::Fn(f) => emit_signature(&format!("{rel}#{}", f.sig.ident), &f.sig, vocab, out),
        Item::Impl(imp) => {
            let Some(type_name) = type_leaf_name(&imp.self_ty) else {
                return;
            };
            for it in &imp.items {
                if let ImplItem::Fn(f) = it {
                    let from = format!("{rel}#{type_name}.{}", f.sig.ident);
                    emit_signature(&from, &f.sig, vocab, out);
                }
            }
        }
        // Same file-scoped, v1-flat symbol model `lang::calls` uses: an inline `mod` block's items are
        // still THIS file's symbols.
        Item::Mod(m) => {
            if let Some((_, items)) = &m.content {
                for inner in items {
                    walk_item(rel, inner, vocab, out);
                }
            }
        }
        _ => {}
    }
}

fn emit_signature(
    from: &str,
    sig: &syn::Signature,
    vocab: &RustGuardVocab,
    out: &mut Vec<RawCall>,
) {
    // The function's OWN line, not the parameter's: an extractor runs as part of entering this function,
    // and anchoring every edge on one line keeps a finding's evidence pointing at the handler.
    let line = crate::line_of(&sig.ident);
    for arg in &sig.inputs {
        let FnArg::Typed(t) = arg else {
            continue; // `self` carries no extractor
        };
        for name in type_names(&t.ty) {
            if is_optional_extractor(&name, vocab) {
                continue;
            }
            out.push(RawCall {
                from_symbol: from.to_string(),
                callee_name: name,
                line,
                // The type IS the callee; there is no receiver to name.
                receiver_type: None,
                is_heritage: false,
            });
        }
    }
}

fn is_optional_extractor(name: &str, vocab: &RustGuardVocab) -> bool {
    let lower = name.to_ascii_lowercase();
    vocab
        .optional_extractor_prefixes
        .iter()
        .any(|p| lower.starts_with(&p.to_ascii_lowercase()))
}

/// The type's leaf name, plus the leaf name of each of its generic arguments (one level) — see the
/// module doc's "Generic arguments" boundary. A reference (`&AuthUser`) is unwrapped first.
fn type_names(ty: &Type) -> Vec<String> {
    let ty = match ty {
        Type::Reference(r) => &*r.elem,
        other => other,
    };
    let Type::Path(tp) = ty else {
        return Vec::new();
    };
    let Some(last) = tp.path.segments.last() else {
        return Vec::new();
    };
    let mut out = vec![last.ident.to_string()];
    if let syn::PathArguments::AngleBracketed(args) = &last.arguments {
        for a in &args.args {
            if let syn::GenericArgument::Type(inner) = a {
                if let Some(n) = type_leaf_name(inner) {
                    out.push(n);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests;
