//! `assemble` — the tree-wide assembly orchestrator, split into sequential phases (each a `mod` below, in
//! run order): [`collect`] per-tree substrates out of the fused per-file pass, [`provides`] whole-tree
//! PROVIDE/CONSUME facts, [`dep_graph`] dep graph + git-history collection, [`rules`] every whole-graph/
//! call-graph-BFS native analysis (plus its own whole-tree `Matcher::IoScan` DSL sub-phase), [`warnings`]
//! framework-silence self-report, [`metrics`] git-dependent scores. Glue only — no analysis logic here.

use zzop_core::{merge_findings, CommonIr, IoFacts, MinimalIr};

use crate::analyze::diagnostics::{
    compute_dsl_scope, global_exclude_diagnostics, minified_files_warning, pack_scope_warnings,
    rule_overrides_applied, run_diagnostics, uncompilable_rule_warnings,
    unmatched_suppression_warnings, unparsed_extension_warning,
};
use crate::{pipeline::FileArtifact, AnalyzeOutput, EngineConfig};

mod collect;
mod dep_graph;
// `pub(in crate::analyze)`: `native_rules::callgraph`'s python/rust arms share these predicates and resolvers — the one agreement point outside this mod, reached as `assemble::helpers::*`.
pub(in crate::analyze) mod helpers;
mod metrics;
mod orm;
mod provides;
mod rules;
mod sfc;
mod warnings;

