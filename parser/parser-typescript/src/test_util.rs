//! Shared test-only construction helpers, hoisted from the per-module `#[cfg(test)]` mods that
//! duplicated them verbatim (`binding`: `imports` / `parse` / `cjs_require`; `names`: `factory` /
//! `symbols_tests`). Compiled only under `cfg(test)` — see the crate-root module declaration.

use zzop_core::{ImportBinding, SourceSymbol};

pub(crate) fn binding(specifier: &str, original: &str, type_only: bool) -> ImportBinding {
    ImportBinding {
        specifier: specifier.into(),
        original: original.into(),
        deferred: false,
        type_only,
    }
}

pub(crate) fn names(syms: &[SourceSymbol]) -> Vec<&str> {
    syms.iter().map(|s| s.name.as_str()).collect()
}
