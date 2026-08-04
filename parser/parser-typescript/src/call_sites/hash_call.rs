//! The `hash-call` half of the TypeScript/JavaScript call-site producer — Node `crypto` digest
//! construction, and the one place in this crate that fills [`zzop_core::CallSite::algorithm`].
//!
//! # What is recognized
//! `crypto.createHash(...)` / `crypto.createHmac(...)`, reached through this file's OWN module-level
//! `crypto` / `node:crypto` bindings, resolved exactly the way [`super::process_exec`] resolves
//! `child_process` (and for the same reason: `createHash` is not a global — the bare spelling is
//! whatever the file bound it to). Recognized binding shapes are that module's, verbatim: a named
//! import with or without an alias, a default/namespace import, and both `require` forms. `callee` is
//! the spelling AS WRITTEN (`createHash`, `crypto.createHash`, an alias).
//!
//! # `algorithm` — the channel's one argument-derived fact, and its never-guess line
//! `Some` only when the FIRST argument is a plain string LITERAL, carried verbatim
//! (`createHash("md5")` → `Some("md5")`, `createHash("MD5")` → `Some("MD5")` — case is the author's,
//! and a consuming rule owns its own case-insensitivity). Everything else is `None`, with no
//! inference attempted and none possible to add later without breaking the contract:
//! - an identifier or member (`createHash(algo)`, `createHash(cfg.hash)`) — the value is a dataflow
//!   question this channel does not answer;
//! - a template literal, even a constant one (`` createHash(`md5`) ``) — a template is an expression,
//!   and reading the cooked value of a no-substitution one would be the first step onto exactly the
//!   slope the no-argument-capture rule exists to prevent;
//! - a concatenation, a call, a conditional.
//!
//! A `None` site is still a SITE: the digest construction happened, and a rule that does not filter on
//! algorithm (or one asking "is any digest built here") can still read it. What `None` costs is the
//! `algorithm_pattern` filter, which never matches it — silence, never an approximation.
//!
//! # Deliberate silences
//! - **Third-party digest packages** (`js-md5`, `hash.js`, `crypto-js`) are NOT this family in v1:
//!   not the platform's API, each with its own surface, and the consuming rules' claims are stated
//!   about Node's `crypto`. Same boundary every sibling producer draws for wrappers.
//! - **`crypto.subtle.digest(...)`** (WebCrypto) — a different API on a different object, whose
//!   algorithm argument is an object as often as a string; out of v1 rather than half-supported.
//! - **`createCipheriv`/`createDecipheriv`** — cipher construction is not a digest. The `hash-call`
//!   family claims digests only; a cipher family would be its own kind with its own consuming rule.
//! - A **re-exported or indirected binding**, and a **member call on a non-binding receiver**
//!   (`this.crypto.createHash(...)`) — the callee does not resolve, so there is no site.

use std::collections::HashMap;

use swc_core::ecma::ast::{Expr, ExprOrSpread, Lit};

use super::process_exec::Binding;

/// The `crypto` functions that construct a DIGEST — the platform's own names (Node `crypto`), not
/// names a project picks. `createCipheriv` and friends are deliberately absent (module doc).
pub const CRYPTO_HASH_METHODS: &[&str] = &["createHash", "createHmac"];

/// The two module specifiers that ARE Node's crypto module — the bare and `node:`-prefixed spellings
/// of one module, an equivalence Node's own resolution fixes.
pub const CRYPTO_SPECIFIERS: &[&str] = &["crypto", "node:crypto"];

/// The callee spelling to record for this call, or `None` when it is not a resolved digest binding.
/// Returns the spelling AS WRITTEN, so an alias survives into the channel (module doc).
pub(super) fn hash_callee(callee: &Expr, bindings: &HashMap<String, Binding>) -> Option<String> {
    super::process_exec::resolved_callee(callee, bindings, CRYPTO_HASH_METHODS)
}

/// The algorithm this call SPELLS, or `None` — module doc's never-guess list. Reads the first
/// argument and accepts a plain string literal and nothing else.
pub(super) fn spelled_algorithm(args: &[ExprOrSpread]) -> Option<String> {
    let first = args.first()?;
    if first.spread.is_some() {
        return None; // `createHash(...args)` names no algorithm at this site.
    }
    match &*first.expr {
        Expr::Lit(Lit::Str(s)) => Some(s.value.as_str().unwrap_or_default().to_string()),
        _ => None,
    }
}
