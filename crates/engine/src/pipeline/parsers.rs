//! Language parser dispatch targets: TypeScript / Prisma / Java / Python / Rust / Go / C#, plus the
//! shared lexical loc fallback. The parser's AST never leaves these functions — only `zzop_core` types
//! cross back.

use zzop_core::{ir::SourceSymbol, ImportMap};

/// RAW physical line count computed from raw text alone (no parse) — delegates to
/// `zzop_parser_typescript::count_loc`, so blank and comment lines COUNT (see `MinimalIr::loc`'s doc for
/// the contract). Used for oversized files, lexical-only files, and the fallback when a parse panics.
pub(super) fn lexical_loc(text: &str) -> u32 {
    zzop_parser_typescript::count_loc(text)
}

/// TypeScript parse: symbols + imports + loc, or a degraded lexical fallback.
///
/// `parse_symbols`/`parse_imports` fold "swc couldn't parse this" and "legitimately empty file" into
/// the same empty result, so the broken/empty distinction instead comes from
/// `zzop_parser_typescript::parse_ok`: `false` means swc produced no `Module` at all — route straight to
/// the lexical fallback; `true` proceeds to `parse_symbols`/`parse_imports`. EVERY swc call in here is
/// `catch_unwind`-wrapped, the probe included — see the comment at the probe for the run-killing panic
/// that was measured when it was not.
///
/// Also computes `used_names` (`parse_local_identifier_refs`) for `unimported-export`. Known cost: every
/// extraction call parses independently, and the four in THIS function (`parse_ok`'s probe plus the three
/// below) are a minority of a pass — `super::fresh`/`super::io` run many further extractors over the same
/// text, each parsing again. Deliberately no count here: this doc used to give one ("three times per
/// pass"), it described only this function, and it was wrong about a run by an order of magnitude by the
/// time anyone checked — a number in a comment cannot be re-measured. The per-file figure is measured
/// instead, by `crates/engine/tests/analyze_parse_census.rs` against `zzop_parser_typescript::parse_count`.
/// `zzop_cache::FileIrSlice` caches the results, so a warm run pays the whole bill only once per distinct
/// file content.
pub(super) fn parse_typescript(
    rel: &str,
    text: &str,
    write_site_vocab: &zzop_parser_typescript::WriteSiteVocab<'_>,
) -> (Vec<SourceSymbol>, Option<ImportMap>, u32, bool, Vec<String>) {
    // Wrapped like its three siblings below, and for the reason the asymmetry itself taught: the probe
    // PARSES, so it panics on exactly the inputs they do, and until 2026-07-29 it was the only swc call in
    // this function standing outside a `catch_unwind` — which is how one `.ts` file came to kill a whole
    // `analyze_tree` run. The real fix is at the owner (`zzop_parser_typescript::parse_with_cm` now
    // collapses a panic into the `None` its contract already promised; that comment carries the incident).
    // This stays as the same defense in depth every frontend here carries, and a panic means what `false`
    // means — no `Module` for these bytes, take the lexical lane.
    let parses = std::panic::catch_unwind(|| zzop_parser_typescript::parse_ok(rel, text));
    if !parses.unwrap_or(false) {
        return (
            Vec::new(),
            Some(ImportMap::new()),
            lexical_loc(text),
            true,
            Vec::new(),
        );
    }
    let result = std::panic::catch_unwind(|| {
        let symbols = zzop_parser_typescript::parse_symbols_with_vocab(rel, text, write_site_vocab);
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
///
/// Unlike every other arm here, this one ALSO returns the bridge's `IoFacts` (the per-model
/// `db-table` PROVIDEs `build_common_ir` computes): the other parsers' io is projected by a separate
/// extractor call in `pipeline::fresh`, but Prisma's rides along inside the same `CommonIr` this
/// function already builds, so returning it costs nothing. It was previously DROPPED on the floor here
/// — `crates/engine/tests/rule_contracts/capability_matrix.rs` documented that orphaned capability as a
/// canary-guarded fact ("computed, then discarded by `parse_prisma`"); this is the wiring that closes
/// it, so a `schema.prisma` model's table now reaches the whole-tree provide list and the cross-layer
/// join like a `CREATE TABLE`'s does.
pub(super) fn parse_prisma(
    source_id: &str,
    rel: &str,
    text: &str,
) -> (
    Vec<SourceSymbol>,
    Option<ImportMap>,
    u32,
    bool,
    Option<zzop_core::IoFacts>,
) {
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
            (ir.ir.symbols, None, loc, false, ir.ir.io)
        }
        Err(_) => (Vec::new(), None, lexical_loc(text), true, None),
    }
}

/// Java parse: symbols + imports + loc, or a degraded lexical fallback — same shape/contract as
/// `parse_go` above, backed by `zzop_parser_java_21::parse_java` (a tree-sitter-based frontend) instead
/// of tree-sitter-go. Like `parse_go`, `zzop_parser_java_21::parse_java` already gates its own
/// parse-failure case internally (`Option::None` = the source did not parse into a usable CST) and
/// returns all four facts behind ONE all-or-nothing gate, so there is no separate `parse_ok` probe here —
/// just the `catch_unwind` defense-in-depth every parser frontend in this fused pass carries. Now
/// participates in the shared dep graph, on the terms `fresh::ts_slot` sets.
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

/// C# parse: symbols + imports + loc, or a degraded lexical fallback — same shape/contract as
/// `parse_go` above, backed by `zzop_parser_csharp::parse_csharp` (a tree-sitter-based frontend) instead
/// of tree-sitter-go. Like `parse_go`, `zzop_parser_csharp::parse_csharp` already gates its own
/// parse-failure case internally (`Option::None` = the source did not parse into a usable CST) and
/// returns all four facts behind ONE all-or-nothing gate, so there is no separate `parse_ok` probe here —
/// just the `catch_unwind` defense-in-depth every parser frontend in this fused pass carries.
pub(super) fn parse_csharp(
    rel: &str,
    text: &str,
) -> (Vec<SourceSymbol>, Option<ImportMap>, u32, bool, Vec<String>) {
    let result = std::panic::catch_unwind(|| zzop_parser_csharp::parse_csharp(rel, text));
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
