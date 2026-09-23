//! `assemble` — the tree-wide assembly orchestrator, split into sequential phases (each a `mod` below, in
//! run order): [`collect`] per-tree substrates out of the fused per-file pass, [`provides`] whole-tree
//! PROVIDE/CONSUME facts, [`dep_graph`] dep graph + git-history collection, [`rules`] every whole-graph/
//! call-graph-BFS native analysis (plus its own whole-tree `Matcher::IoScan` DSL sub-phase), [`warnings`]
//! framework-silence self-report, [`metrics`] git-dependent scores. Glue only — no analysis logic here.

use zzop_core::merge_findings;

use crate::analyze::diagnostics::rule_overrides_applied;
use crate::{pipeline::FileArtifact, AnalyzeOutput, EngineConfig};

mod collect;
// The one degraded-file record, re-surfaced for `analyze::diagnostics`' cause self-report — the census
// that produces it and the report that reads it must not each hold their own idea of what a degrade is.
pub(in crate::analyze) use collect::DegradedFile;
mod declared;
mod dep_graph;
mod diagnose;
mod finalize;
mod fuse;
// `pub(in crate::analyze)`: `native_rules::callgraph`'s python/rust arms share these predicates and resolvers, reached as `assemble::helpers::*`. (`rules` below is the other cross-mod export.)
pub(in crate::analyze) mod helpers;
mod metrics;
mod nuxt_auto_import;
mod orm;
mod prescan;
mod provides;
pub(in crate::analyze) mod rules; // pub for `analyze/mod.rs`'s re-surface of the profiled io-scan evaluator (Mode A reuse — see `rules`'s re-export comment)
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
    git_cache: &crate::analyze::GitCache,
) -> AnalyzeOutput {
    let collect::Collected {
        file_count,
        parser_dispatched,
        mut per_file_findings,
        all_symbols,
        loc_by_path,
        ts_import_pairs,
        ts_re_export_pairs,
        ts_dynamic_import_pairs,
        ts_call_graph_pairs,
        ts_asset_ref_pairs,
        ts_paths,
        mut degraded,
        mut minified,
        suppress_markers,
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
        prescan_rels,
    } = collect::collect(root, artifacts, config, &overlay_applied.covered_paths);

    // Assemble-time disk reads for references the cached per-file pass cannot see: pre-scanned
    // `<script>`/frontmatter imports, and Nuxt auto-import targets reached by bare symbol name (applied
    // as a `fan_in` bump, never a `dep` node, by `dep_graph`'s third fan-in arm).
    let prescan =
        prescan::collect_prescan(root, &prescan_rels, &ts_paths, &dead_export_names_by_file);
    let prescan_import_pairs = prescan.import_pairs;
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
        prescan_targets,
        asset_targets,
        auto_import_targets,
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
        &all_symbols,
        &rust_workspace,
        &go_modules,
        &java_index,
        &csharp_index,
        &prescan_import_pairs,
        &ts_asset_ref_pairs,
        &prescan.auto_import_refs,
        git_cache,
    );

    let global_findings = rules::run(
        root,
        config,
        &rules::GraphInputs {
            cycles: &cycles,
            nodes: &nodes,
            dep: &dep,
            prescan_targets: &prescan_targets,
            asset_targets: &asset_targets,
            auto_import_targets: &auto_import_targets,
            auto_import_names: &prescan.auto_import_refs.names_by_target,
            overlay_entry_paths: &overlay_applied.entry_paths,
        },
        &pkg_scan,
        &tsconfigs,
        &ts_paths,
        &ts_import_pairs,
        &ts_call_graph_pairs,
        &java_rels,
        &java_index,
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
        &prescan_import_pairs,
        &mut per_file_findings,
        &mut warnings,
    );

    let findings = merge_findings(
        vec![per_file_findings, global_findings],
        &config.rule_config,
    );

    degraded.sort_by(|a, b| a.rel.cmp(&b.rel));
    minified.sort();
    helpers::sort_io_provides(&mut io_provides);
    helpers::sort_io_consumes(&mut io_consumes);
    let rels: Vec<&str> = loc_by_path.keys().map(String::as_str).collect();
    // Filled by the provide-side tripwire trio inside `sweep`, and only when it actually measured (see
    // its out-param doc). `None` travels out to the run-wide provide-blind gate meaning "not measured",
    // which is NOT the same as "no routes visible" — the gate has to be able to tell those apart.
    let mut visible_route_registrations: Option<usize> = None;
    let dsl_scope = diagnose::sweep(
        &diagnose::DiagnoseInput {
            root,
            config,
            rels: &rels,
            minified: &minified,
            suppress_markers: &suppress_markers,
            degraded: &degraded,
            unparsed_extensions: &unparsed_extensions,
            ts_paths: &ts_paths,
            java_rels: &java_rels,
            csharp_rels: &csharp_rels,
            package_import_files: &package_import_files,
            loc_by_path: &loc_by_path,
            all_symbols: &all_symbols,
            dep: &dep,
            overlay_io: &overlay_applied.io_by_parser,
        },
        &io_provides,
        &io_consumes,
        &mut warnings,
        &mut visible_route_registrations,
    );
    let io = diagnose::fold_io(io_provides, io_consumes);

    let finalize::Finalized {
        metrics:
            metrics::MetricsResult {
                scores,
                health,
                recommendations,
                critical,
                critical_truncated,
                seams,
                layer_co_churn,
                co_change,
            },
        config_warnings,
        rule_timings,
        ir,
        coverage,
        git_window,
    } = finalize::run(
        finalize::FinalizeInputs {
            root,
            config,
            nodes: &nodes,
            dep,
            cycles,
            commits,
            git_active,
            findings: &findings,
            rule_time,
            file_count,
            all_symbols,
            loc_by_path,
            io,
            parser_dispatched,
            degraded: degraded.len(),
            ts_import_pairs: &ts_import_pairs,
            ts_re_export_pairs: &ts_re_export_pairs,
            ts_dynamic_import_pairs: &ts_dynamic_import_pairs,
        },
        &mut warnings,
    );

    AnalyzeOutput {
        ir,
        findings,
        // The wire field is the PATH list and stays exactly that: the cause rides the `warnings` channel
        // (`diagnostics::degraded_files`), which is where every other "what did this run fail to see"
        // sentence already lives, rather than widening a published array's element shape.
        degraded: degraded.into_iter().map(|d| d.rel).collect(),
        // Sorted by its producer — a SET upstream, and §6 makes byte-identical output a contract.
        build_script_paths: pkg_scan.sorted_script_paths(),
        file_count,
        coverage,
        package_imports: crate::PackageImportSummary::census(package_import_files),
        visible_route_registrations,
        attributes: attribute_store,
        nodes,
        scores,
        health,
        recommendations,
        critical,
        critical_truncated,
        seams,
        folders,
        layer_co_churn,
        co_change,
        packs_loaded: crate::PackLoaded::from_config(config, &dsl_scope),
        native_analyses: crate::NativeAnalyses::of(&config.rule_config),
        warnings,
        config_warnings,
        // Set by `analyze_tree` after this returns (needs `pipeline::run_file_pass`'s private counters).
        cache: None,
        rule_timings,
        rule_overrides_applied: rule_overrides_applied(config),
        git_window,
    }
}
