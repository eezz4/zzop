//! Phase 1: the fused per-file pass's own accumulation loop — walks every `FileArtifact` once and
//! buckets its fields into the per-tree substrates every later `assemble` phase consumes. Pure data
//! shuffling: no composition/resolution logic runs here (that starts in `super::provides`).

use std::collections::{HashMap, HashSet};

use zzop_core::{Finding, ImportMap, IoConsume, IoProvide, ReExport};

use crate::pipeline::FileArtifact;
use crate::EngineConfig;

use super::helpers::{is_go_source_ext, is_java_source_ext, is_rust_source_ext};

mod candidates;
mod census;
mod types;

use candidates::{record_unparsed_extension, stage_package_import_candidate, LangGates};
pub(super) use types::Collected;

/// Runs the fused pass's own accumulation loop over `artifacts`, bucketing every field into a
/// [`Collected`]. See the pre-split monolithic `assemble`'s history for why each substrate exists —
/// field docs above carry that context forward verbatim. `root` is only used for the Rust
/// workspace-member manifest scan ([`crate::pipeline::scan_rust_workspace`]), the Go `go.mod` module
/// manifest scan ([`crate::pipeline::scan_go_modules`]), and the Java `(package, type)` index
/// ([`crate::pipeline::scan_java_index`]) — see task 6's doc on
/// [`super::helpers::resolve_rust_import`] / [`super::helpers::resolve_go_import_package_dir`] /
/// [`super::helpers::resolve_java_import`].
pub(super) fn collect(
    root: &std::path::Path,
    artifacts: Vec<FileArtifact>,
    config: &EngineConfig,
) -> Collected {
    let file_count = artifacts.len();
    // Built up front (before the artifact-consuming loop below, which needs `rel` strings the loop's own
    // `for artifact in artifacts` would otherwise have already moved) — see `scan_rust_workspace`'s doc.
    // Cheap no-op when the tree has no `.rs` files at all (empty iterator -> empty map, no disk I/O).
    let rust_workspace = crate::pipeline::scan_rust_workspace(
        root,
        artifacts
            .iter()
            .map(|a| a.rel.as_str())
            .filter(|rel| is_rust_source_ext(rel)),
    );
    // Same up-front, cheap-when-empty pattern as `rust_workspace` above, for `go.mod` module manifests —
    // see `crate::pipeline::scan_go_modules`'s doc.
    let go_modules = crate::pipeline::scan_go_modules(
        root,
        artifacts
            .iter()
            .map(|a| a.rel.as_str())
            .filter(|rel| is_go_source_ext(rel)),
    );
    // Same up-front, cheap-when-empty pattern as `rust_workspace`/`go_modules` above, for the Java
    // `(package, type)` index — see `crate::pipeline::scan_java_index`'s doc.
    let java_index = crate::pipeline::scan_java_index(
        root,
        artifacts
            .iter()
            .map(|a| a.rel.as_str())
            .filter(|rel| is_java_source_ext(rel)),
    );
    let mut per_file_findings: Vec<Finding> = Vec::new();
    let mut all_symbols = Vec::new();
    let mut loc_by_path: HashMap<String, u32> = HashMap::new();
    let mut ts_import_pairs: Vec<(String, ImportMap)> = Vec::new();
    let mut ts_re_export_pairs: Vec<(String, Vec<ReExport>)> = Vec::new();
    let mut ts_dynamic_import_pairs: Vec<(String, Vec<String>)> = Vec::new();
    let mut ts_paths: HashSet<String> = HashSet::new();
    let mut degraded: Vec<String> = Vec::new();
    let mut minified: Vec<String> = Vec::new();
    let mut io_provides: Vec<IoProvide> = Vec::new();
    let mut io_consumes: Vec<IoConsume> = Vec::new();
    let mut used_names_by_file: HashMap<String, Vec<String>> = HashMap::new();
    let mut prisma_rels: Vec<String> = Vec::new();
    let mut java_rels: Vec<String> = Vec::new();
    let mut rule_time: HashMap<String, (u128, usize)> = HashMap::new();
    let mut package_import_files: std::collections::BTreeMap<
        String,
        std::collections::BTreeSet<String>,
    > = std::collections::BTreeMap::new();
    let mut fragment_pairs: Vec<(String, HashMap<String, String>)> = Vec::new();
    let mut trpc_fragment_pairs: Vec<(String, Vec<zzop_core::ProcedureRouterFragment>)> =
        Vec::new();
    let mut router_mount_pairs: Vec<(String, Vec<zzop_core::RouterMountFragment>)> = Vec::new();
    let mut wrapper_def_pairs: Vec<(String, Vec<zzop_core::WrapperDefFragment>)> = Vec::new();
    let mut wrapper_call_pairs: Vec<(String, Vec<zzop_core::WrapperCallFragment>)> = Vec::new();
    let mut controller_prefix_route_pairs: Vec<(
        String,
        Vec<zzop_core::ControllerPrefixRouteFragment>,
    )> = Vec::new();
    let mut class_shape_pairs: Vec<(String, Vec<zzop_core::ClassShapeFragment>)> = Vec::new();
    let mut query_call_sites: Vec<zzop_core::QueryCallSite> = Vec::new();
    let mut field_usage_tokens: HashSet<String> = HashSet::new();
    // Per-language F5 census staging (Python/Rust/Go): `candidates::stage_package_import_candidate`'s own
    // doc explains why each is deferred rather than censused immediately, and `census`'s own doc explains
    // the post-loop drain below that consumes each of these three.
    let mut python_package_import_candidates: Vec<(String, Option<String>, String)> = Vec::new();
    let mut rust_package_import_candidates: Vec<(String, String)> = Vec::new();
    let mut go_package_import_candidates: Vec<(String, String)> = Vec::new();
    let mut java_package_import_candidates: Vec<(String, String)> = Vec::new();
    let mut unparsed_extensions: std::collections::BTreeMap<String, (usize, Vec<String>)> =
        std::collections::BTreeMap::new();
    // The "bring an adapter" disclosure's overlay-exclusion set: every path any `config.adapter_overlays`
    // entry declares a `FileProjection` for THAT ITSELF CARRIES A REAL FACT (`envelope::
    // overlay_file_carries_facts` — non-empty io/symbols/imports/fragments/attributes, or `is_entry`) —
    // built once, straight from config (same source the `dead-candidates` block in `super::rules` reads
    // for its own `is_entry` union, not re-validated here either), so a file an adapter overlay already
    // covers with REAL data is never told it "has no native parser": the overlay IS the parser for it, and
    // both `apply_adapter_overlays` merge branches (onto an existing native artifact, or a brand-new
    // synthetic one) land that same `rel` in `artifacts` either way, so checking `config` directly here
    // tracks artifact provenance for every VALID overlay.
    //
    // Was previously ANY declared path, regardless of whether its projection carried any facts at all — a
    // bad/empty adapter (every entry's `io`/`symbols`/`imports`/fragments/`attributes` empty, `is_entry:
    // false`) still counted as "coverage" and thereby SUPPRESSED this very disclosure for files it does
    // nothing for. Narrowed to fact-carrying paths so a zero-fact "coverage" claim no longer silences the
    // disclosure (`apply_adapter_overlays` separately warns about the zero-fact overlay itself). Still not
    // exact for an overlay `apply_adapter_overlays` itself rejects (fails `validate_envelope`, e.g. a bad
    // `format` string) — that overlay's fact-carrying declared paths are excluded here even though nothing
    // actually merged, so a file behind a rejected overlay stays silently un-warned rather than reported.
    // Accepted as the same trade-off the `dead-candidates` `is_entry` union in `super::rules` already
    // makes (also read straight from `config.adapter_overlays`, unvalidated) — not a new gap this change
    // introduces.
    let overlay_covered_paths: HashSet<&str> = config
        .adapter_overlays
        .iter()
        .flat_map(|overlay| overlay.files.iter())
        .filter(|file| crate::envelope::overlay_file_carries_facts(file))
        .map(|file| file.path.as_str())
        .collect();

    for artifact in artifacts {
        loc_by_path.insert(artifact.rel.clone(), artifact.loc);
        if artifact.minified_or_generated {
            minified.push(artifact.rel.clone());
        }
        // Computed once per artifact (was two separate `dispatch(...)` calls in the `else if` chain below,
        // plus now a third use for the unparsed-extension check) — `dispatch` is a pure path/extension
        // match, so caching it in a local is a free correctness-neutral simplification, not a behavior
        // change.
        let dispatch_lang = crate::dispatch::dispatch(&artifact.rel, &config.dispatch);
        if artifact.degraded {
            degraded.push(artifact.rel.clone());
        } else if dispatch_lang == Some(crate::dispatch::Language::Prisma) {
            prisma_rels.push(artifact.rel.clone());
        } else if dispatch_lang == Some(crate::dispatch::Language::Java21) {
            java_rels.push(artifact.rel.clone());
        }
        // "Bring an adapter" per-extension disclosure — see `candidates::record_unparsed_extension`'s doc.
        record_unparsed_extension(
            &artifact.rel,
            dispatch_lang,
            &overlay_covered_paths,
            &mut unparsed_extensions,
        );
        if let Some(imports) = artifact.imports {
            // F5 census staging — see `candidates::stage_package_import_candidate`'s doc.
            let gates = LangGates::for_rel(&artifact.rel);
            for binding in imports.values() {
                stage_package_import_candidate(
                    &binding.specifier,
                    &binding.original,
                    &artifact.rel,
                    gates.is_python,
                    gates.is_rust,
                    gates.is_go,
                    gates.is_java,
                    &mut python_package_import_candidates,
                    &mut rust_package_import_candidates,
                    &mut go_package_import_candidates,
                    &mut java_package_import_candidates,
                    &mut package_import_files,
                );
            }
            ts_paths.insert(artifact.rel.clone());
            if !artifact.re_exports.is_empty() {
                ts_re_export_pairs.push((artifact.rel.clone(), artifact.re_exports));
            }
            if !artifact.dynamic_imports.is_empty() {
                ts_dynamic_import_pairs.push((artifact.rel.clone(), artifact.dynamic_imports));
            }
            ts_import_pairs.push((artifact.rel.clone(), imports));
            used_names_by_file.insert(artifact.rel.clone(), artifact.used_names.clone());
        }
        if let Some(io) = artifact.io {
            io_provides.extend(io.provides);
            io_consumes.extend(io.consumes);
        }
        if !artifact.const_map_fragment.is_empty() {
            fragment_pairs.push((artifact.rel.clone(), artifact.const_map_fragment));
        }
        if !artifact.procedure_router_fragments.is_empty() {
            trpc_fragment_pairs.push((artifact.rel.clone(), artifact.procedure_router_fragments));
        }
        if !artifact.router_mount_fragments.is_empty() {
            router_mount_pairs.push((artifact.rel.clone(), artifact.router_mount_fragments));
        }
        if !artifact.wrapper_def_fragments.is_empty() {
            wrapper_def_pairs.push((artifact.rel.clone(), artifact.wrapper_def_fragments));
        }
        if !artifact.wrapper_call_fragments.is_empty() {
            wrapper_call_pairs.push((artifact.rel.clone(), artifact.wrapper_call_fragments));
        }
        if !artifact.controller_prefix_route_fragments.is_empty() {
            controller_prefix_route_pairs.push((
                artifact.rel.clone(),
                artifact.controller_prefix_route_fragments,
            ));
        }
        if !artifact.class_shape_fragments.is_empty() {
            class_shape_pairs.push((artifact.rel.clone(), artifact.class_shape_fragments));
        }
        query_call_sites.extend(artifact.query_call_sites);
        field_usage_tokens.extend(artifact.field_usage_tokens);
        all_symbols.extend(artifact.symbols);
        for t in artifact.rule_timings {
            let entry = rule_time.entry(t.rule_id).or_insert((0, 0));
            entry.0 += t.nanos;
            entry.1 += t.findings;
        }
        per_file_findings.extend(artifact.findings);
    }
    // Files are collected in `artifacts`' own `rel` order (`pipeline::run_file_pass`'s invariant), so a
    // stable sort by `(file, line)` alone reproduces the removed filesystem scan's ordering exactly.
    query_call_sites.sort_by(|a, b| (a.file.as_str(), a.line).cmp(&(b.file.as_str(), b.line)));

    // F5 drain: `ts_paths`/`rust_workspace` are both final now (every artifact's own `insert` above has
    // run) — see `census`'s own doc for what each drain resolves and why a resolved specifier must not
    // pollute `package_import_files` (S2/S4's server-framework/http-client import tripwires, and
    // `cross-layer/sdk-import-no-visible-consume`, would otherwise be polluted by an in-tree specifier).
    census::drain_python_candidates(
        python_package_import_candidates,
        &ts_paths,
        &mut package_import_files,
    );
    census::drain_rust_candidates(
        rust_package_import_candidates,
        &ts_paths,
        &rust_workspace,
        &mut package_import_files,
    );
    census::drain_go_candidates(
        go_package_import_candidates,
        &go_modules,
        &mut package_import_files,
    );
    census::drain_java_candidates(
        java_package_import_candidates,
        &java_index,
        &mut package_import_files,
    );

    Collected {
        file_count,
        per_file_findings,
        all_symbols,
        loc_by_path,
        ts_import_pairs,
        ts_re_export_pairs,
        ts_dynamic_import_pairs,
        ts_paths,
        degraded,
        minified,
        io_provides,
        io_consumes,
        used_names_by_file,
        prisma_rels,
        java_rels,
        rule_time,
        package_import_files,
        fragment_pairs,
        trpc_fragment_pairs,
        router_mount_pairs,
        wrapper_def_pairs,
        wrapper_call_pairs,
        controller_prefix_route_pairs,
        class_shape_pairs,
        query_call_sites,
        field_usage_tokens,
        unparsed_extensions,
        rust_workspace,
        go_modules,
        java_index,
    }
}
