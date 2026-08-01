//! zzop-parser-python-3 — native ruff Python parser -> Common IR projection, at the same grade as
//! `zzop-parser-typescript`'s swc projection. ruff AST types (`ruff_python_ast`/`ruff_python_parser`)
//! stay inside this crate (a ruff upgrade should never leak into the public IR); only `zzop_core` types
//! cross the crate boundary — mirrors the swc isolation discipline `zzop-parser-typescript`'s module doc
//! describes (a sibling isolation guard script covers both crates identically).
//!
//! ## Layout
//! - `lang` — ruff AST -> Common-IR LANGUAGE projection: `SourceSymbol` extraction (`symbols`),
//!   `ImportMap` extraction (`imports`), identifier-reference collection (`used_names`, dead-export
//!   analysis substrate — mirrors `zzop_parser_typescript::parse_local_identifier_refs`'s purpose), and
//!   `RawCall` call-site attribution (`calls`, the substrate for the engine's whole-repo `SymbolGraph`).
//! - `adapters` — framework-vocabulary producers emitting cross-layer IO facts: FastAPI route PROVIDES
//!   as router-mount fragments (`adapters::fastapi`) and `requests`/`httpx` literal egress CONSUMES
//!   (`adapters::http_clients`), plus the two AUTH-GUARD evidence producers (`adapters::fastapi::guard`,
//!   `adapters::django_routes::guard`) feeding the framework-neutral `auth-guarded` channel.
//!
//! ## Line numbers
//! ruff gives every node a `TextRange` of UTF-8 BYTE offsets, not line/column positions (unlike swc's
//! `SourceMap`, which resolves a `BytePos` to a line directly). This crate never pulls in an extra ruff
//! line-indexing crate for that — `LineIndex` (below) is a from-scratch newline-byte-offset table built
//! once per file and binary-searched per lookup, the same complexity swc's `SourceMap::lookup_char_pos`
//! offers, just implemented locally.

pub mod adapters;
pub mod lang;

pub use adapters::const_map::const_map_fragment;
pub use adapters::django::{extract_django_db_table_consumes, extract_django_db_table_provides};
pub use adapters::django_routes::extract_django_route_fragments;
pub use adapters::django_routes::guard::{
    extract_django_view_guard_classes, extract_django_view_guard_classes_with_vocab,
};
pub use adapters::fastapi::extract_fastapi_router_fragments;
pub use adapters::fastapi::guard::{
    extract_fastapi_guarded_lines, extract_fastapi_guarded_lines_with_vocab,
    extract_python_guard_aliases, extract_python_guard_aliases_with_vocab,
};
pub use adapters::guard_vocab::PythonGuardVocab;
pub use adapters::http_clients::extract_python_http_consumes;
pub use adapters::sqlalchemy::{
    extract_sqlalchemy_db_table_consumes, extract_sqlalchemy_db_table_provides,
};
pub use lang::calls::parse_calls;
pub use lang::imports::parse_imports;
pub use lang::resolve::python_import_candidates;
pub use lang::symbols::parse_symbols;
pub use lang::used_names::parse_local_identifier_refs;

/// Cache-bust token for `zzop-cache`: `parser-id/pinned-toolchain/last-change-version`. The `ruff-0.0.4`
/// segment must match this crate's `Cargo.toml` `ruff_python_parser`/`ruff_python_ast` pin (a ruff upgrade
/// changes extraction → restamp); the trailing `CARGO_PKG_VERSION` is restamped when this crate's projected
/// IR shape changes, else kept so warm Python caches survive the upgrade (2026-07-22 version reform).
///
/// **This string is an ID, not a version — it no longer has to be bumped.** `crates/engine/build.rs`
/// hashes this crate's whole dependency closure into the cache key beside it, so a change to any
/// source here invalidates on its own. What is left is the part a person reads in a cache path or a
/// bug report: which frontend parsed the file. Change it when the FRONTEND changes; correctness no
/// longer depends on remembering.
pub const PARSER_FINGERPRINT: &str = "python3/ruff-0.0.4/0.24.0";

/// Parses `text` with ruff's Python parser, returning `None` on any syntax error (never panics —
/// unexpected/malformed input degrades to `None`, letting the caller fall back to a lexical scan, same
/// contract `zzop_parser_typescript::parse_module` upholds for swc parse failures). Internal-only: ruff's
/// `ModModule` type never crosses this crate's public API.
pub(crate) fn parse_module(text: &str) -> Option<ruff_python_ast::ModModule> {
    let parsed = ruff_python_parser::parse_module(text).ok()?;
    if !parsed.has_valid_syntax() {
        return None;
    }
    Some(parsed.into_syntax())
}

