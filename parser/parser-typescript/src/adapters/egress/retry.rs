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
//!   `backOff(…)`, `retryAsync(…)`, `asyncRetry(…)`). Only DISTINCTIVE wrapper idents are recognized by
//!   NAME alone; the bare `retry(` that `async-retry` is often imported as stays excluded there (too
//!   collision-prone with unrelated user functions).
//! - **Wrapper binding (`retry-wrapper-binding-v1`)** — the recall the name list traded away, bought back
//!   with evidence instead of a wider vocabulary: the local name a file binds to the DEFAULT export of a
//!   retry-wrapper package (`import retry from 'async-retry'`, `import withRetry from 'p-retry'`) is a
//!   retry wrapper IN THAT FILE. The import is the proof, so `retry(…)` in a file that never imports one
//!   of those packages is still not a wrapper — the collision the name list was avoiding cannot occur.
//!   Deliberately DEFAULT-import only: a retry package's named exports are error types
//!   (`p-retry`'s `AbortError`), not wrappers. `require()` binding and namespace imports are not read —
//!   accepted silence, the same direction every gate in this module errs.
//!
//! Read verbs are never tagged (replaying a GET is safe); the tag is a risk marker, set only on writes.
//! TS-only producer — see the projection-contract language-coverage matrix.

use std::collections::HashSet;

use swc_core::ecma::ast::{
    CallExpr, Callee, Expr, ImportSpecifier, Module, ModuleDecl, ModuleItem,
};

/// Distinctive retry-wrapper callee identifiers (see module doc for why bare `retry` is excluded) — the
/// DEFAULT for `vocabulary.retryWrappers`. What a project names its retry helper is its own convention,
/// so the effective list is declared; this is only what a run assumes when nothing is declared.
pub const RETRY_WRAPPERS: [&str; 4] = ["pRetry", "backOff", "retryAsync", "asyncRetry"];

/// POLICY VOCABULARY — packages whose DEFAULT export IS the retry wrapper function, so a file's local
/// name for it is a wrapper callee in that file (`retry-wrapper-binding-v1`). Each entry exists to wrap a
/// caller-supplied function in an automatic retry loop and does nothing else; `exponential-backoff`'s
/// `backOff` is absent because it is a NAMED export already covered by [`RETRY_WRAPPERS`], and
/// `axios-retry`/`retry-axios` are absent because they patch a client instance rather than wrap a call —
/// that is the file gate's job, not this one's.
const RETRY_WRAPPER_PACKAGES: [&str; 3] = ["p-retry", "async-retry", "promise-retry"];

/// The local names this file binds to a [`RETRY_WRAPPER_PACKAGES`] default export. Empty for virtually
/// every file; computed once per file beside [`file_wires_retry`].
pub(super) fn retry_wrapper_bindings(module: &Module) -> HashSet<String> {
    let mut out = HashSet::new();
    for item in &module.body {
        let ModuleItem::ModuleDecl(ModuleDecl::Import(imp)) = item else {
            continue;
        };
        let Some(src) = imp.src.value.as_str() else {
            continue;
        };
        if !RETRY_WRAPPER_PACKAGES.contains(&src) {
            continue;
        }
        for s in &imp.specifiers {
            if let ImportSpecifier::Default(d) = s {
                out.insert(d.local.sym.to_string());
            }
        }
    }
    out
}

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

/// True when this call's callee is a distinctive retry wrapper (`pRetry(...)`, `backOff(...)`, …) OR a
/// name THIS file bound to a retry package's default export (`bindings`, `retry-wrapper-binding-v1`); its
/// subtree's egress calls are then retry-exposed. Only a bare-identifier callee counts.
pub(super) fn is_retry_wrapper_call(
    call: &CallExpr,
    bindings: &HashSet<String>,
    retry_wrappers: &[&str],
) -> bool {
    let Callee::Expr(callee) = &call.callee else {
        return false;
    };
    let Expr::Ident(id) = &**callee else {
        return false;
    };
    retry_wrappers.contains(&&*id.sym) || bindings.contains(id.sym.as_ref())
}

