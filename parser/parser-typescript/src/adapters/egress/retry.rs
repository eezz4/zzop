//! Automatic-retry recognition for egress call sites (`egress-retry-v1`) — the FE half of the
//! `cross-layer/retrying-write-no-idempotency` cross-layer wedge. A WRITE-verb HTTP call that runs under an
//! automatic retry may be replayed, so if it resolves to a real provider route the duplicate write is a
//! latent data hazard the provider can only defend against with idempotency. Two high-precision, purely
//! lexical/structural signals — no config-value parsing, no guessing:
//!
//! - **File gate (`axios-retry`)** — a file that imports `axios-retry` has wired transparent retries onto
//!   its axios instance(s), so every write AXIOS call in it is retry-exposed (the caller applies this gate
//!   only to `client == "axios"` consumes — `axios-retry` cannot wrap a `fetch()` in the same file).
//!   Mirrors the react-query / Angular per-file import gate. The bare import specifier is distinctive enough
//!   to gate on directly (a file that imports the package but never calls `axiosRetry(...)` is a rarity we
//!   accept — warning severity).
//! - **Wrapper enclosure** — a call lexically nested inside a retry-wrapper call (`pRetry(() => …)`,
//!   `backOff(…)`, `retryAsync(…)`, `asyncRetry(…)`). Only DISTINCTIVE wrapper idents are recognized; the
//!   bare `retry(` that `async-retry` is often imported as is deliberately excluded (too collision-prone
//!   with unrelated user functions) — recall traded for precision, expandable later.
//!
//! Read verbs are never tagged (replaying a GET is safe); the tag is a risk marker, set only on writes.
//! TS-only producer — see the projection-contract language-coverage matrix.

use swc_core::ecma::ast::{CallExpr, Callee, Expr, Module, ModuleDecl, ModuleItem};

/// Distinctive retry-wrapper callee identifiers (see module doc for why bare `retry` is excluded).
const RETRY_WRAPPERS: [&str; 4] = ["pRetry", "backOff", "retryAsync", "asyncRetry"];

/// True when the file imports `axios-retry` — the per-file gate that marks its axios write calls as
/// retry-exposed (mirrors [`imports_react_query`](super::react_query::imports_react_query)).
pub(super) fn file_wires_retry(module: &Module) -> bool {
    module.body.iter().any(|item| {
        matches!(
            item,
            ModuleItem::ModuleDecl(ModuleDecl::Import(imp))
                if matches!(imp.src.value.as_str(), Some("axios-retry"))
        )
    })
}

/// True when this call's callee is a distinctive retry wrapper (`pRetry(...)`, `backOff(...)`, …); its
/// subtree's egress calls are then retry-exposed. Only a bare-identifier callee counts.
pub(super) fn is_retry_wrapper_call(call: &CallExpr) -> bool {
    let Callee::Expr(callee) = &call.callee else {
        return false;
    };
    let Expr::Ident(id) = &**callee else {
        return false;
    };
    RETRY_WRAPPERS.contains(&&*id.sym)
}

/// True for the non-idempotent HTTP verbs whose replay is a hazard (POST/PUT/PATCH/DELETE). Read verbs
/// (GET/HEAD/OPTIONS) return false — retrying them is safe, so they are never tagged retry-configured.
///
/// This set MUST equal `zzop_rules_http`'s `WRITE_HTTP_METHODS` and `zzop_rules_cross_layer`'s
/// `is_write_method` write set — the crate boundary forbids sharing the symbol (parsers do not depend
/// on rule crates), so the pairing is pinned by the exact-set test below (T2; policy-value inventory
/// row). Drift is fail-safe by construction (the consuming rule re-derives the verb defensively) but
/// would silently narrow/widen which call sites carry `retry_configured`.
pub(super) fn is_write_verb(method: &str) -> bool {
    matches!(
        method.to_ascii_uppercase().as_str(),
        "POST" | "PUT" | "PATCH" | "DELETE"
    )
}

#[cfg(test)]
mod write_verb_pin {
    use super::is_write_verb;

    /// T2 exact-set pin for the cross-crate write-verb pairing described on `is_write_verb`'s doc.
    #[test]
    fn is_write_verb_is_pinned_to_the_exact_write_set() {
        for verb in ["POST", "PUT", "PATCH", "DELETE", "post", "delete"] {
            assert!(is_write_verb(verb), "{verb} must be a write verb");
        }
        for verb in ["GET", "HEAD", "OPTIONS", "get", "TRACE", ""] {
            assert!(!is_write_verb(verb), "{verb} must not be a write verb");
        }
    }
}
