//! Mode A orchestrator — `analyze_envelope`, the whole-tree envelope entry point. The per-file
//! accumulation loop lives in `file_pass` (extracted verbatim); this module keeps the pack gating,
//! fragment composition, whole-graph analyses, warnings, and output assembly, in the same order as
//! the pre-split single function.

use std::collections::HashSet;

use zzop_core::{
    circular_from_dep_excluding, merge_findings, registry, CommonIr, GitStats, MinimalIr,
    NormalizedEnvelope, RulePackDef, DEFAULT_WEIGHTS,
};

use crate::analyze::dep_stats_from_dep;
use crate::{AnalyzeOutput, EngineConfig};

use super::file_pass::{run_file_pass, FilePassState};
use super::reserved::reserved_drop_warning;
use super::resolve::{envelope_rule_pack, resolve_envelope_specifier};

/// Ingests one `NormalizedEnvelope` (already validated — see `zzop_core::validate_envelope`) and
/// produces the same `AnalyzeOutput` shape `analyze_tree` does, per the envelope module's doc for
/// which analyses run and which are skipped in envelope mode. Files are processed in `path`-sorted
/// order (mirroring `pipeline::run_file_pass`) so output is deterministic regardless of the
/// envelope's own file order.
pub fn analyze_envelope(envelope: &NormalizedEnvelope, config: &EngineConfig) -> AnalyzeOutput {
    let mut files: Vec<&zzop_core::FileProjection> = envelope.files.iter().collect();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let file_count = files.len();

    let all_paths: HashSet<&str> = files.iter().map(|f| f.path.as_str()).collect();

    // Pack-level AND per-rule `disabled_rules` gating, same split `pipeline::run_file_pass` uses:
    // pack-level drops a whole disabled pack via `is_enabled`, then `gate_pack_rules` (shared, not
    // duplicated) drops an individually-disabled `"{pack}/{rule}"` id. `envelope_rule_pack`'s
    // SymbolScan/IoScan-only filter runs last.
    let enabled_packs: Vec<RulePackDef> = config
        .packs
        .iter()
        .filter(|p| registry::is_pack_enabled(&config.rule_config, &p.id))
        .map(|p| crate::pipeline::gate_pack_rules(p, &config.rule_config))
        .map(|p| envelope_rule_pack(&p))
        .filter(|p| !p.rules.is_empty())
        .collect();

    // Per-file fact collection + hand-built dep edges + SymbolScan/IoScan DSL pass — see
    // `file_pass::run_file_pass` (the pre-split in-function loop, moved verbatim).
    let FilePassState {
        loc_by_path,
        mut degraded,
        all_symbols,
        mut io_provides,
        mut io_consumes,
        dep,
        noncycle_edges,
        per_file_findings,
        trpc_fragment_pairs,
        router_mount_pairs,
        const_fragment_pairs,
        class_shape_pairs,
        package_import_files,
        reserved_dropped,
        mut rule_time,
    } = run_file_pass(&files, &all_paths, &enabled_packs, config.profile_rules);

    // Fragment composition + late const-map consume re-resolution must run before `io_provides`/
    // `io_consumes` are sorted and frozen into `MinimalIr::io` below.
    if !trpc_fragment_pairs.is_empty() {
        let composed =
            crate::analyze::compose_trpc_provides(trpc_fragment_pairs, |specifier, from_file| {
                resolve_envelope_specifier(specifier, from_file, &all_paths)
            });
        io_provides.extend(composed);
    }
    // `compose_router_mount_provides` also composes producer-judged attributes riding the same
    // fragments (e.g. a recognized Express middleware guard) — kept, together with the envelope's own
    // per-file `attributes`, in `AnalyzeOutput::attributes` below. Mode A never runs
    // `schema_usage_findings` and never joins `analyze_trees`' cross-layer stage, but this store DOES
    // have a Mode A rule consumer now: the envelope call-graph pass below threads it into
    // `mutating-route-no-auth` as `route_attr_store`, so an injected `auth-guarded` attribute clears
    // a route here exactly as it does natively (it is also `AnalyzeOutput` plumbing — the cross-layer
    // idempotency veto reads it for filesystem trees).
    let mut warnings: Vec<String> = Vec::new();
    let mut native_attrs: Vec<zzop_core::Attribute> = Vec::new();
    // Merged BEFORE `const_fragment_pairs` moves into `late_resolve_cross_file_consumes` below —
    // `MountRef` prefixes resolve against the same map an envelope's consumes do, so a Mode A producer
    // gets identical semantics to the native path rather than a second, quieter one.
    let merged_consts = crate::analyze::merge_const_map_fragments(&const_fragment_pairs);
    if !router_mount_pairs.is_empty() {
        let (composed, attrs) = crate::analyze::compose_router_mount_provides(
            router_mount_pairs,
            |specifier, from_file, _ident| {
                resolve_envelope_specifier(specifier, from_file, &all_paths)
            },
            &merged_consts,
            &mut warnings,
        );
        io_provides.extend(composed);
        native_attrs = attrs;
    }
    let attribute_store =
        zzop_core::AttributeStore::from_parts(native_attrs, std::slice::from_ref(envelope));
    crate::analyze::late_resolve_cross_file_consumes(const_fragment_pairs, &mut io_consumes);
    // `body`/`response` dtoRef resolution — the native assemble seam, reused at the same position
    // (after every provide-composition pass, before any whole-tree rule reads `io_provides`); also
    // strips + discloses the no-return-type sentinel so it can never leak into `MinimalIr::io`.
    super::shapes::resolve_shape_refs(&mut io_provides, &class_shape_pairs, &mut warnings);

    // Whole-tree `Matcher::IoScan` DSL pass — the envelope-mode counterpart of `analyze::assemble`'s own
    // (native-path) call, run here now that `io_provides`/`io_consumes`/`attribute_store` above all exist.
    // `anchor_line` is always `None`: an envelope carries no source text (see this module's doc, "No
    // source text" bullet), so `anchor_exclude_pattern`/suppress-marker recognition stay honestly inactive
    // — the same "no info available, never a guess" contract every `None` callback result gets in
    // `eval_pack_io_scan`'s own doc. No decorator-guard minting here: Mode A has no source text, so
    // no decorator/annotation producer ever runs and there is no `decorator_guarded` evidence to mint
    // from — `attribute_store` is used as-is. (The envelope call-graph pass below is edge-only for
    // the same reason; injected `auth-guarded` attributes are the envelope-native guard channel.)
    // `enabled_packs` is
    // already the same is_enabled/`gate_pack_rules`/`envelope_rule_pack`-gated pack list `run_file_pass`
    // evaluated per-file above; reused here unchanged for the whole-tree pass.
    let anchor_line = |_: &str, _: u32| None;
    let io_scan_ctx = zzop_core::IoScanTreeContext {
        provides: &io_provides,
        consumes: &io_consumes,
        attrs: &attribute_store,
        anchor_line: &anchor_line,
    };
    // Profiled branch = the SAME pack-splitting evaluator the native whole-tree pass uses
    // (`assemble::rules::io_scan::eval_pack_timed`, re-exported): one `"{pack}/{rule}"` entry per
    // `IoScan` rule into the one shared accumulator. Cloned because that evaluator drains its pack.
    let mut io_scan_findings = Vec::new();
    if config.profile_rules {
        for pack in &mut enabled_packs.clone() {
            crate::analyze::eval_io_scan_pack_timed(
                pack,
                &io_scan_ctx,
                &mut rule_time,
                &mut io_scan_findings,
            );
        }
    } else {
        for pack in &enabled_packs {
            zzop_core::eval_pack_io_scan(pack, &io_scan_ctx, &mut io_scan_findings);
        }
    }
    crate::pipeline::findings::append_disable_hints(&mut io_scan_findings);

    let cycles = circular_from_dep_excluding(&dep, &noncycle_edges);
    let dep_stats = dep_stats_from_dep(&dep);
    // Every `FileProjection` is, by construction, a parsed-source file (an external parser only ever
    // projects source it understood) — so `is_source` is unconditionally true here, unlike
    // `analyze::assemble`'s dispatch-backed classifier.
    let nodes = zzop_core::build_file_nodes(
        &dep_stats,
        &GitStats::default(),
        &loc_by_path,
        &DEFAULT_WEIGHTS,
        |_| true,
    );

    // `AnalyzeOutput::folders` is not git-gated, so envelope mode gets a real rollup too.
    // `layer_co_churn` stays `None`: envelope mode never has real commit history.
    let folders = Some(zzop_metrics::build_folder_aggregates(
        &nodes,
        &dep,
        zzop_metrics::DEFAULT_FOLDER_DEPTH,
    ));

    if let Some(w) = reserved_drop_warning("envelope", &envelope.parser, reserved_dropped) {
        warnings.push(w);
    }
    if let Some(w) = crate::analyze::zero_packs_warning(config) {
        warnings.push(w);
    }
    if config.git.is_some() {
        warnings.push(
            "git collection skipped: envelope mode has no filesystem root to collect history from"
                .to_string(),
        );
    }
    // Config-diagnostics parity with `analyze::assemble` — the envelope path used to skip these, so a
    // `disabled_rules` typo or a dead suppression/top-level-exclude filter was silently ineffective in
    // envelope mode only (the "envelope diagnostics asymmetry" gap). `commits` is empty and `git_active`
    // false: envelope mode never has history, and `build_diagnostics` skips every git-window warning on
    // that gate, so only the structural coverage-gap + unknown-`disabled_rules` self-reports fire.
    let diagnostics_report =
        crate::analyze::run_diagnostics(file_count, &dep, &all_symbols, &[], config, false);
    warnings.extend(diagnostics_report.warnings);
    let config_warnings = diagnostics_report.config_warnings;
    let rels: Vec<&str> = loc_by_path.keys().map(String::as_str).collect();
    warnings.extend(crate::analyze::unmatched_suppression_warnings(
        config, &rels,
    ));
    warnings.extend(crate::analyze::unmatched_global_exclude_warnings(
        config, &rels,
    ));
    // One census, two consumers (see `compute_dsl_scope`): the warning below and `packs_loaded`'s counts.
    let dsl_scope = crate::analyze::compute_dsl_scope(&config.packs, &rels);
    // Both of that census's reports are config-derived, so Mode A gets the identical pair Mode B does.
    warnings.extend(crate::analyze::pack_scope_warnings(config, &dsl_scope));
    // Same disclosure Mode B's native twin makes (`analyze::assemble`) — an uncompilable rule is dead in
    // envelope mode too, and a caller who injected the pack inline never ran `validate-rule-pack` on it.
    warnings.extend(crate::analyze::uncompilable_rule_warnings(&config.packs));

    // Whole-graph native analyses (`circular`/`unreachable`/`dead-candidates`) — extracted verbatim
    // to `super::native_pass` (line cap); same gates, same timing ids, same order.
    let profile = config.profile_rules;
    let mut global_findings = super::native_pass::run_whole_graph_native(
        envelope,
        config,
        &cycles,
        &nodes,
        &dep,
        &mut rule_time,
    );

    degraded.sort();
    // Config-declared topology onto both channels, then the sort + freeze — one seam because the
    // ordering constraint lives BETWEEN them. See `super::topology_freeze`. Runs BEFORE the
    // call-graph pass below (which used to sit nowhere — findings were merged first) so that pass
    // reads the same post-mount http keys the native `run_callgraph_rules` sees (native applies
    // config mounts in `assemble`'s provides phase, before its rules phase).
    let io = super::topology_freeze::apply_topology_and_freeze(
        io_provides,
        io_consumes,
        config,
        &mut warnings,
    );

    // Mode A call-graph pass — the consumer of the envelope's `calls` channel (`FileProjection::
    // calls`): builds the whole-tree `SymbolGraph` from the envelope's own edges and runs the
    // call-graph-BFS rules the "No filesystem root" bullet used to rule out entirely. An envelope
    // WITHOUT the channel keeps the old behavior (those rules silent), now disclosed rather than
    // mute — see `super::callgraph`'s module doc for the pass, its disclosures, and its documented
    // deviations from the native pass.
    if let Some(io_facts) = io.as_ref() {
        super::callgraph::run_envelope_callgraph(
            &files,
            &all_paths,
            &all_symbols,
            &io_facts.provides,
            &attribute_store,
            config,
            profile,
            &mut rule_time,
            &mut global_findings,
            &mut warnings,
        );
    }

    let findings = merge_findings(
        vec![per_file_findings, global_findings, io_scan_findings],
        &config.rule_config,
    );

    let ir = CommonIr {
        source: config.source_id.clone(),
        parser: envelope.parser.clone(),
        ir: MinimalIr {
            dep,
            symbols: all_symbols,
            loc: loc_by_path,
            io,
        },
    };

    // `parser_dispatched == file_count`: every envelope file is adapter-declared source (see that field's doc).
    let coverage = crate::CoverageCensus::compute(file_count, file_count, &ir, degraded.len());

    let package_imports = crate::PackageImportSummary::census(package_import_files);

    AnalyzeOutput {
        ir,
        findings,
        degraded,
        file_count,
        coverage,
        package_imports,
        attributes: attribute_store,
        nodes,
        scores: None,
        health: None,
        recommendations: Vec::new(),
        critical: Vec::new(),
        seams: Vec::new(),
        folders,
        layer_co_churn: None,
        packs_loaded: crate::PackLoaded::from_config(config, &dsl_scope.files_in_scope_by_pack),
        warnings,
        config_warnings,
        cache: None,
        rule_timings: profile.then(|| crate::analyze::sort_rule_timings(rule_time)),
        rule_overrides_applied: crate::analyze::rule_overrides_applied(config),
        // Envelope mode (Mode A) never runs git collection — no real tree to walk — so this stays
        // `None` exactly like `scores`/`health`/`critical`/`seams` above.
        git_window: None,
    }
}