/// True for the non-idempotent HTTP verbs whose replay is a hazard (POST/PUT/PATCH/DELETE). Read verbs
/// (GET/HEAD/OPTIONS) return false — retrying them is safe, so they are never tagged retry-configured.
///
/// This set MUST equal `zzop_rules_http`'s `WRITE_HTTP_METHODS` and `zzop_rules_cross_layer`'s
/// `is_write_method` write set — the crate boundary forbids sharing the symbol (parsers do not depend
/// on rule crates), so the pairing is pinned in `crates/engine/tests/rule_contracts/policy_pins.rs`,
/// the one place that can read BOTH shipped sides (T2). Drift is fail-safe by construction (the
/// consuming rule re-derives the verb defensively) but would silently narrow/widen which call sites
/// carry `retry_configured`.
///
/// The set is a named `pub const` for exactly that reason. Until 2026-07-28 it was an inline
/// `matches!` arm whose doc pointed at "the exact-set test below" — and that test compared it to
/// verbs spelled AGAIN inside the test, so it was a third copy rather than a pin, and could not have
/// failed when the rule's set changed. See `WRITE_HTTP_METHODS`'s doc for the measurement.
pub const RETRY_WRITE_VERBS: [&str; 4] = ["POST", "PUT", "PATCH", "DELETE"];

pub(super) fn is_write_verb(method: &str) -> bool {
    RETRY_WRITE_VERBS.contains(&method.to_ascii_uppercase().as_str())
}

#[cfg(test)]
mod write_verb_pin {
    use super::is_write_verb;

    /// Local behaviour of the predicate — case folding and the read verbs it must reject. This is NOT
    /// the cross-crate pin (it cannot be: the rule's set is not visible from here); that lives in
    /// `crates/engine/tests/rule_contracts/policy_pins.rs`. It was named and documented as the T2 pin
    /// until 2026-07-28, which is how a set that no test compared went four crates wide.
    #[test]
    fn is_write_verb_folds_case_and_rejects_read_verbs() {
        for verb in ["POST", "PUT", "PATCH", "DELETE", "post", "delete"] {
            assert!(is_write_verb(verb), "{verb} must be a write verb");
        }
        for verb in ["GET", "HEAD", "OPTIONS", "get", "TRACE", ""] {
            assert!(!is_write_verb(verb), "{verb} must not be a write verb");
        }
    }
}

#[cfg(test)]
mod tests {
    //! `retry-wrapper-binding-v1` coverage. The file gate and the distinctive-ident wrappers are
    //! covered end-to-end in `crates/engine/tests/analyze_cross_layer_retry_write.rs`.
    use crate::adapters::egress::{extract_http_egress, files};

    fn tags(src: &str) -> Vec<Option<bool>> {
        extract_http_egress(&files(&[("a.ts", src)]))
            .iter()
            .map(|c| c.retry_configured)
            .collect()
    }

    #[test]
    fn an_async_retry_default_import_makes_the_bare_call_a_wrapper() {
        assert_eq!(
            tags("import retry from 'async-retry';\nretry(async () => { await axios.post('/orders', body); });"),
            vec![Some(true)]
        );
    }

    #[test]
    fn a_renamed_p_retry_default_import_is_recognized() {
        assert_eq!(
            tags("import withRetry from 'p-retry';\nwithRetry(() => axios.put('/x', b));"),
            vec![Some(true)]
        );
    }

    #[test]
    fn a_bare_retry_call_without_the_import_is_not_a_wrapper() {
        // The precision the module doc traded recall for: an unrelated user function named `retry`.
        assert_eq!(
            tags("retry(async () => { await axios.post('/orders', body); });"),
            vec![None]
        );
    }

    #[test]
    fn a_named_import_from_a_retry_package_is_not_a_wrapper() {
        // Only the DEFAULT import is the wrapper; `p-retry`'s named exports are error types.
        assert_eq!(
            tags("import { AbortError } from 'p-retry';\nAbortError(() => axios.post('/x', b));"),
            vec![None]
        );
    }

    #[test]
    fn a_read_verb_under_a_bound_retry_wrapper_is_never_tagged() {
        assert_eq!(
            tags("import retry from 'async-retry';\nretry(() => axios.get('/x'));"),
            vec![None]
        );
    }
}
