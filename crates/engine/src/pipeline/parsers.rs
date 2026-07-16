//! Language parser dispatch targets: TypeScript / Prisma / Java / Python / Rust / Go, plus the shared
//! lexical loc fallback. The parser's AST never leaves these functions — only `zzop_core` types cross
//! back.

use zzop_core::{ir::SourceSymbol, ImportMap};

/// Non-blank, non-comment line count computed from raw text alone (no parse) — used for oversized
/// files, lexical-only files, and the fallback when a parse panics. Approximate for Prisma text
/// (also uses `//` comments), acceptable for a fallback-only path.
pub(super) fn lexical_loc(text: &str) -> u32 {
    zzop_parser_typescript::count_loc(text)
}

/// TypeScript parse: symbols + imports + loc, or a degraded lexical fallback.
///
/// `parse_symbols`/`parse_imports` fold "swc couldn't parse this" and "legitimately empty file" into
/// the same empty result, so the broken/empty distinction instead comes from
/// `zzop_parser_typescript::parse_ok`: `false` means swc produced no `Module` at all — route straight to
/// the lexical fallback; `true` proceeds to `parse_symbols`/`parse_imports`, still `catch_unwind`-wrapped
/// as defense in depth.
///
/// Also computes `used_names` (`parse_local_identifier_refs`) for `dead-exports`. Known cost: each of
/// the three extraction calls parses independently, so a well-formed file is parsed by swc three times
/// per pass (four counting `parse_ok`'s probe) — `zzop_cache::FileIrSlice::used_names` caches the result
/// so a warm run pays this only once per distinct file content.
pub(super) fn parse_typescript(
    rel: &str,
    text: &str,
) -> (Vec<SourceSymbol>, Option<ImportMap>, u32, bool, Vec<String>) {
    if !zzop_parser_typescript::parse_ok(rel, text) {
        return (
            Vec::new(),
            Some(ImportMap::new()),
            lexical_loc(text),
            true,
            Vec::new(),
        );
    }
    let result = std::panic::catch_unwind(|| {
        let symbols = zzop_parser_typescript::parse_symbols(rel, text);
        let imports = zzop_parser_typescript::parse_imports(rel, text);
        let loc = zzop_parser_typescript::count_loc(text);
        let used_names: Vec<String> =
            zzop_parser_typescript::parse_local_identifier_refs(rel, text)
                .into_iter()
                .collect();
        (symbols, imports, loc, used_names)
    });
    match result {
        Ok((symbols, imports, loc, used_names)) => (symbols, Some(imports), loc, false, used_names),
        Err(_) => (
            Vec::new(),
            Some(ImportMap::new()),
            lexical_loc(text),
            true,
            Vec::new(),
        ),
    }
}

/// Prisma parse: reuses `zzop_parser_prisma::build_common_ir` with a single-file slice. Its parser is a
/// line-based regex scanner with no AST step, so a malformed schema degrades to "zero models found"
/// rather than panicking; `catch_unwind` is still applied as defense in depth. Prisma files never
/// participate in the TS dep graph (`imports: None`, always).
pub(super) fn parse_prisma(
    source_id: &str,
    rel: &str,
    text: &str,
) -> (Vec<SourceSymbol>, Option<ImportMap>, u32, bool) {
    let owned = (rel.to_string(), text.to_string());
    let result = std::panic::catch_unwind(|| {
        zzop_parser_prisma::build_common_ir(source_id, std::slice::from_ref(&owned))
    });
    match result {
        Ok(ir) => {
            let loc = ir
                .ir
                .loc
                .get(rel)
                .copied()
                .unwrap_or_else(|| lexical_loc(text));
            (ir.ir.symbols, None, loc, false)
        }
        Err(_) => (Vec::new(), None, lexical_loc(text), true),
    }
}

