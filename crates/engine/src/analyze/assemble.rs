//! `assemble` — the tree-wide assembly orchestrator, split into sequential phases (each phase is a
//! `mod` below, in the order it runs): [`collect`] the fused per-file pass's own output into
//! per-tree substrates, [`provides`] compose whole-tree PROVIDE/CONSUME facts from those substrates,
//! [`dep_graph`] build the dependency graph + run git-history-dependent collection, [`rules`] run every
//! whole-graph/call-graph-BFS native analysis (plus its own whole-tree `Matcher::IoScan` DSL sub-phase),
//! [`warnings`] run the framework-silence coverage self-report, and [`metrics`] compute
//! git-history-dependent scores/health/recommendations/critical/seams. This file is the glue tying every
//! phase's output into the final `AnalyzeOutput` — no composition/analysis logic of its own beyond wiring.

use zzop_core::{merge_findings, CommonIr, IoFacts, MinimalIr};

use crate::analyze::diagnostics::{
    compute_dsl_scope, minified_files_warning, no_applicable_dsl_rule_warning,
    rule_overrides_applied, run_diagnostics, unmatched_global_exclude_warnings,
    unmatched_suppression_warnings, unparsed_extension_warning,
};
use crate::pipeline::FileArtifact;
use crate::{AnalyzeOutput, EngineConfig};

mod collect;
mod dep_graph;
mod helpers;
mod metrics;
mod orm;
mod provides;
mod rules;
mod sfc;
mod warnings;

/// Consumes the fused pass's per-file artifacts and produces the final `AnalyzeOutput`. `artifacts` must
/// already be sorted by `rel` (an invariant `pipeline::run_file_pass` upholds), which is what makes
/// `ir.ir.symbols` deterministic. `root` is only used for the optional git collection (and the phases
/// below that read from disk: Java project pass, file-convention routes, framework-silence probes).
pub(crate) fn assemble(
    root: &std::path::Path,
    artifacts: Vec<FileArtifact>,
    config: &EngineConfig,
) -> AnalyzeOutput {
    let collected = collect::collect(root, artifacts, config);
    let collect::Collected {
        file_count,
        per_file_findings,
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
        used_names_by_file,
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
    } = collected;

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
        &cycles,
        &nodes,
        &dep,
        &pkg_scan,
        &tsconfigs,
        &ts_paths,
        &ts_import_pairs,
        &java_rels,
        &all_symbols,
        &used_names_by_file,
        &prisma_rels,
        &attribute_store,
        &field_usage_tokens,
        &query_call_sites,
        &io_provides,
        &io_consumes,
        &mut rule_time,
        &sfc_import_pairs,
        &sfc_targets,
        &asset_targets,
    );

    let findings = merge_findings(
        vec![per_file_findings, global_findings],
        &config.rule_config,
    );

    degraded.sort();
    minified.sort();
    if let Some(w) = minified_files_warning(&minified) {
        warnings.push(w);
    }
    warnings.extend(unparsed_extension_warning(&unparsed_extensions));
    let rels: Vec<&str> = loc_by_path.keys().map(String::as_str).collect();
    warnings.extend(unmatched_suppression_warnings(config, &rels));
    warnings.extend(unmatched_global_exclude_warnings(config, &rels));
    // One census, two consumers: the zero-applicability warning below and `packs_loaded`'s per-pack
    // `files_in_scope` count (see `compute_dsl_scope`'s doc for the cost model).
    let dsl_scope = compute_dsl_scope(&config.packs, &rels);
    if let Some(w) = no_applicable_dsl_rule_warning(&config.packs, &dsl_scope) {
        warnings.push(w);
    }
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

    // `root.is_dir()` gates this so it doesn't duplicate `analyze_tree`'s more specific "root does not
    // exist / is not a directory" self-report (`lib.rs`'s `scope_warnings`), which already states the cause
    // when the root itself is invalid; every invalid-root failure funnels through `file_count == 0` too. A
    // root that DOES exist but matched no analyzable files gets no such self-report (`lib.rs`'s "0 source
    // files found under root" covers that from a different angle), so this generic line still earns its keep.
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
        // Multiple parser frontends (TypeScript + Prisma, v1 scope) are fused into one tree-wide IR here —
        // no single `parser` id is accurate the way it is for a single-frontend `build_common_ir` call, so
        // this is a zzop-only tag naming the fused engine itself rather than one frontend.
        parser: "engine".to_string(),
        ir: MinimalIr {
            dep,
            symbols: all_symbols,
            loc: loc_by_path,
            io,
        },
    };

    let coverage = crate::CoverageCensus::compute(file_count, &ir, degraded.len());

    // Gated exactly like `scores`/`health`/`critical`/`seams`: `Some` only when git collection actually
    // ran, so a consumer never sees a window echoed for numbers that stayed empty.
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
            // BTreeSet iteration is sorted — first() is the lexicographically first importing file.
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
