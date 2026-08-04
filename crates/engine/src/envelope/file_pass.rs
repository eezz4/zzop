//! Mode A's per-file accumulation loop — extracted verbatim from `analyze_envelope` (see `ingest`)
//! so the orchestrator and this pass each fit their own file. Everything the loop accumulates crosses
//! back via [`FilePassState`]; behavior is identical to the pre-split inline loop.

use std::collections::{BTreeMap, HashMap, HashSet};

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
    /// Each file's own `const_map_fragment` (dotted-constant-name -> literal string value, e.g.
    /// `"ControlKey.AUTHEN.getUserInfo" -> "/authen/getUserInfo"`), paired with its path — despite the
    /// short name this is NOT a code/AST fragment, it's specifically the late-cross-file-consume-
    /// resolution substrate `analyze::late_resolve_cross_file_consumes` re-resolves unresolved
    /// `IoConsume`s against below.
    pub(super) const_fragment_pairs: Vec<(String, HashMap<String, String>)>,
    /// Each file's declared class/interface shapes, path-paired — the `body`/`response` dtoRef
    /// resolution substrate (`super::shapes`), collected exactly as `analyze::assemble::collect`
    /// gathers the native artifacts' `class_shape_fragments`.
    pub(super) class_shape_pairs: Vec<(String, Vec<zzop_core::ClassShapeFragment>)>,
    /// Same summary `analyze::assemble` builds natively — see `AnalyzeOutput::package_imports`.
    pub(super) package_import_files:
        std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
    /// Aggregate reserved-sentinel drop count across every file in this envelope — reported as ONE
    /// `warnings` entry by the orchestrator (via `reserved_drop_warning`), not per-file, mirroring
    /// `apply_adapter_overlays`'s own per-overlay aggregation. See the in-loop comment for why these
    /// are dropped at all.
    pub(super) reserved_dropped: usize,
    /// `EngineConfig::profile_rules` accumulator — the envelope-mode counterpart of
    /// `analyze::assemble`'s per-artifact `rule_timings` reduce: per-rule `(nanos, findings)` summed
    /// across every file, same `"{pack}/{rule}"` keys `record_native_timing`'s callers use, finalized
    /// by `sort_rule_timings` in the orchestrator. Stays empty when profiling is off.
    pub(super) rule_time: HashMap<String, (u128, usize)>,
}

