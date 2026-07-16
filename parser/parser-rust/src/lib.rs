//! zzop-parser-rust — a syn-based Rust parser -> Common IR projection, mirroring
//! `zzop-parser-python-3`'s crate shape and discipline exactly: `syn`/`proc-macro2` AST types stay inside
//! this crate (a syn upgrade should never leak into the public IR); only `zzop_core` types cross the
//! crate boundary.
//!
//! ## Layout
//! - `lang` — syn AST -> Common-IR LANGUAGE projection: `SourceSymbol` extraction (`symbols`),
//!   `ImportMap` extraction (`imports`), identifier-reference collection (`used_names`), and the pure
//!   import-specifier -> candidate-file-path resolver (`resolve`).
//! - `adapters` — framework-vocabulary producers emitting cross-layer IO facts: axum router PROVIDES as
//!   router-mount fragments (`adapters::axum`) and `reqwest` literal egress CONSUMES
//!   (`adapters::http_clients`).
//!
//! ## Line numbers
//! Unlike `zzop-parser-python-3` (which builds its own byte-offset `LineIndex` because ruff only hands
//! back `TextRange` byte offsets), this crate never needs one: `proc-macro2`'s "span-locations" feature
//! (enabled in this crate's `Cargo.toml`, and transitively unified into `syn`'s own `proc-macro2`
//! dependency by Cargo's feature-unification rules, since both depend on the very same crate instance)
//! makes every `Span` produced by `syn::parse_str` carry a real 1-based `LineColumn` computed from the
//! source text itself — `span.start().line` is used directly wherever a symbol's or call site's line is
//! needed, with no extra table to build or search.
//!
//! ## Scope note: macros
//! `macro_rules!` definitions are not extracted as symbols, and identifiers used only INSIDE a macro
//! invocation's argument tokens (e.g. `println!("{}", x)`'s `x`) are not visible to `used_names` — syn
//! parses a macro call's arguments as an opaque `TokenStream`, not a structured `Expr` tree, so nothing
//! inside it is walkable without macro-specific (and inherently guessy) token parsing. Both are
//! documented, deliberate v1 gaps: macro-expansion visibility is out of this crate's never-guess scope,
//! the same way `zzop-parser-python-3` leaves Python's `exec`/`eval` unexamined.

pub mod adapters;
pub mod lang;

pub use adapters::axum::extract_axum_router_fragments;
pub use adapters::http_clients::extract_rust_http_consumes;
pub use lang::imports::parse_imports;
pub use lang::resolve::rust_import_candidates;
pub use lang::symbols::parse_symbols;
pub use lang::used_names::parse_local_identifier_refs;

/// Cache key ingredient for `zzop-cache`, mirroring `zzop_parser_python_3::PARSER_FINGERPRINT`'s scheme:
/// parser id + pinned frontend + a logic-version counter.
/// - `v1`: initial release — symbols (top-level fn/struct/enum/trait/type-alias/const/static/union, plus
///   `impl` block methods/assoc consts emitted dotted as `Type.member`), imports (`use` trees including
///   groups/globs/renames/`pub use`, plus `mod x;` declarations), `used_names`, axum router-mount
///   fragments, and `reqwest` literal HTTP egress consumes.
pub const PARSER_FINGERPRINT: &str = "rust/syn-2/v1";

/// Parses `text` with `syn`, returning `None` on any syntax error (never panics — unexpected/malformed
/// input degrades to `None`, letting the caller fall back to a lexical scan, same contract every parser
/// in this workspace upholds for a parse failure). Internal-only: `syn::File` never crosses this crate's
/// public API.
pub(crate) fn parse_file(text: &str) -> Option<syn::File> {
    syn::parse_str::<syn::File>(text).ok()
}

/// 1-based line of any `syn`/`proc-macro2`-spanned node — see this module's "Line numbers" doc section.
/// Shared by `lang::symbols`, `adapters::axum`, and `adapters::http_clients` so the same one-line
/// span-to-line conversion is never reimplemented per module.
pub(crate) fn line_of<T: syn::spanned::Spanned>(node: &T) -> u32 {
    node.span().start().line as u32
}

/// Raw physical line count — mirrors `zzop_parser_python_3::count_loc` exactly (the Rust equivalent of JS
/// `content.split("\n").length`; a trailing newline adds 1). The file is never parsed here, just
/// counted, so this is safe to call even when `parse_file` would return `None`.
pub fn count_loc(text: &str) -> u32 {
    text.split('\n').count() as u32
}

/// Language projection: source -> `(symbols, imports, loc, used_names)`, the tuple mirroring
/// `zzop_parser_python_3::parse_python`'s pipeline slot shape. Returns `None` when `syn` fails to parse
/// `text` — the caller degrades to a lexical fallback. `imports` and `used_names` are still computed
/// from a fresh parse each (this function does not thread a shared AST across the three calls) —
/// acceptable duplication for the "each function parses internally" public contract this crate's caller
/// (`zzop-engine`) relies on for per-fact caching granularity.
pub fn parse_rust(
    rel: &str,
    text: &str,
) -> Option<(
    Vec<zzop_core::SourceSymbol>,
    zzop_core::ImportMap,
    u32,
    Vec<String>,
)> {
    parse_file(text)?; // parse-failure gate only — each sub-call below re-parses independently.
    let symbols = lang::symbols::parse_symbols(rel, text);
    let imports = lang::imports::parse_imports(text);
    let loc = count_loc(text);
    let used_names: Vec<String> = lang::used_names::parse_local_identifier_refs(text)
        .into_iter()
        .collect();
    Some((symbols, imports, loc, used_names))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rust_returns_none_on_syntax_error() {
        // Deliberately broken syntax — an unclosed paren.
        assert!(parse_rust("bad.rs", "fn f(:\n").is_none());
    }

    #[test]
    fn parse_rust_returns_some_on_valid_source() {
        let out = parse_rust("ok.rs", "fn f() {}\n");
        assert!(out.is_some());
    }

    #[test]
    fn count_loc_matches_python_convention() {
        assert_eq!(count_loc("a\nb\n"), 3);
        assert_eq!(count_loc(""), 1);
    }
}
