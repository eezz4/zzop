//! zzop-parser-rust — a syn-based Rust parser -> Common IR projection, mirroring
//! `zzop-parser-python-3`'s crate shape and discipline exactly: `syn`/`proc-macro2` AST types stay inside
//! this crate (a syn upgrade should never leak into the public IR); only `zzop_core` types cross the
//! crate boundary.
//!
//! ## Layout
//! - `lang` — syn AST -> Common-IR LANGUAGE projection: `SourceSymbol` extraction (`symbols`),
//!   `ImportMap` extraction (`imports`), identifier-reference collection (`used_names`), the pure
//!   import-specifier -> candidate-file-path resolver (`resolve`), call-site extraction (`calls`),
//!   handler-signature extractor evidence (`extractor_guards`), and TEST-ONLY line spans
//!   (`test_spans`). The middle two are the pair that lifts `.rs` into the shared call graph — see
//!   `extractor_guards`' own doc for why a Rust guard is a TYPE and not a call, and why shipping one
//!   without the other would have been dishonest. `test_spans` is the axis that lets a rule pack tell
//!   Rust's INLINE `#[cfg(test)] mod tests` apart from shipped code, which no path pattern can do.
//! - `adapters` — framework-vocabulary producers emitting cross-layer IO facts: axum router PROVIDES as
//!   router-mount fragments (`adapters::axum`), `reqwest` literal egress CONSUMES
//!   (`adapters::http_clients`), and raw-SQL `db-table` CONSUMES (`adapters::raw_sql`) — the third of
//!   the three cross-layer channels, and the one this crate had no producer for at all until
//!   2026-08-02 (see that module's doc for what the silence cost).
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
//!
//! ## Scope note: inline `mod` bodies
//! An item written inside an inline `mod foo { ... }` block is out of v1 scope for every SYMBOL-KEYED
//! projection in this crate — `lang::symbols` mints no `SourceSymbol` for it, `lang::imports` reads no
//! `use` from it, `lang::calls` attributes no `RawCall` from it, and `adapters::axum` finds no router
//! built in it. USER-VISIBLE CONSEQUENCE, stated because a reader would otherwise read silence as a
//! verdict: a guard call, an import, or an axum route that exists ONLY inside an inline `mod` is
//! invisible, so `mutating-route-no-auth` can report a route that is in fact guarded that way, and a
//! route registered that way is never reported at all.
//!
//! `lang::calls` was the one dissenter until 2026-08-10, and walking in cost more than the silence
//! does: with no symbol of its own, a nested item's calls were attributed to the id a TOP-LEVEL
//! homonym holds, so an unrelated module's auth call cleared an open mutating route (measured through
//! the engine — that module's doc carries the reproduction). Line-keyed projections are unaffected and
//! deliberately still see inside: `lang::call_sites` and `lang::test_spans` walk the whole file,
//! because a line anchor cannot be mis-attributed to the wrong symbol.
//!
//! The v1 scope exists because a qualified id (`file.rs#foo::inner`) has to be minted by every one of
//! those producers at once or the graph dangles. Widening it is that batch, not a local edit.

pub mod adapters;
pub mod lang;

pub use adapters::axum::extract_axum_router_fragments;
pub use adapters::http_clients::extract_rust_http_consumes;
pub use adapters::raw_sql::extract_rust_raw_sql_db_table_consumes;
pub use lang::call_sites::extract_call_sites;
pub use lang::calls::parse_calls;
pub use lang::extractor_guards::{
    parse_extractor_guards, RustGuardVocab, RUST_OPTIONAL_EXTRACTOR_PREFIXES,
};
pub use lang::imports::parse_imports;
pub use lang::loop_spans::extract_loop_spans;
pub use lang::resolve::{rust_import_candidates, PATH_ATTR_HEAD};
pub use lang::string_literals::extract_string_literals;
pub use lang::symbols::parse_symbols;
pub use lang::test_spans::extract_test_spans;
pub use lang::used_names::parse_local_identifier_refs;

/// Cache key ingredient for `zzop-cache`, mirroring `zzop_parser_python_3::PARSER_FINGERPRINT`'s
/// scheme: parser id + frontend + trailing counter. The `syn-2` segment is a HUMAN LABEL, not a pin —
/// `Cargo.toml` declares `syn = "2"`, a caret range.
///
/// **This string is an ID, not a version — it no longer has to be bumped.** `crates/engine/build.rs`
/// hashes this crate's whole dependency closure into the cache key beside it, so a change to any
/// source here invalidates on its own. What is left is the part a person reads in a cache path or a
/// bug report: which frontend parsed the file. Change it when the FRONTEND changes; correctness no
/// longer depends on remembering.
pub const PARSER_FINGERPRINT: &str = "rust/syn-2/0.21.0";

/// Parses `text` with `syn`, returning `None` on any syntax error (never panics — unexpected/malformed
/// input degrades to `None`, letting the caller fall back to a lexical scan, same contract every parser
/// in this workspace upholds for a parse failure). Internal-only: `syn::File` never crosses this crate's
/// public API.
pub(crate) fn parse_file(text: &str) -> Option<syn::File> {
    syn::parse_str::<syn::File>(text).ok()
}

/// 1-based line of any `syn`/`proc-macro2`-spanned node — see this module's "Line numbers" doc section.
/// Shared by `lang::symbols`, `lang::call_sites`, `lang::string_literals`, `adapters::axum`, and
/// `adapters::http_clients` so the same one-line span-to-line conversion is never reimplemented per
/// module.
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

use zzop_core::recognizer::{channel, FrameworkRecognizer};

/// Frameworks this parser recognizes — see [`zzop_core::recognizer`]. Verified against return types.
pub const FRAMEWORK_RECOGNIZERS: &[FrameworkRecognizer] = &[
    FrameworkRecognizer {
        framework: "axum",
        extensions: &["rs"],
        emits: &[channel::PROVIDES],
    },
    FrameworkRecognizer {
        framework: "reqwest",
        extensions: &["rs"],
        emits: &[channel::CONSUMES],
    },
    // Spelled after the SHAPE, not after a crate — the same call `parser-typescript` made for its own
    // `raw sql` row and for `pathname dispatch`, and for the same reason: there is no single package to
    // name. `adapters::raw_sql` recognizes a SQL statement string wherever it sits, which is how one row
    // covers sqlx, tokio-postgres, rusqlite, `diesel::sql_query` and `sea_orm::Statement` at once. What
    // it does NOT cover is the schema-DSL tier — `diesel::table!` and sea-orm's `DeriveEntityModel`
    // declare tables without ever writing SQL, and this build has no recognizer for either; that is the
    // residual a reader of this list should assume, because a row named after a crate would have implied
    // otherwise.
    FrameworkRecognizer {
        framework: "raw sql",
        extensions: &["rs"],
        emits: &[channel::DB],
    },
];

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