/// A from-scratch newline-byte-offset table (see module doc) resolving a ruff `TextSize` byte offset to
/// its 1-based line number. Built once per file; `line_of` binary-searches it (`O(log n)` per lookup, `O(n)`
/// to build) — the same complexity class as swc's `SourceMap::lookup_char_pos`.
pub(crate) struct LineIndex {
    /// Byte offset of every `\n` in the source, ascending.
    newlines: Vec<u32>,
}

impl LineIndex {
    pub(crate) fn new(text: &str) -> Self {
        let newlines = text
            .char_indices()
            .filter(|&(_, c)| c == '\n')
            .map(|(i, _)| i as u32)
            .collect();
        Self { newlines }
    }

    /// 1-based line number containing byte offset `offset`. `partition_point` returns the count of
    /// newlines strictly before `offset` — i.e. the number of already-completed lines — so `+1` gives the
    /// line `offset` itself sits on.
    pub(crate) fn line_of(&self, offset: ruff_text_size::TextSize) -> u32 {
        let offset: u32 = offset.into();
        self.newlines.partition_point(|&nl| nl < offset) as u32 + 1
    }
}

/// Raw physical line count — mirrors `zzop_parser_typescript::count_loc` exactly (the Rust equivalent of
/// JS `content.split("\n").length`; a trailing newline adds 1). The file is never parsed here, just
/// counted, so this is safe to call even when [`parse_module`] would return `None`.
pub fn count_loc(text: &str) -> u32 {
    text.split('\n').count() as u32
}

/// Language projection: source -> `(symbols, imports, loc, used_names)`, the tuple mirroring the TS
/// pipeline slot's shape. Returns `None` when ruff fails to parse `text` — the caller degrades to a
/// lexical fallback, same contract every parser in this workspace upholds for a parse failure. `imports`
/// and `used_names` are still computed from a fresh parse each (this function does not thread a shared
/// AST across the three calls) — acceptable duplication for the "each function parses internally" public
/// contract this crate's caller (`zzop-engine`) relies on for per-fact caching granularity.
pub fn parse_python(
    rel: &str,
    text: &str,
) -> Option<(
    Vec<zzop_core::SourceSymbol>,
    zzop_core::ImportMap,
    u32,
    Vec<String>,
)> {
    parse_module(text)?; // parse-failure gate only — each sub-call below re-parses independently.
    let symbols = lang::symbols::parse_symbols(rel, text);
    let imports = lang::imports::parse_imports(text);
    let loc = count_loc(text);
    let used_names: Vec<String> = lang::used_names::parse_local_identifier_refs(text)
        .into_iter()
        .collect();
    Some((symbols, imports, loc, used_names))
}

use zzop_core::recognizer::{channel, FrameworkRecognizer};

/// Frameworks this parser recognizes — see [`zzop_core::recognizer`] for what a declaration does and
/// does not claim. Verified against each adapter's RETURN TYPE, not a token scan: `RouterMountFragment`
/// composes into the provide side, `IoConsume` is the consume side, and the db-table adapters emit both.
pub const FRAMEWORK_RECOGNIZERS: &[FrameworkRecognizer] = &[
    FrameworkRecognizer {
        framework: "django",
        extensions: &["py"],
        emits: &[channel::DB],
    },
    FrameworkRecognizer {
        framework: "django",
        extensions: &["py"],
        emits: &[channel::PROVIDES],
    },
    FrameworkRecognizer {
        framework: "fastapi",
        extensions: &["py"],
        emits: &[channel::PROVIDES],
    },
    FrameworkRecognizer {
        framework: "sqlalchemy",
        extensions: &["py"],
        emits: &[channel::DB],
    },
    FrameworkRecognizer {
        framework: "httpx",
        extensions: &["py"],
        emits: &[channel::CONSUMES],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_python_returns_none_on_syntax_error() {
        // Deliberately broken syntax — an unclosed paren.
        assert!(parse_python("bad.py", "def f(:\n    pass\n").is_none());
    }

    #[test]
    fn parse_python_returns_some_on_valid_source() {
        let out = parse_python("ok.py", "def f():\n    pass\n");
        assert!(out.is_some());
    }

    #[test]
    fn line_index_resolves_offsets_across_multiple_lines() {
        let text = "a\nbb\nccc\n";
        let idx = LineIndex::new(text);
        // byte offsets: 'a'=0, '\n'=1, 'b'=2, 'b'=3, '\n'=4, 'c'=5,6,7, '\n'=8
        assert_eq!(idx.line_of(ruff_text_size::TextSize::from(0)), 1);
        assert_eq!(idx.line_of(ruff_text_size::TextSize::from(2)), 2);
        assert_eq!(idx.line_of(ruff_text_size::TextSize::from(5)), 3);
        assert_eq!(idx.line_of(ruff_text_size::TextSize::from(8)), 3);
    }

    #[test]
    fn count_loc_matches_ts_convention() {
        assert_eq!(count_loc("a\nb\n"), 3);
        assert_eq!(count_loc(""), 1);
    }
}
