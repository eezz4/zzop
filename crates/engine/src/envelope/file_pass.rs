//! Mode A's per-file accumulation loop — extracted verbatim from `analyze_envelope` (see `ingest`)
//! so the orchestrator and this pass each fit their own file. Everything the loop accumulates crosses
//! back via [`FilePassState`]; behavior is identical to the pre-split inline loop.

use std::collections::{HashMap, HashSet};

use zzop_core::{
    eval_pack, pack_loader, DepGraph, Finding, IoConsume, IoProvide, RuleContext, RulePackDef,
    SourceFile,
};

use super::reserved::{is_reserved_consume_kind, is_reserved_provide_kind};

/// Everything [`run_file_pass`] accumulates across the envelope's files, handed back to
/// `analyze_envelope` for the whole-graph phases (fragment composition, cycle detection, io freeze,
/// output assembly). Field-by-field this is exactly the set of `let mut` locals the pre-split loop
/// wrote into.
pub(super) struct FilePassState {
    pub(super) loc_by_path: HashMap<String, u32>,
    pub(super) degraded: Vec<String>,
    pub(super) all_symbols: Vec<zzop_core::SourceSymbol>,
    pub(super) io_provides: Vec<IoProvide>,
    pub(super) io_consumes: Vec<IoConsume>,
    pub(super) dep: DepGraph,
    /// Ephemeral noncycle-exclusion set (see `circular_from_dep_excluding`'s doc) — never
    /// cached/serialized, lives only for this one `analyze_envelope` call. A `(from, to)` pair lands here
    /// when EVERY edge contributing that target is excludable from cycle detection (type-only binding/
    /// re-export, or a dynamic import); a pair with at least one plain value edge to the same target is
    /// never inserted, so it still counts toward `cycles` downstream.
    pub(super) noncycle_edges: HashSet<(String, String)>,
    pub(super) per_file_findings: Vec<Finding>,
    /// Fragment-composition substrate — the envelope-mode counterpart of `analyze::assemble`'s own
    /// `trpc_fragment_pairs`/`router_mount_pairs`/`fragment_pairs`: collected during the per-file loop,
    /// composed once after (path-paired so composition can sort for deterministic first-writer-wins).
    pub(super) trpc_fragment_pairs: Vec<(String, Vec<zzop_core::ProcedureRouterFragment>)>,
    pub(super) router_mount_pairs: Vec<(String, Vec<zzop_core::RouterMountFragment>)>,
    pub(super) const_fragment_pairs: Vec<(String, HashMap<String, String>)>,
    /// Same summary `analyze::assemble` builds natively — see `AnalyzeOutput::package_imports`.
    pub(super) package_import_files:
        std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
    /// Aggregate reserved-sentinel drop count across every file in this envelope — reported as ONE
    /// `warnings` entry by the orchestrator (via `reserved_drop_warning`), not per-file, mirroring
    /// `apply_adapter_overlays`'s own per-overlay aggregation. See the in-loop comment for why these
    /// are dropped at all.
    pub(super) reserved_dropped: usize,
}