/// The per-file pass of `analyze_envelope`: per-file fact collection, hand-built dep-graph edges, and the
/// per-file `SymbolScan` DSL rule evaluation (`IoScan` no-ops here — it evaluates whole-tree, run once by
/// `ingest::analyze_envelope` after this pass returns; see `zzop_core::dsl::eval_pack_io_scan`'s doc).
/// `files` must already be `path`-sorted (the orchestrator sorts) so every accumulated `Vec` is
/// deterministic.
pub(super) fn run_file_pass(
    files: &[&zzop_core::FileProjection],
    all_paths: &HashSet<&str>,
    enabled_packs: &[RulePackDef],
    profile: bool,
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
        class_shape_pairs: Vec::new(),
        package_import_files: std::collections::BTreeMap::new(),
        reserved_dropped: 0usize,
        rule_time: HashMap::new(),
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
        if !file.class_shape_fragments.is_empty() {
            state
                .class_shape_pairs
                .push((file.path.clone(), file.class_shape_fragments.clone()));
        }

        // Every file gets a `dep` entry (even an empty edge list) so `dep_stats_from_dep` downstream
        // counts it as a graph node, letting an isolated (import-free) file still get a `FileNode`.
        let mut seen = HashSet::new();
        let mut targets = Vec::new();
        // target -> true iff EVERY edge resolving to it so far is excludable from cycle detection
        // (type-only, or a dynamic import) — mirrors
        // `zzop_parser_typescript::lang::resolve::build_dep_impl`'s own aggregation, folded in here since
        // envelope mode builds `dep` by hand rather than calling that shared helper.
        // BTreeMap, not HashMap: this map is drained into `state.noncycle_edges` below, and an
        // ordered walk removes the question of whether that drain order matters at all.
        let mut target_noncycle: BTreeMap<String, bool> = BTreeMap::new();
        for binding in file.imports.values() {
            // Non-relative specifier naming no projected file = a package import — summarized for
            // `cross-layer/untraced-client-import-no-visible-consume`.
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
            // Same plumbing rationale as `loop_spans` directly above (carry the real fact, never a
            // hardcoded placeholder), and likewise inert in envelope mode today.
            function_spans: file.function_spans.clone(),
            // Unlike the two above this one is NOT inert here: the test-region gate (`zzop_core::dsl::
            // eval`'s `TestRegions`) runs over whatever this per-file pass produces, which in envelope
            // mode is symbol-scan, so a producer that declares `test_spans` gets its fixtures excluded in
            // Mode B too. Mode B and Mode A must not disagree about what counts as shipped code.
            //
            // Two edges this pass does NOT cover, both stated at the wire field
            // (`docs/adapters/envelope.schema.json`, `docs/NORMALIZED_AST.md`) so a producer is not left
            // inferring them: a rule declaring `scan_test_regions` keeps judging these spans on purpose
            // (credential-at-rest rules — the commit is the leak), and `Matcher::IoScan` is evaluated
            // whole-tree by `ingest`'s own `eval_pack_io_scan` call, over assembled facts this
            // `SourceFile` is not part of. The producer withholds test-region io facts instead; gating
            // them here would clean the findings and leave the same route in the join.
            test_spans: file.test_spans.clone(),
            // Always empty in Mode A/B: `FileProjection` has no call-SITE channel on the wire today.
            // Its `calls` channel is NOT that — those are call-graph EDGES (a different fact category,
            // consumed by `super::callgraph`'s BFS pass, never by `Matcher::CallScan`) — and opening a
            // per-file call-SITE channel on the external contract is its own additive, version-gated
            // change. Consequence, stated so it cannot be discovered by surprise: a `CallScan` rule is
            // silent on every envelope-projected file, the same recall-side degrade a language with no
            // native producer gets.
            call_sites: Vec::new(),
            // Always empty in Mode A/B — the boundary here is PRIVACY (unsalted 64-bit hashes of
            // candidate SECRETS must not ride an external submission), not only additive-change
            // discipline: see docs/NORMALIZED_AST.md "Channels deliberately NOT on this wire".
            string_literals: Vec::new(),
            rel: file.path.clone(),
            text: String::new(),
            symbols: file.symbols.clone(),
            io: Some(file.io.clone()),
        };
        let ctx_files = std::slice::from_ref(&source_file);
        let ctx = RuleContext { files: ctx_files };
        for pack in enabled_packs {
            if pack_loader::applies_to(pack, &file.path) {
                // Same profiled/unprofiled branch `pipeline::findings::eval_packs` uses: an unprofiled
                // run keeps the exact whole-pack call it always made (no `Instant::now()` cost), a
                // profiled one goes through `eval_pack_profiled` and folds each `RuleTiming` into the
                // SAME per-rule accumulator shape `analyze::assemble`'s reduce step sums — findings are
                // byte-identical either way (profiling never changes what runs).
                let mut found = if profile {
                    let (found, timings) = zzop_core::dsl::eval_pack_profiled(pack, &ctx);
                    for t in timings {
                        let entry = state.rule_time.entry(t.rule_id).or_insert((0, 0));
                        entry.0 += t.nanos;
                        entry.1 += t.findings;
                    }
                    found
                } else {
                    eval_pack(pack, &ctx)
                };
                // D13①: same config-disable-hint append `pipeline::findings::eval_packs` does for Mode
                // A — via the SAME shared helper (never a second hand-written hint template). Mode B has
                // no on-disk cache (see `envelope.rs`'s module doc), so this never touches
                // `CACHE_SCHEMA_VERSION`'s contract the way the Mode A call site does.
                crate::pipeline::findings::append_disable_hints(&mut found);
                state.per_file_findings.extend(found);
            }
        }
    }

    state
}
