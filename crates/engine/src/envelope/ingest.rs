//! Mode A orchestrator — `analyze_envelope`, the whole-tree envelope entry point. The per-file
//! accumulation loop lives in `file_pass` (extracted verbatim); this module keeps the pack gating,
//! fragment composition, whole-graph analyses, warnings, and output assembly, in the same order as
//! the pre-split single function.

use std::collections::HashSet;

use zzop_core::{
    circular_from_dep_excluding, is_enabled, merge_findings, registry, CommonIr, GitStats, IoFacts,
    MinimalIr, NormalizedEnvelope, RulePackDef, DEFAULT_WEIGHTS,
};

use crate::analyze::{
    circular_findings, dead_candidate_findings, dep_stats_from_dep, unreachable_findings,
};
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
        .filter(|p| registry::is_enabled(&config.rule_config, &p.id))
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
        package_import_files,
        reserved_dropped,
    } = run_file_pass(&files, &all_paths, &enabled_packs);

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
    // `run_callgraph_rules`/`schema_usage_findings` (see the envelope module doc, "No filesystem root
    // -> no ... call-graph-BFS rules") and never joins `analyze_trees`' cross-layer stage, so no rule
    // reads this store TODAY — but the field is part of `AnalyzeOutput`'s plumbing contract now
    // (the cross-layer idempotency veto reads it for filesystem trees), and silently dropping an
    // envelope's injected attributes here would make this mode's output lie about them.
    let mut native_attrs: Vec<zzop_core::Attribute> = Vec::new();
    if !router_mount_pairs.is_empty() {
        let (composed, attrs) = crate::analyze::compose_router_mount_provides(
            router_mount_pairs,
            |specifier, from_file, _ident| {
                resolve_envelope_specifier(specifier, from_file, &all_paths)
            },
        );
        io_provides.extend(composed);
        native_attrs = attrs;
    }
    let attribute_store =
        zzop_core::AttributeStore::from_parts(native_attrs, std::slice::from_ref(envelope));
    crate::analyze::late_resolve_cross_file_consumes(const_fragment_pairs, &mut io_consumes);

    // Whole-tree `Matcher::IoScan` DSL pass — the envelope-mode counterpart of `analyze::assemble`'s own
    // (native-path) call, run here now that `io_provides`/`io_consumes`/`attribute_store` above all exist.
    // `anchor_line` is always `None`: an envelope carries no source text (see this module's doc, "No
    // source text" bullet), so `anchor_exclude_pattern`/suppress-marker recognition stay honestly inactive
    // — the same "no info available, never a guess" contract every `None` callback result gets in
    // `eval_pack_io_scan`'s own doc. No decorator-guard minting here: Mode A never runs
    // `run_callgraph_rules` (this module's doc, "No filesystem root" bullet), so there is no
    // `decorator_guarded` evidence to mint from — `attribute_store` is used as-is. `enabled_packs` is
    // already the same is_enabled/`gate_pack_rules`/`envelope_rule_pack`-gated pack list `run_file_pass`
    // evaluated per-file above; reused here unchanged for the whole-tree pass.
    let anchor_line = |_: &str, _: u32| None;
    let io_scan_ctx = zzop_core::IoScanTreeContext {
        provides: &io_provides,
        consumes: &io_consumes,
        attrs: &attribute_store,
        anchor_line: &anchor_line,
    };
    let mut io_scan_findings = Vec::new();
    for pack in &enabled_packs {
        zzop_core::eval_pack_io_scan(pack, &io_scan_ctx, &mut io_scan_findings);
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

    let mut warnings = Vec::new();
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
    // One census, two consumers — same seam as `analyze::assemble`'s (see `compute_dsl_scope`'s doc):
    // the zero-applicability warning below and `packs_loaded`'s per-pack `files_in_scope` count.
    let dsl_scope = crate::analyze::compute_dsl_scope(&config.packs, &rels);
    if let Some(w) = crate::analyze::no_applicable_dsl_rule_warning(&config.packs, &dsl_scope) {
        warnings.push(w);
    }

    let mut global_findings = Vec::new();
    if is_enabled(&config.rule_config, "circular") {
        global_findings.extend(circular_findings(&cycles));
    }
    if is_enabled(&config.rule_config, "unreachable") {
        // No filesystem root here (see the envelope module doc), so there are no cargo manifests to
        // scan for declared-target entries — the empty set is the honest Mode-A value, same rationale
        // as `dead-candidates`' empty package.json entry set just below.
        global_findings.extend(unreachable_findings(&nodes, &dep, &Default::default()));
    }
    if is_enabled(&config.rule_config, "dead-candidates") {
        // No filesystem root (see the envelope module doc) -> no package.json-referenced entries; the
        // envelope's own `is_entry`-marked projections ARE the entry set — the Mode A counterpart of the
        // Mode B overlay union in `analyze::assemble` (same contract marker, same exemption). Before
        // this, Mode A silently dropped `is_entry` and every convention-loaded entry file (a crate's
        // `lib.rs`, a test harness file) read as dead — caught by the rust-parser-adapter example's
        // self-analysis.
        let extra_entries: HashSet<String> = envelope
            .files
            .iter()
            .filter(|f| f.is_entry)
            .map(|f| f.path.clone())
            .collect();
        // Deliberate divergence from the native `assemble::rules` path: it post-filters out generated
        // (`@generated`/auto-generated-bannered) files via `generated_banner::file_has_generated_banner`,
        // which re-reads each candidate's head off disk. Mode A has no filesystem `root` and a
        // `FileProjection` (normalized.rs) carries no raw text, so that head-comment detector structurally
        // cannot run here — an adapter that wants a generated file exempt marks it `is_entry` (above) or
        // omits it from the envelope. Documented, not a bug: the exemption is a native-path-only refinement.
        global_findings.extend(dead_candidate_findings(&nodes, &dep, &extra_entries));
    }

    let findings = merge_findings(
        vec![per_file_findings, global_findings, io_scan_findings],
        &config.rule_config,
    );

    // Deployment-topology mount apply (`EngineConfig::mounts`, config-declared) — the Mode A counterpart
    // of `analyze::assemble`'s own call (`analyze/mod.rs`'s placement doc, ~line 397, which this mirrors):
    // must run AFTER every provide-composing step above (tRPC/router-mount fragment composition,
    // `compose_trpc_provides`/`compose_router_mount_provides`) so a config mount covers every http provide
    // this mode ever produces, and BEFORE `io_provides` is sorted/frozen into `MinimalIr::io` just below —
    // deployment topology is origin-agnostic (same rationale Mode B's overlay provides receive mounts
    // under, in `analyze::assemble`), so a tree analyzed via Mode A must not silently freeze un-mounted
    // keys while the native path mounts the same config. See `compose::apply_config_mounts`'s own doc for
    // the winner-selection/validation/zero-effect-tripwire rules.
    crate::analyze::apply_config_mounts(&mut io_provides, &config.mounts, &mut warnings);

    degraded.sort();
    io_provides.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| a.key.cmp(&b.key))
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
    });
    io_consumes.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| a.key.cmp(&b.key))
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
    });
    let io = if io_provides.is_empty() && io_consumes.is_empty() {
        None
    } else {
        Some(IoFacts {
            provides: io_provides,
            consumes: io_consumes,
        })
    };

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

    let coverage = crate::CoverageCensus::compute(file_count, &ir, degraded.len());

    let package_imports = package_import_files
        .into_iter()
        .map(|(specifier, files)| crate::PackageImportSummary {
            file_count: files.len(),
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
        rule_timings: None,
        rule_overrides_applied: crate::analyze::rule_overrides_applied(config),
        // Envelope mode (Mode A) never runs git collection — no real tree to walk — so this stays
        // `None` exactly like `scores`/`health`/`critical`/`seams` above.
        git_window: None,
    }
}