/// Consumes the fused pass's per-file artifacts and produces the final `AnalyzeOutput`. `artifacts` must
/// already be sorted by `rel` (`pipeline::run_file_pass`'s invariant), which is what makes `ir.ir.symbols`
/// deterministic. `root` is only used for the optional git collection and the phases below that read from
/// disk (Java project pass, file-convention routes, framework-silence probes). `overlay_applied` is
/// `envelope::apply_adapter_overlays`' return value — see [`collect`] and [`rules::GraphInputs`].
pub(crate) fn assemble(
    root: &std::path::Path,
    artifacts: Vec<FileArtifact>,
    config: &EngineConfig,
    overlay_applied: &crate::envelope::OverlayApplication,
) -> AnalyzeOutput {
    let collect::Collected {
        file_count,
        source_files,
        mut per_file_findings,
        all_symbols,
        loc_by_path,
        ts_import_pairs,
        ts_re_export_pairs,
        ts_dynamic_import_pairs,
        ts_asset_ref_pairs,
        ts_paths,
        mut degraded,
        mut minified,
        io_provides,
        io_consumes,
        dead_export_names_by_file,
        prisma_rels,
        java_rels,
        csharp_rels,
        mut rule_time,
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
        csharp_index,
        sfc_rels,
    } = collect::collect(root, artifacts, config, &overlay_applied.covered_paths);

    let sfc_import_pairs = sfc::collect_sfc_import_pairs(root, &sfc_rels);
    let provides::ProvidesResult {
        mut io_provides,
        mut io_consumes,
        mut warnings,
        attribute_store,
        pkg_scan,
        tsconfigs,
    } = provides::compose(
        root,
        config,
        &loc_by_path,
        &ts_paths,
        &java_rels,
        &csharp_rels,
        &all_symbols,
        io_provides,
        io_consumes,
        fragment_pairs,
        trpc_fragment_pairs,
        router_mount_pairs,
        wrapper_def_pairs,
        wrapper_call_pairs,
        controller_prefix_route_pairs,
        class_shape_pairs,
        &rust_workspace,
        &go_modules,
    );

    let dep_graph::DepGraphResult {
        dep,
        cycles,
        nodes,
        folders,
        commits,
        git_active,
        sfc_targets,
        asset_targets,
    } = dep_graph::build(
        root,
        config,
        &mut warnings,
        &loc_by_path,
        &ts_import_pairs,
        &ts_re_export_pairs,
        &ts_dynamic_import_pairs,
        &ts_paths,
        &pkg_scan,
        &tsconfigs,
        &rust_workspace,
        &go_modules,
        &java_index,
        &csharp_index,
        &sfc_import_pairs,
        &ts_asset_ref_pairs,
    );

    let global_findings = rules::run(
        root,
        config,
        &rules::GraphInputs {
            cycles: &cycles,
            nodes: &nodes,
            dep: &dep,
            sfc_targets: &sfc_targets,
            asset_targets: &asset_targets,
            overlay_entry_paths: &overlay_applied.entry_paths,
        },
        &pkg_scan,
        &tsconfigs,
        &ts_paths,
        &ts_import_pairs,
        &java_rels,
        &rust_workspace,
        &all_symbols,
        &dead_export_names_by_file,
        &prisma_rels,
        &attribute_store,
        &field_usage_tokens,
        &query_call_sites,
        &io_provides,
        &io_consumes,
        &mut rule_time,
        &sfc_import_pairs,
        &mut per_file_findings,
        &mut warnings,
    );

    let findings = merge_findings(
        vec![per_file_findings, global_findings],
        &config.rule_config,
    );

    degraded.sort();
    minified.sort();
    let rels: Vec<&str> = loc_by_path.keys().map(String::as_str).collect();
    // One census, three consumers: both warnings below and `packs_loaded`'s `files_in_scope` count.
    let dsl_scope = compute_dsl_scope(&config.packs, &rels);
    if let Some(w) = minified_files_warning(&minified, &dsl_scope.in_scope_rels) {
        warnings.push(w);
    }
    warnings.extend(unparsed_extension_warning(&unparsed_extensions));
    warnings.extend(unmatched_suppression_warnings(config, &rels));
    warnings.extend(global_exclude_diagnostics(config, &rels));
    warnings.extend(pack_scope_warnings(config, &dsl_scope));
    warnings.extend(uncompilable_rule_warnings(&config.packs)); // dead rule != quiet rule
    helpers::sort_io_provides(&mut io_provides);
    helpers::sort_io_consumes(&mut io_consumes);

    warnings.extend(warnings::framework_silence_warnings(
        root,
        &io_provides,
        &io_consumes,
        &ts_paths,
        &java_rels,
        &package_import_files,
        &loc_by_path,
        &config.vocabulary.resolve().fetch_wrapper_export_names,
    ));
    let io = if io_provides.is_empty() && io_consumes.is_empty() {
        None
    } else {
        Some(IoFacts {
            provides: io_provides,
            consumes: io_consumes,
        })
    };

    let metrics::MetricsResult {
        scores,
        health,
        recommendations,
        critical,
        seams,
        layer_co_churn,
    } = metrics::compute(
        config,
        &nodes,
        &dep,
        &cycles,
        &commits,
        git_active,
        &findings,
        &mut rule_time,
    );

    let diagnostics_report =
        run_diagnostics(file_count, &dep, &all_symbols, &commits, config, git_active);
    warnings.extend(diagnostics_report.warnings);
    let config_warnings = diagnostics_report.config_warnings;

    // `root.is_dir()` gates this so it doesn't duplicate `analyze_tree`'s more specific "root missing / not
    // a directory" self-report (`lib.rs`'s `scope_warnings`); an existing-but-empty root gets no such one.
    if file_count == 0 && root.is_dir() {
        warnings.push(
            "root produced 0 analyzable files — check the path exists and contains supported source files".to_string(),
        );
    }

    let rule_timings = config
        .profile_rules
        .then(|| crate::analyze::sort_rule_timings(rule_time));

    let ir = CommonIr {
        source: config.source_id.clone(),
        // Multiple parser frontends (TypeScript + Prisma, v1 scope) fuse into one tree-wide IR here, so no
        // single `parser` id is accurate the way it is for a single-frontend `build_common_ir` call — this
        // is a zzop-only tag naming the fused engine itself rather than one frontend.
        parser: "engine".to_string(),
        ir: MinimalIr {
            dep,
            symbols: all_symbols,
            loc: loc_by_path,
            io,
        },
    };

    let coverage = crate::CoverageCensus::compute(file_count, source_files, &ir, degraded.len());

    // Gated like `scores`/`health`/`critical`/`seams`: `Some` only when git collection actually ran, so no
    // consumer sees a window echoed for numbers that stayed empty.
    let git_window = git_active
        .then_some(config.git.as_ref())
        .flatten()
        .map(|g| crate::GitWindow {
            recent_days: g.recent_days,
            since: g.since.clone(),
        });

    let package_imports = package_import_files
        .into_iter()
        .map(|(specifier, files)| crate::PackageImportSummary {
            file_count: files.len(),
            // BTreeSet iteration is sorted -> the lexicographically first importing file, deterministic.
            example_file: files.into_iter().next().unwrap_or_default(),
            specifier,
        })
        .collect();

    AnalyzeOutput {
        ir,
        findings,
        degraded,
        file_count,
        coverage,
        package_imports,
        attributes: attribute_store,
        nodes,
        scores,
        health,
        recommendations,
        critical,
        seams,
        folders,
        layer_co_churn,
        packs_loaded: crate::PackLoaded::from_config(config, &dsl_scope.files_in_scope_by_pack),
        warnings,
        config_warnings,
        // Set by `analyze_tree` after this returns (needs `pipeline::run_file_pass`'s private counters).
        cache: None,
        rule_timings,
        rule_overrides_applied: rule_overrides_applied(config),
        git_window,
    }
}
