//! The size-cap lane: the gate itself, and the artifact a file that trips it gets. Split out of
//! `super`'s `compute_fresh_artifact` because the gate has a SECOND caller — the warm-cache path asks
//! the same question to name a cached degrade's cause — and a predicate with two callers wants one
//! home, not a copy at each.

use zzop_core::RulePackDef;

use crate::dispatch::Language;
use crate::pipeline::findings::eval_packs;
use crate::pipeline::parsers::lexical_loc;
use crate::pipeline::{DegradeCause, FileArtifact};
use crate::EngineConfig;

use super::{sorted_field_usage_tokens, spans, ts_slot};

/// THE size-cap gate — the one place `EngineConfig::size_cap` is compared against a file's length.
/// Its second caller is `crate::pipeline::artifact::cached_degrade_cause`, which must reach the SAME
/// verdict the cold path reached in order to name a cached degrade's cause; a second `>` written there
/// would drift the day this comparison changes (`>=`, a per-language cap, a cap measured in chars).
pub(in crate::pipeline) fn is_oversized(bytes: &[u8], config: &EngineConfig) -> bool {
    bytes.len() > config.size_cap
}

/// The artifact for a file past [`is_oversized`]: loc counted lexically, no symbols/imports/io, but the
/// text is still scanned by line-scan DSL rules (lexical-only files are excluded from structural
/// projection, not from rule evaluation). `field_usage_tokens` is a raw-text regex scan, never an AST
/// parse, so it runs here too (like the removed `scan_field_usage` walk), unaffected by the size cap.
pub(super) fn oversized_artifact(
    rel: &str,
    text: &str,
    language: Option<Language>,
    config: &EngineConfig,
    packs: &[&RulePackDef],
) -> FileArtifact {
    let loc = lexical_loc(text);
    let (findings, rule_timings, minified_or_generated) = eval_packs(
        packs,
        rel,
        text,
        &[],
        None,
        spans::ProjectedSpans::none().facts(),
        // No AST ⇒ no call sites, no bound string literals: `CallScan`/`LiteralScan` are silent
        // on an oversized file (recall-side degrade); line-scan still runs on the raw text.
        &[],
        &[],
        config.profile_rules,
    );
    FileArtifact {
        rel: rel.to_string(),
        symbols: Vec::new(),
        imports: ts_slot(language),
        re_exports: Vec::new(),
        dynamic_imports: Vec::new(),
        asset_refs: Vec::new(),
        loc,
        findings,
        degrade_cause: Some(DegradeCause::Oversized),
        minified_or_generated,
        io: None,
        rule_timings,
        used_names: Vec::new(),
        exported_signature_names: Vec::new(),
        const_map_fragment: std::collections::HashMap::new(),
        procedure_router_fragments: Vec::new(),
        router_mount_fragments: Vec::new(),
        wrapper_def_fragments: Vec::new(),
        wrapper_call_fragments: Vec::new(),
        controller_prefix_route_fragments: Vec::new(),
        class_shape_fragments: Vec::new(),
        query_call_sites: Vec::new(),
        loop_spans: Vec::new(),
        function_spans: Vec::new(),
        test_spans: Vec::new(),
        call_sites: Vec::new(),
        string_literals: Vec::new(),
        field_usage_tokens: sorted_field_usage_tokens(rel, text),
    }
}