/// The per-file pass of `analyze_envelope`: per-file fact collection, hand-built dep-graph edges, and
/// the SymbolScan/IoScan-only DSL rule evaluation. `files` must already be `path`-sorted (the
/// orchestrator sorts) so every accumulated `Vec` is deterministic.
pub(super) fn run_file_pass(
    files: &[&zzop_core::FileProjection],
    all_paths: &HashSet<&str>,
    enabled_packs: &[RulePackDef],
) -> FilePassState {
    let mut state = FilePassState {
        loc_by_path: HashMap::new(),
        degraded: Vec::new(),
        all_symbols: Vec::new(),
        io_provides: Vec::new(),
        io_consumes: Vec::new(),
        dep: DepGraph::new(),
        noncycle_edges: HashSet::new(),
        per_file_findings: Vec::new(),
        trpc_fragment_pairs: Vec::new(),
        router_mount_pairs: Vec::new(),
        const_fragment_pairs: Vec::new(),
        package_import_files: std::collections::BTreeMap::new(),
        reserved_dropped: 0usize,
    };

    for file in files {
        state.loc_by_path.insert(file.path.clone(), file.loc);
        if file.degraded {
            state.degraded.push(file.path.clone());
        }
        state.all_symbols.extend(file.symbols.iter().cloned());
        // Reserved ENGINE-INTERNAL sentinel kinds are dropped at ingestion: envelope mode never runs
        // the native assemble seams that consume+strip them (`apply_and_strip_global_prefix`,
        // `apply_client_base_prefixes`), so an external producer emitting one of these kinds would
        // otherwise leak a raw sentinel into `MinimalIr::io`/rules instead of getting the native rewrite
        // semantics. Dropping is still the right degrade, but it is no longer SILENT (opus NOTE,
        // axios-defaults-base-v1, superseded): a dropped-but-unwarned sentinel left an external-parser
        // producer with no way to learn its `nest-global-prefix`/`client-base-prefix` entry vanished, the
        // asymmetry Mode B closed for overlays first (1a70aae) — the count is aggregated in `state` and
        // reported as one `warnings` entry per envelope by the orchestrator, parallel to that fix.
        // Filters shared with `apply_adapter_overlays`'s own Mode B filter (`is_reserved_provide_kind`/
        // `is_reserved_consume_kind`) so the two modes can't drift on which kinds are reserved.
        state.reserved_dropped += file
            .io
            .provides
            .iter()
            .filter(|p| is_reserved_provide_kind(&p.kind))
            .count();
        state.reserved_dropped += file
            .io
            .consumes
            .iter()
            .filter(|c| is_reserved_consume_kind(&c.kind))
            .count();
        state.io_provides.extend(
            file.io
                .provides
                .iter()
                .filter(|p| !is_reserved_provide_kind(&p.kind))
                .cloned(),
        );
        state.io_consumes.extend(
            file.io
                .consumes
                .iter()
                .filter(|c| !is_reserved_consume_kind(&c.kind))
                .cloned(),
        );
        if !file.procedure_router_fragments.is_empty() {
            state
                .trpc_fragment_pairs
                .push((file.path.clone(), file.procedure_router_fragments.clone()));
        }
        if !file.router_mount_fragments.is_empty() {
            state
                .router_mount_pairs
                .push((file.path.clone(), file.router_mount_fragments.clone()));
        }
        if !file.const_map_fragment.is_empty() {
            state
                .const_fragment_pairs
                .push((file.path.clone(), file.const_map_fragment.clone()));
        }

        // Every file gets a `dep` entry (even an empty edge list) so `dep_stats_from_dep` downstream
        // counts it as a graph node, letting an isolated (import-free) file still get a `FileNode`.
        let mut seen = HashSet::new();
        let mut targets = Vec::new();
        // target -> true iff EVERY edge resolving to it so far is excludable from cycle detection
        // (type-only, or a dynamic import) — mirrors
        // `zzop_parser_typescript::lang::resolve::build_dep_impl`'s own aggregation, folded in here since
        // envelope mode builds `dep` by hand rather than calling that shared helper.
        let mut target_noncycle: HashMap<String, bool> = HashMap::new();
        for binding in file.imports.values() {
            // Non-relative specifier naming no projected file = a package import — summarized for
            // `cross-layer/sdk-import-no-visible-consume`.
            if !binding.specifier.starts_with('.')
                && !binding.specifier.starts_with('/')
                && !all_paths.contains(binding.specifier.as_str())
            {
                state
                    .package_import_files
                    .entry(binding.specifier.clone())
                    .or_default()
                    .insert(file.path.clone());
            }
            if binding.deferred {
                continue; // lazy import: no module-load edge.
            }
            if binding.specifier != file.path && all_paths.contains(binding.specifier.as_str()) {
                target_noncycle
                    .entry(binding.specifier.clone())
                    .and_modify(|all| *all &= binding.type_only)
                    .or_insert(binding.type_only);
                if seen.insert(binding.specifier.clone()) {
                    targets.push(binding.specifier.clone());
                }
            }
        }
        // Defect A/1 (envelope parity): fold each re-export's specifier in too, mirroring
        // `zzop_parser_typescript::lang::resolve::build_dep_impl`'s own re-export merge — a barrel
        // `export { x } from './impl'` with no local import of `impl` must still give `impl` a dep edge
        // (fan-in), or `dead-candidates` false-positives it. A type-only re-export (Defect 1) now gets
        // the same edge-but-excluded-from-cycles treatment as a type-only import binding, rather than
        // being dropped entirely.
        for re in &file.re_exports {
            if re.specifier != file.path && all_paths.contains(re.specifier.as_str()) {
                target_noncycle
                    .entry(re.specifier.clone())
                    .and_modify(|all| *all &= re.type_only)
                    .or_insert(re.type_only);
                if seen.insert(re.specifier.clone()) {
                    targets.push(re.specifier.clone());
                }
            }
        }
        // Defect 2 (envelope parity): a dynamic `import()` specifier gives its target fan-in but is
        // never a synchronous-load cycle edge — always excludable, mirroring `build_dep_impl`'s own
        // dynamic-import handling.
        for spec in &file.dynamic_imports {
            if spec != &file.path && all_paths.contains(spec.as_str()) {
                target_noncycle.entry(spec.clone()).or_insert(true);
                if seen.insert(spec.clone()) {
                    targets.push(spec.clone());
                }
            }
        }
        for (target, all_noncycle) in target_noncycle {
            if all_noncycle {
                state.noncycle_edges.insert((file.path.clone(), target));
            }
        }
        state.dep.insert(file.path.clone(), targets);

        // Per-file DSL pass — symbol-scan/io-scan only (see the envelope module doc). `text` is empty
        // since an envelope carries no source lines.
        let source_file = SourceFile {
            // Plumbed straight from the producer's projection (empty when absent, via `#[serde(default)]`
            // on `FileProjection::loop_spans`) — currently inert in envelope mode regardless, since
            // `envelope_rule_pack` only keeps `SymbolScan`/`IoScan` matchers (see the envelope module
            // doc: method-scan rules never run without source text), but this field should carry the
            // real fact rather than a hardcoded placeholder.
            loop_spans: file.loop_spans.clone(),
            rel: file.path.clone(),
            text: String::new(),
            symbols: file.symbols.clone(),
            io: Some(file.io.clone()),
        };
        let ctx_files = std::slice::from_ref(&source_file);
        let ctx = RuleContext {
            files: ctx_files,
            ir: None,
        };
        for pack in enabled_packs {
            if pack_loader::applies_to(pack, &file.path) {
                // D13①: same config-disable-hint append `pipeline::findings::eval_packs` does for Mode
                // A — via the SAME shared helper (never a second hand-written hint template). Mode B has
                // no on-disk cache (see `envelope.rs`'s module doc), so this never touches
                // `CACHE_SCHEMA_VERSION`'s contract the way the Mode A call site does.
                let mut found = eval_pack(pack, &ctx);
                crate::pipeline::findings::append_disable_hints(&mut found);
                state.per_file_findings.extend(found);
            }
        }
    }

    state
}
