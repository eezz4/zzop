//! The per-file CALL-GRAPH projection: this file's call sites, plus the NestJS guard evidence that
//! rides on the same parse.
//!
//! ## Why it lives here and not in the pass that consumes it
//! The whole-tree call-graph pass (`analyze::native_rules::callgraph`) used to read and re-parse every
//! dispatched TypeScript source in its own loop. That cost a SECOND full swc parse per file, and the
//! reason is structural rather than incidental: `parse_with_cm`'s memo is ONE ENTRY and THREAD-LOCAL
//! (`parser-typescript/src/parse/memo.rs`). It collapses the per-file lane's consecutive extractors
//! because they all ask about the same `(rel, text)` in a row — and it can never be warm for a pass
//! that runs afterwards, on other threads, over the whole tree.
//!
//! Measured on this repository (review ledger V103/V108): the pass was 68% of a warm run (~7.5s of
//! 11.4s), and `analyze_parse_census` pinned swc parses at exactly 2N for an N-file tree. Extracting
//! here makes it N, and — because this lane's output is what the cache stores — a warm file pays
//! neither the read nor the parse.
//!
//! ## Why the guard extractors moved too, and why their gate did not come with them
//! Three NestJS extractors sat in that same loop behind a `need_decorator_guarded` flag. Leaving them
//! there would have left the read AND the parse in place for every tree that has an auth rule enabled,
//! which is most of them — so the halved parse count would have been a fixture artifact rather than a
//! property. They run unconditionally here instead. The trade is exact and it is worth stating: three
//! extra AST VISITS on a tree that will not use them, against one whole PARSE saved on every tree. The
//! gate did not disappear, it moved to consumption, where `need_decorator_guarded` still decides
//! whether the evidence is read.
//!
//! ## Rust joined on the same evidence (2026-09-08, review ledger V111)
//! Splitting the call-graph pass's own wall clock said the TypeScript loop was never the expensive
//! part of it. On this repository — 1,395 `.rs` files against 240 TypeScript ones — `rust_guard`
//! was **4.4-5.8s of a ~7.5s warm run**, re-reading and re-parsing every Rust file with `syn` on
//! every run, warm or cold. Same shape, same remedy, and this time the number was measured before
//! the edit rather than inferred from it.
//!
//! What that leaves in the pass for Rust is resolution, not extraction: `resolve_rust_call_target`
//! needs the workspace map and the tree's path set, which are whole-tree facts a per-file lane
//! cannot have.
//!
//! ## Degrade direction: RECALL
//! Empty facts ⇒ the call-graph BFS has fewer edges and the guard evidence is thinner, so the rules
//! that walk it (`mutating-route-no-auth`, `unsafe-read-endpoint`, `non-idempotent-write`) can only
//! fail to report — except that thinner GUARD evidence pushes `mutating-route-no-auth` the other way,
//! toward reporting a route whose guard it can no longer see. That is the same asymmetry the auth
//! vocabulary keys carry, and it is why this projection is gated on a real, non-degraded parse rather
//! than on a lexical fallback: a guess here would be a guess about whether code is protected.

use zzop_core::callgraph::CallGraphFacts;

use crate::dispatch::Language;

/// Project this file's call-graph facts. TypeScript only, and only from a well-formed parse — the same
/// AST gate every sibling projection in `fresh` applies, for the reason in this module's degrade note.
///
/// The four extractors below all go through the parse memo, so the first one pays for the parse and the
/// rest are visits over the module it already returned.
pub(super) fn project(
    language: Option<Language>,
    degraded: bool,
    rel: &str,
    text: &str,
    vocab: &crate::vocabulary::ResolvedVocabulary<'_>,
) -> CallGraphFacts {
    if degraded {
        return CallGraphFacts::default();
    }
    // Rust's arm is call sites only. Its guard evidence is a handler-parameter TYPE, which
    // `parse_extractor_guards` turns into an ordinary graph EDGE rather than a side-channel — see
    // `callgraph::rust_guard`'s module doc — so it belongs in `raw_calls` beside the calls.
    if matches!(language, Some(Language::Rust)) {
        let mut raw_calls = zzop_parser_rust::parse_calls(rel, text);
        raw_calls.extend(zzop_parser_rust::parse_extractor_guards(
            rel,
            text,
            &vocab.rust_guard(),
        ));
        return CallGraphFacts {
            raw_calls,
            ..CallGraphFacts::default()
        };
    }
    if !matches!(language, Some(Language::TypeScript)) {
        return CallGraphFacts::default();
    }
    // The overlay languages that ride the TypeScript dispatch slot (`.svelte`, `.vue`) are excluded for
    // the reason the pass's own loop excluded them: re-reading their whole file as TypeScript injects
    // template text as if it were code.
    if !crate::dead_exports::is_ts_source_ext(rel) {
        return CallGraphFacts::default();
    }
    CallGraphFacts {
        raw_calls: zzop_parser_typescript::parse_calls(rel, text),
        controller_guarded_lines: zzop_parser_typescript::extract_controller_guarded_lines(
            rel, text,
        )
        .into_iter()
        .collect(),
        nest_forroutes: zzop_parser_typescript::extract_nest_forroutes_guarded(rel, text),
        global_prefix: zzop_parser_typescript::extract_global_prefix_marker(rel, text)
            .map(|p| p.key),
    }
}

#[cfg(test)]
mod tests;
