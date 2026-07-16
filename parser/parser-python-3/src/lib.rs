//! zzop-parser-python-3 — native ruff Python parser -> Common IR projection, at the same grade as
//! `zzop-parser-typescript`'s swc projection. ruff AST types (`ruff_python_ast`/`ruff_python_parser`)
//! stay inside this crate (a ruff upgrade should never leak into the public IR); only `zzop_core` types
//! cross the crate boundary — mirrors the swc isolation discipline `zzop-parser-typescript`'s module doc
//! describes (a sibling isolation guard script covers both crates identically).
//!
//! ## Layout
//! - `lang` — ruff AST -> Common-IR LANGUAGE projection: `SourceSymbol` extraction (`symbols`),
//!   `ImportMap` extraction (`imports`), and identifier-reference collection (`used_names`, dead-export
//!   analysis substrate — mirrors `zzop_parser_typescript::parse_local_identifier_refs`'s purpose).
//! - `adapters` — framework-vocabulary producers emitting cross-layer IO facts: FastAPI route PROVIDES
//!   as router-mount fragments (`adapters::fastapi`) and `requests`/`httpx` literal egress CONSUMES
//!   (`adapters::http_clients`).
//!
//! ## Line numbers
//! ruff gives every node a `TextRange` of UTF-8 BYTE offsets, not line/column positions (unlike swc's
//! `SourceMap`, which resolves a `BytePos` to a line directly). This crate never pulls in an extra ruff
//! line-indexing crate for that — `LineIndex` (below) is a from-scratch newline-byte-offset table built
//! once per file and binary-searched per lookup, the same complexity swc's `SourceMap::lookup_char_pos`
//! offers, just implemented locally.

pub mod adapters;
pub mod lang;

pub use adapters::fastapi::extract_fastapi_router_fragments;
pub use adapters::http_clients::extract_python_http_consumes;
pub use lang::imports::parse_imports;
pub use lang::resolve::python_import_candidates;
pub use lang::symbols::parse_symbols;
pub use lang::used_names::parse_local_identifier_refs;

/// Cache key ingredient for `zzop-cache`, mirroring `zzop_parser_typescript::PARSER_FINGERPRINT`'s
/// scheme: parser id + pinned ruff version + a logic-version counter. The `ruff-0.0.4` segment must
/// match this crate's `Cargo.toml` `ruff_python_parser`/`ruff_python_ast` pin exactly (hand-synced, same
/// TODO the swc crate carries — Phase 2 could derive it from the pin automatically).
/// - `v1`: initial release — symbols (function/async function/class/`Class.method`/top-level simple
///   const assignment), imports (absolute dotted + relative slash-form specifiers, star imports),
///   `used_names`, FastAPI router-mount fragments (`FastAPI()`/`APIRouter()` receivers, verb decorators,
///   `include_router` mounts), and `requests`/`httpx` literal HTTP egress consumes.
/// - `v2`: adversarial-review fixes (F2/F3/F4) that change extraction output — `adapters::http_clients`
///   now mirrors `zzop_parser_typescript::adapters::egress`'s consume-key discipline (`/`-headed literal
///   keyed with query/fragment dropped, absolute `http(s)://` keyed as an external host-carrying key,
///   everything else including a base-relative literal left unresolved — no invented base-relative
///   bucket) instead of unconditionally keying every string literal, reassembles an f-string's literal
///   parts (interpolations -> `{}`) instead of always leaving it unresolved, and discovers call sites via
///   a generic `ruff_python_ast::visitor::Visitor` walk instead of a hand-rolled statement/expression
///   descent that missed nested positions (chained calls, dict/list elements, keyword args, `with`
///   context expressions); `lang::symbols` now honors a fully-static top-level `__all__` literal
///   list/tuple for `exported` (falling back to the underscore convention when absent or
///   partially-computed) and a `Class.method` sub-symbol inherits its class's `exported` value instead of
///   deriving its own from the (possibly dotted) method name.
pub const PARSER_FINGERPRINT: &str = "python3/ruff-0.0.4/v2";

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
