//! Per-file processing: read -> cache lookup -> (fresh compute | cached-IR reuse) -> `FileArtifact`.

use std::fs;
use std::path::Path;

use zzop_cache::{AnalysisCache, CacheKey, FileIrSlice};
use zzop_core::{dsl::RuleTiming, RulePackDef};

use crate::cache::CacheCounters;
use crate::dispatch;
use crate::EngineConfig;

use super::findings::{eval_packs, schema_findings, schema_findings_eligible};
use super::fresh::compute_fresh_artifact;
use super::FileArtifact;

/// Processes one file end to end: read -> cache lookup -> (size-cap / dispatch / parse) -> per-file DSL
/// rules -> artifact. Never panics outward: an unreadable file, oversized file, or parser panic all
/// degrade the artifact instead of propagating.
///
/// Cache flow: content-hash `bytes` -> `get_ir`. IR miss -> full parse via `compute_fresh_artifact`,
/// then `put_ir` + `put_findings`. IR hit + findings hit -> full skip, no reparse. IR hit but findings
/// miss (ruleset-only change) -> reuse the cached `FileIrSlice`, re-run `eval_packs`, `put_findings`.
pub(super) fn process_file(
    rel: &str,
    abs: &Path,
    config: &EngineConfig,
    packs: &[&RulePackDef],
    cache: Option<&AnalysisCache>,
    ruleset_fingerprint: Option<&str>,
    counters: Option<&CacheCounters>,
) -> FileArtifact {
    let bytes = match fs::read(abs) {
        Ok(b) => b,
        Err(_) => {
            // Unreadable (permission error, or a race with a concurrent delete) — never a panic, just a
            // degraded empty artifact. No cache lookup: there's no content to hash.
            return FileArtifact {
                rel: rel.to_string(),
                symbols: Vec::new(),
                imports: None,
                re_exports: Vec::new(),
                dynamic_imports: Vec::new(),
                asset_refs: Vec::new(),
                loc: 0,
                findings: Vec::new(),
                degraded: true,
                minified_or_generated: false,
                io: None,
                rule_timings: Vec::new(),
                used_names: Vec::new(),
                const_map_fragment: std::collections::HashMap::new(),
                procedure_router_fragments: Vec::new(),
                router_mount_fragments: Vec::new(),
                wrapper_def_fragments: Vec::new(),
                wrapper_call_fragments: Vec::new(),
                controller_prefix_route_fragments: Vec::new(),
                class_shape_fragments: Vec::new(),
                query_call_sites: Vec::new(),
                field_usage_tokens: Vec::new(),
                loop_spans: Vec::new(),
            };
        }
    };

    let language = dispatch::dispatch(rel, &config.dispatch);

    let cache_key = match (cache, ruleset_fingerprint) {
        (Some(_), Some(rsfp)) => Some(CacheKey {
            content_hash: AnalysisCache::content_hash(&bytes),
            parser_fingerprint: crate::cache::parser_fingerprint(language, config),
            // Without `scope`, two different files with byte-identical content could alias each
            // other's cached IR/findings (which embed their own `file` path).
            scope: crate::cache::cache_scope(config, rel),
            ruleset_fingerprint: rsfp.to_string(),
        }),
        _ => None,
    };

    if let (Some(cache), Some(key)) = (cache, cache_key.as_ref()) {
        if let Some(ir) = cache.get_ir(key) {
            if let Some(findings) = cache.get_findings(key) {
                if let Some(c) = counters {
                    c.record_hit();
                }
                // Full cache hit: no rule evaluation ran this call, so nothing to time.
                return artifact_from_ir(rel, ir, findings, Vec::new());
            }
            // IR hit, findings miss: reuse the parsed IR, re-run rules only.
            let text = String::from_utf8_lossy(&bytes).into_owned();
            let (mut findings, rule_timings, _minified) = eval_packs(
                packs,
                rel,
                &text,
                &ir.symbols,
                ir.io.clone(),
                &ir.loop_spans,
                config.profile_rules,
            );
            if schema_findings_eligible(language, ir.degraded) {
                findings.extend(schema_findings(&config.rule_config, rel, &text));
            }
            let _ = cache.put_findings(key, &findings);
            if let Some(c) = counters {
                c.record_miss();
            }
            return artifact_from_ir(rel, ir, findings, rule_timings);
        }
    }
    if cache_key.is_some() {
        if let Some(c) = counters {
            c.record_miss();
        }
    }

    let text = String::from_utf8_lossy(&bytes).into_owned();
    let artifact = compute_fresh_artifact(rel, &bytes, &text, language, config, packs);

    if let (Some(cache), Some(key)) = (cache, cache_key.as_ref()) {
        let ir_slice = FileIrSlice {
            symbols: artifact.symbols.clone(),
            imports: artifact.imports.clone(),
            re_exports: artifact.re_exports.clone(),
            dynamic_imports: artifact.dynamic_imports.clone(),
            asset_refs: artifact.asset_refs.clone(),
            loc: artifact.loc,
            degraded: artifact.degraded,
            io: artifact.io.clone(),
            used_names: artifact.used_names.clone(),
            minified_or_generated: artifact.minified_or_generated,
            const_map_fragment: artifact.const_map_fragment.clone(),
            procedure_router_fragments: artifact.procedure_router_fragments.clone(),
            router_mount_fragments: artifact.router_mount_fragments.clone(),
            wrapper_def_fragments: artifact.wrapper_def_fragments.clone(),
            wrapper_call_fragments: artifact.wrapper_call_fragments.clone(),
            controller_prefix_route_fragments: artifact.controller_prefix_route_fragments.clone(),
            class_shape_fragments: artifact.class_shape_fragments.clone(),
            query_call_sites: artifact.query_call_sites.clone(),
            field_usage_tokens: artifact.field_usage_tokens.clone(),
            loop_spans: artifact.loop_spans.clone(),
        };
        let _ = cache.put_ir(key, &ir_slice);
        let _ = cache.put_findings(key, &artifact.findings);
    }

    artifact
}

/// Rebuilds a `FileArtifact` from a cached `FileIrSlice` + its (possibly just-recomputed) findings —
/// `rel` is the only piece `FileIrSlice` doesn't carry (not part of the cached payload; the lookup path
/// already knows it). `rule_timings` is empty on a full cache hit.
fn artifact_from_ir(
    rel: &str,
    ir: FileIrSlice,
    findings: Vec<zzop_core::Finding>,
    rule_timings: Vec<RuleTiming>,
) -> FileArtifact {
    FileArtifact {
        rel: rel.to_string(),
        symbols: ir.symbols,
        imports: ir.imports,
        re_exports: ir.re_exports,
        dynamic_imports: ir.dynamic_imports,
        asset_refs: ir.asset_refs,
        loc: ir.loc,
        findings,
        degraded: ir.degraded,
        minified_or_generated: ir.minified_or_generated,
        io: ir.io,
        rule_timings,
        used_names: ir.used_names,
        const_map_fragment: ir.const_map_fragment,
        procedure_router_fragments: ir.procedure_router_fragments,
        router_mount_fragments: ir.router_mount_fragments,
        wrapper_def_fragments: ir.wrapper_def_fragments,
        wrapper_call_fragments: ir.wrapper_call_fragments,
        controller_prefix_route_fragments: ir.controller_prefix_route_fragments,
        class_shape_fragments: ir.class_shape_fragments,
        query_call_sites: ir.query_call_sites,
        field_usage_tokens: ir.field_usage_tokens,
        loop_spans: ir.loop_spans,
    }
}