/// Java parse: symbols + imports + loc, or a degraded lexical fallback — same shape/contract as
/// `parse_go` above, backed by `zzop_parser_java_21::parse_java` (a tree-sitter-based frontend) instead
/// of tree-sitter-go. Like `parse_go`, `zzop_parser_java_21::parse_java` already gates its own
/// parse-failure case internally (`Option::None` = the source did not parse into a usable CST) and
/// returns all four facts behind ONE all-or-nothing gate, so there is no separate `parse_ok` probe here —
/// just the `catch_unwind` defense-in-depth every parser frontend in this fused pass carries. Now
/// participates in the shared TS/Python/Rust/Go dep graph (`ts_slot`, `pipeline::fresh`'s doc) — a real
/// change from the retired lexical brace-matcher, which never produced an `ImportMap` at all.
pub(super) fn parse_java21(
    rel: &str,
    text: &str,
) -> (Vec<SourceSymbol>, Option<ImportMap>, u32, bool, Vec<String>) {
    let result = std::panic::catch_unwind(|| zzop_parser_java_21::parse_java(rel, text));
    match result {
        Ok(Some((symbols, imports, loc, used_names))) => {
            (symbols, Some(imports), loc, false, used_names)
        }
        Ok(None) | Err(_) => (
            Vec::new(),
            Some(ImportMap::new()),
            lexical_loc(text),
            true,
            Vec::new(),
        ),
    }
}

/// Python parse: symbols + imports + loc, or a degraded lexical fallback — same shape/contract as
/// `parse_typescript` above, backed by `zzop_parser_python_3::parse_python` (ruff-based) instead of swc.
/// Unlike `parse_typescript`, `zzop_parser_python_3::parse_python` already gates its own parse-failure
/// case internally (`Option::None` = ruff couldn't produce a valid `ModModule`) and returns all four
/// facts behind ONE all-or-nothing gate (how many parses the crate runs internally is its own
/// business), so there is no separate `parse_ok` probe here — just the `catch_unwind`
/// defense-in-depth every parser frontend in this fused pass carries.
pub(super) fn parse_python(
    rel: &str,
    text: &str,
) -> (Vec<SourceSymbol>, Option<ImportMap>, u32, bool, Vec<String>) {
    let result = std::panic::catch_unwind(|| zzop_parser_python_3::parse_python(rel, text));
    match result {
        Ok(Some((symbols, imports, loc, used_names))) => {
            (symbols, Some(imports), loc, false, used_names)
        }
        Ok(None) | Err(_) => (
            Vec::new(),
            Some(ImportMap::new()),
            lexical_loc(text),
            true,
            Vec::new(),
        ),
    }
}

/// Rust parse: symbols + imports + loc, or a degraded lexical fallback — same shape/contract as
/// `parse_python` above, backed by `zzop_parser_rust::parse_rust` (a syn-based frontend) instead of ruff.
/// Like `parse_python`, `zzop_parser_rust::parse_rust` already gates its own parse-failure case internally
/// (`Option::None` = the source did not parse into a valid AST) and returns all four facts behind ONE
/// all-or-nothing gate (how many parses the crate runs internally is its own business), so there is no
/// separate `parse_ok` probe here — just the `catch_unwind` defense-in-depth every parser frontend in
/// this fused pass carries.
pub(super) fn parse_rust(
    rel: &str,
    text: &str,
) -> (Vec<SourceSymbol>, Option<ImportMap>, u32, bool, Vec<String>) {
    let result = std::panic::catch_unwind(|| zzop_parser_rust::parse_rust(rel, text));
    match result {
        Ok(Some((symbols, imports, loc, used_names))) => {
            (symbols, Some(imports), loc, false, used_names)
        }
        Ok(None) | Err(_) => (
            Vec::new(),
            Some(ImportMap::new()),
            lexical_loc(text),
            true,
            Vec::new(),
        ),
    }
}

/// Go parse: symbols + imports + loc, or a degraded lexical fallback — same shape/contract as
/// `parse_rust` above, backed by `zzop_parser_go::parse_go` (a tree-sitter-based frontend) instead of syn.
/// Like `parse_rust`, `zzop_parser_go::parse_go` already gates its own parse-failure case internally
/// (`Option::None` = the source did not parse into a usable CST) and returns all four facts behind ONE
/// all-or-nothing gate (how many parses the crate runs internally is its own business), so there is no
/// separate `parse_ok` probe here — just the `catch_unwind` defense-in-depth every parser frontend in
/// this fused pass carries.
pub(super) fn parse_go(
    rel: &str,
    text: &str,
) -> (Vec<SourceSymbol>, Option<ImportMap>, u32, bool, Vec<String>) {
    let result = std::panic::catch_unwind(|| zzop_parser_go::parse_go(rel, text));
    match result {
        Ok(Some((symbols, imports, loc, used_names))) => {
            (symbols, Some(imports), loc, false, used_names)
        }
        Ok(None) | Err(_) => (
            Vec::new(),
            Some(ImportMap::new()),
            lexical_loc(text),
            true,
            Vec::new(),
        ),
    }
}

#[cfg(test)]
mod tests;
