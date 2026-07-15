//! Envelope ingestion — the engine-side receiver for the external-parser Normalized AST protocol
//! (`docs/NORMALIZED_AST.md`). Projects a `zzop_core::NormalizedEnvelope`'s `FileProjection`s into the
//! same per-file shape `analyze::assemble` consumes, then runs the same whole-graph analyses
//! (dep-graph resolution, `circular`/`unreachable`/`dead-candidates`, `merge_findings`). An external
//! parser (Java/Python/JSP/anything this engine cannot parse natively) is therefore a first-class
//! citizen of every language-neutral analysis — the engine never sees the external parser's own AST,
//! only this projection.
//!
//! ## Deviations from the native per-file pass (documented, not bugs)
//!
//! - **No source text -> line-scan/method-scan DSL rules never run.** Those matchers scan source text
//!   directly; evaluating them against an empty string would silently look like "ran, found nothing"
//!   instead of "did not run". `SymbolScan`/`IoScan` only read `symbols`/`io`, which a `FileProjection`
//!   does supply, so `envelope_rule_pack` filters every pack down to just those two matcher kinds.
//!   Per-file lexical rules belong on the external parser's own side of the boundary.
//! - **No filesystem root -> no `dead-exports`/call-graph-BFS rules, no git-history analyses.** Those
//!   need a second disk read or a repository root, which an envelope has neither of; the affected
//!   `AnalyzeOutput` fields stay at their "git inactive" empty value, and a configured `git` option
//!   produces one `warnings` entry rather than a panic.
//! - **Dep-graph resolution treats import specifiers as repo-relative.** Edge resolution is a plain
//!   exact match against the envelope's own path set, not the TS parser's relative/extension-guessing
//!   resolver — an arbitrary external parser's `imports` map has no reason to follow TS conventions. An
//!   unmatched specifier is external, never an error; a `deferred` binding gets no edge (lazy import).
//!   [`resolve_envelope_specifier`] is a separate, narrower resolver used only for fragment
//!   `Ref`/`Mount` specifiers, which additionally understands `./`/`../` joins.
//! - **Fragment composition** (tRPC PROVIDEs, router-mount PROVIDEs) and late const-map CONSUME
//!   re-resolution run in envelope mode too, via the same composer functions the native path uses —
//!   only the resolver differs, since an envelope carries no tsconfig or workspace manifests to alias
//!   against.
//! - **No caching, no rule-timing profiling.** Both are ignored — envelope mode has no per-file disk
//!   content to hash and no per-rule timing loop wired for this smaller rule surface.

use std::collections::{HashMap, HashSet};

use zzop_core::{
    circular_from_dep_excluding, eval_pack, is_enabled, merge_findings, pack_loader, registry,
    CommonIr, DepGraph, Finding, GitStats, IoConsume, IoFacts, IoProvide, Matcher, MinimalIr,
    NormalizedEnvelope, RuleContext, RulePackDef, SourceFile, DEFAULT_WEIGHTS,
};

use crate::analyze::{
    circular_findings, dead_candidate_findings, dep_stats_from_dep, unreachable_findings,
};
use crate::{AnalyzeOutput, EngineConfig};

/// True iff `kind` is a reserved, engine-internal `IoProvide` sentinel that only
/// `zzop_parser_typescript::adapters::global_prefix` (native TS) may produce, and only
/// `compose::apply_and_strip_global_prefix` (the native `analyze::assemble` pipeline) may consume+strip —
/// see that pair's docs. A producer feeding this engine any other way (an envelope's `FileProjection`,
/// Mode A or Mode B) must never emit it: envelope/overlay ingestion never runs that consuming seam, so a
/// leaked sentinel would either surface raw in output/rules (Mode A) or get re-applied against the WHOLE
/// native tree by that seam once merged (Mode B) — an external overlay author re-prefixing every native
/// route by accident. Both `analyze_envelope` (Mode A, above) and `apply_adapter_overlays` (Mode B, below)
/// call this — kept as one predicate so the two modes can't drift on which kinds are reserved.
///
/// Bound to the parser's exported const (not a local literal) so a rename on the emit side cannot
/// silently desynchronize this check — a leaked sentinel would reach output.
fn is_reserved_provide_kind(kind: &str) -> bool {
    kind == zzop_parser_typescript::NEST_GLOBAL_PREFIX_KIND
}

/// True iff `kind` is the `IoConsume` counterpart of [`is_reserved_provide_kind`] — the client-base-prefix
/// sentinel only `zzop_parser_typescript::adapters::client_base` may produce and only
/// `compose::apply_client_base_prefixes` may consume+strip. Same producer-forbidden rationale.
fn is_reserved_consume_kind(kind: &str) -> bool {
    kind == zzop_parser_typescript::CLIENT_BASE_PREFIX_KIND
}

/// Builds the one aggregate "reserved sentinel(s) dropped" warning shared by both modes — `Some` iff
/// `dropped > 0`. `subject_kind` is the noun phrase (`"envelope"` for Mode A, `"adapter overlay"` for
/// Mode B) and `subject_id` is that mode's own identifier (`NormalizedEnvelope::parser` in both cases,
/// since Mode B's overlays ARE `NormalizedEnvelope`s too — an envelope's `source` is not used here since
/// `parser` is what a producer actually recognizes as "mine"). Centralizing the count->message step here
/// (rather than duplicating the singular/plural + kind-list text in each call site) is what keeps the two
/// modes' wording from drifting apart the way `is_reserved_provide_kind`/`is_reserved_consume_kind` keep
/// them from drifting on WHICH kinds are reserved.
fn reserved_drop_warning(subject_kind: &str, subject_id: &str, dropped: usize) -> Option<String> {
    if dropped == 0 {
        return None;
    }
    let entries = if dropped == 1 { "entry" } else { "entries" };
    // Built from the two producers' own exported consts (not hardcoded literals) so this text can never
    // drift from the real kinds `is_reserved_provide_kind`/`is_reserved_consume_kind` check — the
    // rendered string is unchanged from before (both consts equal the literals this format! replaces).
    let nest_global_prefix_kind = zzop_parser_typescript::NEST_GLOBAL_PREFIX_KIND;
    let client_base_prefix_kind = zzop_parser_typescript::CLIENT_BASE_PREFIX_KIND;
    Some(format!(
        "{subject_kind} '{subject_id}': dropped {dropped} reserved engine-internal io {entries} \
         (kinds `{nest_global_prefix_kind}`/`{client_base_prefix_kind}` are producer-forbidden)"
    ))
}

/// Ingests one `NormalizedEnvelope` (already validated — see `zzop_core::validate_envelope`) and
/// produces the same `AnalyzeOutput` shape `analyze_tree` does, per this module's doc for which
/// analyses run and which are skipped in envelope mode. Files are processed in `path`-sorted order
/// (mirroring `pipeline::run_file_pass`) so output is deterministic regardless of the envelope's own
/// file order.
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

    let mut loc_by_path: HashMap<String, u32> = HashMap::new();
    let mut degraded: Vec<String> = Vec::new();
    let mut all_symbols = Vec::new();
    let mut io_provides: Vec<IoProvide> = Vec::new();
    let mut io_consumes: Vec<IoConsume> = Vec::new();
    let mut dep: DepGraph = DepGraph::new();
    // Ephemeral noncycle-exclusion set (see `circular_from_dep_excluding`'s doc) — never
    // cached/serialized, lives only for this one `analyze_envelope` call. A `(from, to)` pair lands here
    // when EVERY edge contributing that target is excludable from cycle detection (type-only binding/
    // re-export, or a dynamic import); a pair with at least one plain value edge to the same target is
    // never inserted, so it still counts toward `cycles` below.
    let mut noncycle_edges: HashSet<(String, String)> = HashSet::new();
    let mut per_file_findings: Vec<Finding> = Vec::new();
    // Fragment-composition substrate — the envelope-mode counterpart of `analyze::assemble`'s own
    // `trpc_fragment_pairs`/`router_mount_pairs`/`fragment_pairs`: collected during the per-file loop,
    // composed once after (path-paired so composition can sort for deterministic first-writer-wins).
    let mut trpc_fragment_pairs: Vec<(String, Vec<zzop_core::ProcedureRouterFragment>)> =
        Vec::new();
    let mut router_mount_pairs: Vec<(String, Vec<zzop_core::RouterMountFragment>)> = Vec::new();
    let mut const_fragment_pairs: Vec<(String, HashMap<String, String>)> = Vec::new();
    // Same summary `analyze::assemble` builds natively — see `AnalyzeOutput::package_imports`.
    let mut package_import_files: std::collections::BTreeMap<
        String,
        std::collections::BTreeSet<String>,
    > = std::collections::BTreeMap::new();
    // Aggregate reserved-sentinel drop count across every file in this envelope — reported as ONE
    // `warnings` entry below (via `reserved_drop_warning`), not per-file, mirroring `apply_adapter_overlays`'s
    // own per-overlay aggregation. See the in-loop comment for why these are dropped at all.
    let mut reserved_dropped = 0usize;

    for file in &files {
        loc_by_path.insert(file.path.clone(), file.loc);
        if file.degraded {
            degraded.push(file.path.clone());
        }
        all_symbols.extend(file.symbols.iter().cloned());
        // Reserved ENGINE-INTERNAL sentinel kinds are dropped at ingestion: envelope mode never runs
        // the native assemble seams that consume+strip them (`apply_and_strip_global_prefix`,
        // `apply_client_base_prefixes`), so an external producer emitting one of these kinds would
        // otherwise leak a raw sentinel into `MinimalIr::io`/rules instead of getting the native rewrite
        // semantics. Dropping is still the right degrade, but it is no longer SILENT (opus NOTE,
        // axios-defaults-base-v1, superseded): a dropped-but-unwarned sentinel left an external-parser
        // producer with no way to learn its `nest-global-prefix`/`client-base-prefix` entry vanished, the
        // asymmetry Mode B closed for overlays first (1a70aae) — the count is aggregated above and
        // reported as one `warnings` entry per envelope below, parallel to that fix. Filters shared with
        // `apply_adapter_overlays`'s own Mode B filter below (`is_reserved_provide_kind`/
        // `is_reserved_consume_kind`) so the two modes can't drift on which kinds are reserved.
        reserved_dropped += file
            .io
            .provides
            .iter()
            .filter(|p| is_reserved_provide_kind(&p.kind))
            .count();
        reserved_dropped += file
            .io
            .consumes
            .iter()
            .filter(|c| is_reserved_consume_kind(&c.kind))
            .count();
        io_provides.extend(
            file.io
                .provides
                .iter()
                .filter(|p| !is_reserved_provide_kind(&p.kind))
                .cloned(),
        );
        io_consumes.extend(
            file.io
                .consumes
                .iter()
                .filter(|c| !is_reserved_consume_kind(&c.kind))
                .cloned(),
        );
        if !file.procedure_router_fragments.is_empty() {
            trpc_fragment_pairs.push((file.path.clone(), file.procedure_router_fragments.clone()));
        }
        if !file.router_mount_fragments.is_empty() {
            router_mount_pairs.push((file.path.clone(), file.router_mount_fragments.clone()));
        }
        if !file.const_map_fragment.is_empty() {
            const_fragment_pairs.push((file.path.clone(), file.const_map_fragment.clone()));
        }

        // Every file gets a `dep` entry (even an empty edge list) so `dep_stats_from_dep` below counts
        // it as a graph node, letting an isolated (import-free) file still get a `FileNode`.
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
                package_import_files
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
                noncycle_edges.insert((file.path.clone(), target));
            }
        }
        dep.insert(file.path.clone(), targets);

        // Per-file DSL pass — symbol-scan/io-scan only (see module doc). `text` is empty since an
        // envelope carries no source lines.
        let source_file = SourceFile {
            // Plumbed straight from the producer's projection (empty when absent, via `#[serde(default)]`
            // on `FileProjection::loop_spans`) — currently inert in envelope mode regardless, since
            // `envelope_rule_pack` only keeps `SymbolScan`/`IoScan` matchers (see module doc: method-scan
            // rules never run without source text), but this field should carry the real fact rather than
            // a hardcoded placeholder.
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
        for pack in &enabled_packs {
            if pack_loader::applies_to(pack, &file.path) {
                per_file_findings.extend(eval_pack(pack, &ctx));
            }
        }
    }

    // Fragment composition + late const-map consume re-resolution must run before `io_provides`/
    // `io_consumes` are sorted and frozen into `MinimalIr::io` below.
    if !trpc_fragment_pairs.is_empty() {
        let composed =
            crate::analyze::compose_trpc_provides(trpc_fragment_pairs, |specifier, from_file| {
                resolve_envelope_specifier(specifier, from_file, &all_paths)
            });
        io_provides.extend(composed);
    }
    if !router_mount_pairs.is_empty() {
        let composed = crate::analyze::compose_router_mount_provides(
            router_mount_pairs,
            |specifier, from_file| resolve_envelope_specifier(specifier, from_file, &all_paths),
        );
        io_provides.extend(composed);
    }
    crate::analyze::late_resolve_cross_file_consumes(const_fragment_pairs, &mut io_consumes);

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
    warnings.extend(crate::analyze::run_diagnostics(
        file_count,
        &dep,
        &all_symbols,
        &[],
        config,
        false,
    ));
    let rels: Vec<&str> = loc_by_path.keys().map(String::as_str).collect();
    warnings.extend(crate::analyze::unmatched_suppression_warnings(
        config, &rels,
    ));
    warnings.extend(crate::analyze::unmatched_global_exclude_warnings(
        config, &rels,
    ));

    let mut global_findings = Vec::new();
    if is_enabled(&config.rule_config, "circular") {
        global_findings.extend(circular_findings(&cycles));
    }
    if is_enabled(&config.rule_config, "unreachable") {
        global_findings.extend(unreachable_findings(&nodes, &dep));
    }
    if is_enabled(&config.rule_config, "dead-candidates") {
        // No filesystem root (see module doc) -> no package.json-referenced entries; the envelope's own
        // `is_entry`-marked projections ARE the entry set — the Mode A counterpart of the Mode B overlay
        // union in `analyze::assemble` (same contract marker, same exemption). Before this, Mode A
        // silently dropped `is_entry` and every convention-loaded entry file (a crate's `lib.rs`, a test
        // harness file) read as dead — caught by the rust-parser-adapter example's self-analysis.
        let extra_entries: HashSet<String> = envelope
            .files
            .iter()
            .filter(|f| f.is_entry)
            .map(|f| f.path.clone())
            .collect();
        global_findings.extend(dead_candidate_findings(&nodes, &dep, &extra_entries));
    }

    let findings = merge_findings(
        vec![per_file_findings, global_findings],
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
        nodes,
        scores: None,
        health: None,
        recommendations: Vec::new(),
        critical: Vec::new(),
        seams: Vec::new(),
        folders,
        layer_co_churn: None,
        warnings,
        cache: None,
        rule_timings: None,
    }
}

/// Merges each of `overlays` onto `artifacts` in place — the Mode B counterpart of `analyze_envelope`
/// (Mode A): a partial envelope (typically just `io` + fragment channels for a handful of files) folded
/// onto the native per-file artifacts a real `analyze_tree` run already produced, rather than an
/// envelope standing in for the entire tree. This is how an external framework adapter participates in
/// a native run without reimplementing a full parser (`EngineConfig::adapter_overlays`; empty = the
/// pre-overlay path, byte-for-byte).
///
/// Overlays are processed in `parser`-sorted order (deterministic regardless of assembly order) and
/// each is re-validated via `zzop_core::validate_envelope` first — a malformed overlay degrades to one
/// `warnings` entry naming its `parser` id and first few issues, then is skipped entirely.
///
/// Per `FileProjection`: if `path` matches an existing artifact's `rel`, it's merged in place — `io`
/// entries appended minus exact-duplicate `(kind, key, file, line)` tuples (`file` normalized to
/// `projection.path` first), fragments appended with no dedup (composition dedups later), and
/// `const_map_fragment` native-first (existing key wins); the native artifact's own
/// `imports`/`re_exports`/`dynamic_imports` are left untouched (native dep-graph facts stay
/// authoritative — see `merge_projection_onto_artifact`'s doc). If `path` names no existing artifact
/// (e.g. a `.py`/`.jsp`/`.svelte` sibling the native dispatch table doesn't recognize), it's pushed as a
/// synthetic `FileArtifact` carrying the projection's OWN `imports`/`re_exports`/`dynamic_imports` (so
/// it contributes real dep-graph fan-in edges too — see `synthetic_artifact_from_projection`'s doc) with
/// every other native-only field (symbols, wrapper/query/store/field-usage fragments) at its
/// empty/default value. A `FileProjection` additionally marked `is_entry: true` has its `path` unioned
/// into `dead_candidate_findings`'s `extra_entries` set in `analyze::assemble`, exempting it from
/// `dead-candidates` the same way a package.json manifest entry is exempt.
///
/// `artifacts` is re-sorted by `rel` before returning — `analyze::assemble` relies on that order for
/// `ir.ir.symbols`'s determinism.
///
/// Before either merge branch, every reserved engine-internal sentinel `IoProvide`/`IoConsume` (kinds
/// `nest-global-prefix`/`client-base-prefix`, see [`is_reserved_provide_kind`]/[`is_reserved_consume_kind`])
/// is dropped from the projection's `io` — a producer-forbidden pair only the native TS parser may emit
/// and only the native `analyze::assemble` seams (`apply_and_strip_global_prefix`/
/// `apply_client_base_prefixes`) may consume+strip. Those seams run later over the WHOLE tree's merged
/// `io_provides`/`io_consumes`, so an overlay sentinel that survived the merge would get re-applied
/// project-wide (every native route re-prefixed), not scoped to the overlay's own files. Each overlay with
/// any drops gets one aggregate `warnings` entry naming its `parser`, the dropped count, and the reserved
/// kinds (built by [`reserved_drop_warning`], shared with `analyze_envelope`'s Mode A counterpart so the
/// two modes' wording can't drift) — a partial drop, so (unlike a validation failure) the overlay's other
/// io/fragments still merge.
pub(crate) fn apply_adapter_overlays(
    artifacts: &mut Vec<crate::pipeline::FileArtifact>,
    overlays: &[NormalizedEnvelope],
    warnings: &mut Vec<String>,
) {
    let mut ordered: Vec<&NormalizedEnvelope> = overlays.iter().collect();
    ordered.sort_by(|a, b| a.parser.cmp(&b.parser));

    for overlay in ordered {
        let json = match serde_json::to_string(overlay) {
            Ok(j) => j,
            Err(e) => {
                warnings.push(format!(
                    "adapter overlay '{}' skipped: failed to serialize for validation: {e}",
                    overlay.parser
                ));
                continue;
            }
        };
        if let Err(issues) = zzop_core::validate_envelope(&json) {
            let detail = issues
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join("; ");
            warnings.push(format!(
                "adapter overlay '{}' skipped: {detail}",
                overlay.parser
            ));
            continue;
        }

        // Reserved engine-internal sentinel kinds (see `is_reserved_provide_kind`/`is_reserved_consume_kind`
        // above) are producer-forbidden: dropped from every projection's `io` BEFORE the merge/synthetic
        // branch below, so neither path can hand one to `apply_and_strip_global_prefix`/
        // `apply_client_base_prefixes` (which run later, inside `analyze::assemble`, over the WHOLE native
        // tree's `io_provides`/`io_consumes` — an overlay sentinel surviving to there would get
        // re-interpreted as a real project-wide setting and re-prefix every native route, not just this
        // overlay's own). Dropped counts are aggregated across the WHOLE overlay (every projection), then
        // reported as one warning per overlay — a partial drop, not a skip, so processing continues.
        let mut reserved_dropped = 0usize;
        for projection in &overlay.files {
            let (cleaned, dropped) = drop_reserved_io(projection);
            reserved_dropped += dropped;
            if let Some(artifact) = artifacts.iter_mut().find(|a| a.rel == cleaned.path) {
                merge_projection_onto_artifact(artifact, &cleaned);
            } else {
                artifacts.push(synthetic_artifact_from_projection(&cleaned));
            }
        }
        if let Some(w) = reserved_drop_warning("adapter overlay", &overlay.parser, reserved_dropped)
        {
            warnings.push(w);
        }
    }

    artifacts.sort_by(|a, b| a.rel.cmp(&b.rel));
}

/// Returns a clone of `projection` with every reserved engine-internal `IoProvide`/`IoConsume` entry
/// dropped from its `io` (see [`is_reserved_provide_kind`]/[`is_reserved_consume_kind`]), plus how many
/// entries were dropped — the Mode B (`apply_adapter_overlays`) counterpart of Mode A's own ingestion-time
/// filter in `analyze_envelope` above. Every other field is untouched.
fn drop_reserved_io(projection: &zzop_core::FileProjection) -> (zzop_core::FileProjection, usize) {
    let mut cleaned = projection.clone();
    let before = cleaned.io.provides.len() + cleaned.io.consumes.len();
    cleaned
        .io
        .provides
        .retain(|p| !is_reserved_provide_kind(&p.kind));
    cleaned
        .io
        .consumes
        .retain(|c| !is_reserved_consume_kind(&c.kind));
    let after = cleaned.io.provides.len() + cleaned.io.consumes.len();
    (cleaned, before - after)
}

/// Overwrites every `IoProvide`/`IoConsume` in `io`'s `file` field to `path` — the defensive
/// normalization `apply_adapter_overlays` describes: an overlay is not trusted to already have set
/// `file` to match its own `FileProjection::path`.
fn normalize_io_file_field(io: &mut IoFacts, path: &str) {
    for provide in &mut io.provides {
        provide.file = path.to_string();
    }
    for consume in &mut io.consumes {
        consume.file = path.to_string();
    }
}

/// The "found" branch of `apply_adapter_overlays`'s per-`FileProjection` merge (see that function's doc
/// for the dedup/native-first semantics per channel). A TypeScript artifact the native pass parsed keeps
/// its own authoritative `imports`/`re_exports`/`dynamic_imports` (an overlay never overrides parsed
/// facts). But the native pass walks EVERY file, so a non-TS file type the engine can't parse (e.g. a
/// `.svelte` component) lands here too as a degraded artifact with `imports: None` — nothing to preserve
/// — and an overlay carrying dep-graph data then fills it, letting an adapter complete the graph for that
/// file type (its imports become real fan-in edges to their TS targets).
fn merge_projection_onto_artifact(
    artifact: &mut crate::pipeline::FileArtifact,
    projection: &zzop_core::FileProjection,
) {
    // Dep-graph facts: adopt the overlay's only when the native artifact has none of its own (a
    // degraded/non-TS file), so parsed TS imports always win over an overlay.
    if artifact.imports.is_none()
        && (!projection.imports.is_empty()
            || !projection.re_exports.is_empty()
            || !projection.dynamic_imports.is_empty())
    {
        artifact.imports = Some(projection.imports.clone());
        artifact.re_exports = projection.re_exports.clone();
        artifact.dynamic_imports = projection.dynamic_imports.clone();
    }

    let mut incoming_io = projection.io.clone();
    normalize_io_file_field(&mut incoming_io, &projection.path);

    let existing = artifact.io.get_or_insert_with(IoFacts::default);
    for provide in incoming_io.provides {
        let dup = existing.provides.iter().any(|p| {
            p.kind == provide.kind
                && p.key == provide.key
                && p.file == provide.file
                && p.line == provide.line
        });
        if !dup {
            existing.provides.push(provide);
        }
    }
    for consume in incoming_io.consumes {
        let dup = existing.consumes.iter().any(|c| {
            c.kind == consume.kind
                && c.key == consume.key
                && c.file == consume.file
                && c.line == consume.line
        });
        if !dup {
            existing.consumes.push(consume);
        }
    }

    artifact
        .procedure_router_fragments
        .extend(projection.procedure_router_fragments.iter().cloned());
    artifact
        .router_mount_fragments
        .extend(projection.router_mount_fragments.iter().cloned());
    artifact
        .class_shape_fragments
        .extend(projection.class_shape_fragments.iter().cloned());
    for (key, value) in &projection.const_map_fragment {
        artifact
            .const_map_fragment
            .entry(key.clone())
            .or_insert_with(|| value.clone());
    }
}

/// The "not found" branch of `apply_adapter_overlays`'s per-`FileProjection` merge — builds a brand-new
/// `FileArtifact` for a `path` the native pass never dispatched at all.
fn synthetic_artifact_from_projection(
    projection: &zzop_core::FileProjection,
) -> crate::pipeline::FileArtifact {
    let mut io = projection.io.clone();
    normalize_io_file_field(&mut io, &projection.path);
    let io = if io.provides.is_empty() && io.consumes.is_empty() {
        None
    } else {
        Some(io)
    };

    // Per the Mode B dep-graph-completion contract (the injection contract extends past io/fragments to
    // dep-graph facts, so any non-TS adapter can complete the graph while the engine stays
    // framework-neutral): `analyze::assemble` only ever folds an artifact's `imports`/`re_exports`/
    // `dynamic_imports` into `ts_import_pairs`/`ts_re_export_pairs`/`ts_dynamic_import_pairs` (-> real
    // dep-graph edges, via `build_dep_with_workspace`) inside its `if let Some(imports) = artifact.imports`
    // branch — so `imports` must be `Some` whenever ANY of the three carries data, not just when `imports`
    // itself is non-empty (a bare re-export or a dynamic-only file can have an empty `imports` map and
    // still need graph participation, mirroring `analyze_envelope`'s own Defect-A/2 handling below). Truly
    // empty (none of the three populated) keeps `imports: None` so a no-data overlay file doesn't
    // needlessly enter `ts_import_pairs`/`ts_paths`/`package_import_files`.
    let has_dep_graph_data = !projection.imports.is_empty()
        || !projection.re_exports.is_empty()
        || !projection.dynamic_imports.is_empty();

    crate::pipeline::FileArtifact {
        rel: projection.path.clone(),
        symbols: Vec::new(),
        // Was unconditionally `None` ("dead data" by design) — now carries the projection's own imports
        // whenever there is dep-graph data to contribute, so an injected non-TS file (`.svelte`/`.vue`/
        // `.astro`) gives its imported native TS targets real fan-in, exactly like a native TS importer
        // would. This is the synthetic-artifact half of the injection contract's dep-graph completion;
        // `merge_projection_onto_artifact` (the onto-an-EXISTING-native-artifact branch, above) is
        // deliberately NOT touched here — native imports stay authoritative there, a separate concern.
        imports: has_dep_graph_data.then(|| projection.imports.clone()),
        // Now carried through (previously always `Vec::new()` — see the superseded comment this
        // replaces) via the SAME `if let Some(imports)` branch in `analyze::assemble` as `imports` right
        // above: a synthetic overlay file's bare re-export or dynamic `import()` now gives its target
        // real fan-in too. (Mode A's `analyze_envelope` is unaffected either way: it builds `dep` by hand
        // straight from `FileProjection`, per this file's own re-export/dynamic-import merge in that
        // function, never through this struct.)
        re_exports: projection.re_exports.clone(),
        dynamic_imports: projection.dynamic_imports.clone(),
        loc: projection.loc,
        findings: Vec::new(),
        degraded: false,
        minified_or_generated: false,
        io,
        rule_timings: Vec::new(),
        used_names: Vec::new(),
        const_map_fragment: projection.const_map_fragment.clone(),
        procedure_router_fragments: projection.procedure_router_fragments.clone(),
        router_mount_fragments: projection.router_mount_fragments.clone(),
        // Wrapper resolution, query-call-site recognition, store-binding recognition, and field-usage-
        // token scanning are all native-TS-source concerns; an external adapter emits final io/router
        // fragments instead, so a synthetic overlay artifact never carries these. Controller-prefix
        // route fragments are the same native-TS-only concern (module doc): an external adapter already
        // resolves its own controller prefixes before emitting `IoProvide`s, so it never has one of
        // these to carry either.
        wrapper_def_fragments: Vec::new(),
        wrapper_call_fragments: Vec::new(),
        controller_prefix_route_fragments: Vec::new(),
        // Class shapes ARE plumbed from the projection (unlike the native-TS-only concerns above):
        // an adapter may emit `IoProvide::body.dto_ref` and rely on the same assemble-time resolver
        // native controllers use, feeding it shapes for classes its own language declares.
        class_shape_fragments: projection.class_shape_fragments.clone(),
        query_call_sites: Vec::new(),
        field_usage_tokens: Vec::new(),
        // Plumbed straight from the projection (empty when absent) — same "carry the real fact, never a
        // placeholder" reasoning as the Mode A `SourceFile` above, even though no DSL rule pass runs over
        // a synthetic overlay artifact today (`findings: Vec::new()` above).
        loop_spans: projection.loop_spans.clone(),
    }
}

/// Resolves one fragment `Ref`/`Mount` specifier for envelope-mode composition — no tsconfig/
/// workspace-alias machinery, since an envelope's `FileProjection::path` set is the entire addressable
/// universe. Contract: (a) an exact match of `specifier` against known file paths wins outright; (b)
/// else, if `specifier` starts with `./` or `../`, join it against `from_file`'s own directory
/// (normalizing `.`/`..` segments as pure string ops, no filesystem APIs), try that joined path as-is,
/// then try appending each of `.ts`/`.tsx`/`.js` in turn; (c) anything else resolves to `None` —
/// external/unresolved, never guessed.
fn resolve_envelope_specifier(
    specifier: &str,
    from_file: &str,
    all_paths: &HashSet<&str>,
) -> Option<String> {
    if all_paths.contains(specifier) {
        return Some(specifier.to_string());
    }
    if !specifier.starts_with("./") && !specifier.starts_with("../") {
        return None;
    }

    // `from_file`'s own directory, as path segments (envelope paths are contractually forward-slash,
    // so plain `/`-splitting avoids `std::path::Path`'s Windows-backslash normalization surprises).
    let mut segments: Vec<&str> = from_file.split('/').collect();
    segments.pop(); // drop the file's own basename, keeping just its directory

    for part in specifier.split('/') {
        match part {
            "." | "" => {}
            ".." => {
                segments.pop();
            }
            seg => segments.push(seg),
        }
    }
    let joined = segments.join("/");

    if all_paths.contains(joined.as_str()) {
        return Some(joined);
    }
    for ext in [".ts", ".tsx", ".js"] {
        let candidate = format!("{joined}{ext}");
        if all_paths.contains(candidate.as_str()) {
            return Some(candidate);
        }
    }
    None
}

/// `pack`, with every rule whose matcher is not `SymbolScan`/`IoScan` dropped — see module doc for why.
fn envelope_rule_pack(pack: &RulePackDef) -> RulePackDef {
    let mut p = pack.clone();
    p.rules
        .retain(|r| matches!(r.matcher, Matcher::SymbolScan(_) | Matcher::IoScan(_)));
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use zzop_core::{
        FileProjection, ImportBinding, ImportMap, ReExport, SourceSymbol, SourceSymbolKind,
        NORMALIZED_AST_FORMAT,
    };

    fn projection(path: &str, loc: u32) -> FileProjection {
        FileProjection {
            path: path.to_string(),
            loc,
            symbols: Vec::new(),
            imports: ImportMap::new(),
            re_exports: Vec::new(),
            dynamic_imports: Vec::new(),
            used_names: Vec::new(),
            const_map_fragment: HashMap::new(),
            procedure_router_fragments: Vec::new(),
            router_mount_fragments: Vec::new(),
            class_shape_fragments: Vec::new(),
            io: IoFacts::default(),
            degraded: false,
            is_entry: false,
            attributes: Vec::new(),
            loop_spans: Vec::new(),
        }
    }

    fn envelope(files: Vec<FileProjection>) -> NormalizedEnvelope {
        NormalizedEnvelope {
            format: NORMALIZED_AST_FORMAT.to_string(),
            version: 1,
            parser: "test-parser/1".to_string(),
            source: "test".to_string(),
            files,
        }
    }

    fn config() -> EngineConfig {
        EngineConfig {
            source_id: "test".to_string(),
            ..EngineConfig::default()
        }
    }

    #[test]
    fn projects_loc_and_symbols_into_the_common_ir() {
        let mut a = projection("a.jsp", 10);
        a.symbols.push(SourceSymbol {
            id: "a.jsp#Foo".to_string(),
            file: "a.jsp".to_string(),
            name: "Foo".to_string(),
            kind: SourceSymbolKind::Class,
            line: 1,
            exported: true,
            is_default: false,
            body_start: None,
            body_end: None,
            write_sites: Vec::new(),
        });
        let env = envelope(vec![a]);
        let out = analyze_envelope(&env, &config());
        assert_eq!(out.file_count, 1);
        assert_eq!(out.ir.ir.loc.get("a.jsp"), Some(&10));
        assert_eq!(out.ir.ir.symbols.len(), 1);
        assert_eq!(out.ir.parser, "test-parser/1");
        assert_eq!(out.ir.source, "test");
    }

    #[test]
    fn resolves_dep_edge_when_specifier_matches_a_projected_path() {
        let mut a = projection("a.jsp", 5);
        a.imports.insert(
            "b".to_string(),
            ImportBinding {
                specifier: "b.jsp".to_string(),
                original: "default".to_string(),
                deferred: false,
                type_only: false,
            },
        );
        let b = projection("b.jsp", 5);
        let env = envelope(vec![a, b]);
        let out = analyze_envelope(&env, &config());
        assert_eq!(
            out.ir.ir.dep.get("a.jsp").cloned().unwrap_or_default(),
            vec!["b.jsp".to_string()]
        );
        assert_eq!(
            out.ir.ir.dep.get("b.jsp").cloned().unwrap_or_default(),
            Vec::<String>::new()
        );
    }

    // --- Envelope-mode parity for Defect A: bare re-exports merge into the dep graph too ---

    #[test]
    fn bare_re_export_creates_dep_edge_and_gives_the_target_fan_in_in_envelope_mode() {
        // `export { x } from './impl'` with no local import of `impl` — mirrors
        // `zzop_parser_typescript::lang::resolve::build_dep_impl`'s own
        // `bare_named_re_export_creates_dep_edge`/`re_export_target_gains_fan_in_via_reverse_dep_edge`,
        // but through the envelope entry point (`analyze_envelope`), which builds `dep` by hand rather
        // than calling `build_dep_impl`.
        let mut barrel = projection("barrel.jsp", 5);
        barrel.re_exports.push(ReExport {
            specifier: "impl.jsp".to_string(),
            original: "x".to_string(),
            local_alias: "x".to_string(),
            type_only: false,
        });
        let impl_file = projection("impl.jsp", 5);
        let env = envelope(vec![barrel, impl_file]);
        let out = analyze_envelope(&env, &config());

        assert_eq!(
            out.ir.ir.dep.get("barrel.jsp").cloned().unwrap_or_default(),
            vec!["impl.jsp".to_string()]
        );
        // `impl.jsp` must not read as dead — some other file's `dep` entry now names it, i.e. it has
        // fan-in via the reverse edge, and `dead-candidates` (a whole-graph analysis run above) must not
        // flag it.
        let fan_in = out
            .ir
            .ir
            .dep
            .values()
            .filter(|tos| tos.contains(&"impl.jsp".to_string()))
            .count();
        assert_eq!(fan_in, 1);
        assert!(!out
            .findings
            .iter()
            .any(|f| f.rule_id == "dead-candidates" && f.file == "impl.jsp"));
    }

    #[test]
    fn type_only_re_export_creates_excludable_dep_edge_in_envelope_mode() {
        // `export type { X } from './y'` is erased by TS at compile time, so it must never form a real
        // runtime cycle — but (Defect 1) it now DOES gain a real dep edge (fan-in), mirroring
        // `build_dep_impl`'s own `type_only_re_export_creates_excludable_dep_edge`.
        let mut barrel = projection("barrel.jsp", 5);
        barrel.re_exports.push(ReExport {
            specifier: "y.jsp".to_string(),
            original: "X".to_string(),
            local_alias: "X".to_string(),
            type_only: true,
        });
        let y_file = projection("y.jsp", 5);
        let env = envelope(vec![barrel, y_file]);
        let out = analyze_envelope(&env, &config());

        assert_eq!(
            out.ir.ir.dep.get("barrel.jsp").cloned().unwrap_or_default(),
            vec!["y.jsp".to_string()]
        );
        assert!(!out
            .findings
            .iter()
            .any(|f| f.rule_id == "dead-candidates" && f.file == "y.jsp"));
    }

    #[test]
    fn dynamic_import_creates_excludable_dep_edge_in_envelope_mode() {
        // Defect 2 (envelope parity): a dynamic `import()` specifier used to create no dep edge at all,
        // so a code-split-only module looked dead. It now gains a real edge (fan-in), and the cycle it
        // would otherwise form with a mutual dynamic import is not reported (mirrors
        // `dynamic_import_creates_excludable_dep_edge`/`dynamic_import_cycle_is_not_reported_as_circular`
        // in `zzop_parser_typescript::lang::resolve`'s own tests).
        let mut page = projection("page.jsp", 5);
        page.dynamic_imports.push("chart.jsp".to_string());
        let chart = projection("chart.jsp", 5);
        let env = envelope(vec![page, chart]);
        let out = analyze_envelope(&env, &config());

        assert_eq!(
            out.ir.ir.dep.get("page.jsp").cloned().unwrap_or_default(),
            vec!["chart.jsp".to_string()]
        );
        assert!(!out
            .findings
            .iter()
            .any(|f| f.rule_id == "dead-candidates" && f.file == "chart.jsp"));
    }

    #[test]
    fn unresolvable_specifier_is_external_not_an_error() {
        let mut a = projection("a.jsp", 5);
        a.imports.insert(
            "ext".to_string(),
            ImportBinding {
                specifier: "some/external/package".to_string(),
                original: "default".to_string(),
                deferred: false,
                type_only: false,
            },
        );
        let env = envelope(vec![a]);
        let out = analyze_envelope(&env, &config());
        assert!(out
            .ir
            .ir
            .dep
            .get("a.jsp")
            .cloned()
            .unwrap_or_default()
            .is_empty());
    }

    #[test]
    fn degraded_file_is_reported_but_loc_still_counted() {
        let mut a = projection("a.jsp", 3);
        a.degraded = true;
        let env = envelope(vec![a]);
        let out = analyze_envelope(&env, &config());
        assert_eq!(out.degraded, vec!["a.jsp".to_string()]);
        assert_eq!(out.ir.ir.loc.get("a.jsp"), Some(&3));
    }

    #[test]
    fn circular_import_pair_produces_a_circular_finding() {
        let mut a = projection("a.jsp", 5);
        a.imports.insert(
            "b".to_string(),
            ImportBinding {
                specifier: "b.jsp".to_string(),
                original: "default".to_string(),
                deferred: false,
                type_only: false,
            },
        );
        let mut b = projection("b.jsp", 5);
        b.imports.insert(
            "a".to_string(),
            ImportBinding {
                specifier: "a.jsp".to_string(),
                original: "default".to_string(),
                deferred: false,
                type_only: false,
            },
        );
        let env = envelope(vec![a, b]);
        let out = analyze_envelope(&env, &config());
        assert!(out.findings.iter().any(|f| f.rule_id == "circular"));
    }

    #[test]
    fn mutual_dynamic_import_pair_does_not_produce_a_circular_finding_in_envelope_mode() {
        // Defect 2 (envelope parity): two files linked ONLY by dynamic `import()` (both directions) must
        // not read as a cycle — a value-import cycle between the same two files still must (covered by
        // `circular_import_pair_produces_a_circular_finding` above).
        let mut a = projection("a.jsp", 5);
        a.dynamic_imports.push("b.jsp".to_string());
        let mut b = projection("b.jsp", 5);
        b.dynamic_imports.push("a.jsp".to_string());
        let env = envelope(vec![a, b]);
        let out = analyze_envelope(&env, &config());
        assert!(!out.findings.iter().any(|f| f.rule_id == "circular"));
    }

    #[test]
    fn io_facts_are_collected_and_surfaced_on_the_common_ir() {
        let mut a = projection("Ctrl.jsp", 20);
        a.io.provides.push(IoProvide {
            body: None,
            kind: "http".to_string(),
            key: "GET /legacy/user.jsp".to_string(),
            file: "Ctrl.jsp".to_string(),
            line: 3,
            symbol: None,
        });
        let env = envelope(vec![a]);
        let out = analyze_envelope(&env, &config());
        let io = out.ir.ir.io.expect("expected io facts");
        assert_eq!(io.provides.len(), 1);
        assert_eq!(io.provides[0].key, "GET /legacy/user.jsp");
    }

    #[test]
    fn git_config_is_ignored_with_a_warning_and_never_panics() {
        let mut cfg = config();
        cfg.git = Some(crate::GitOptions::default());
        let env = envelope(vec![projection("a.jsp", 1)]);
        let out = analyze_envelope(&env, &cfg);
        assert!(out.scores.is_none());
        assert!(out.health.is_none());
        assert!(out
            .warnings
            .iter()
            .any(|w| w.contains("git collection skipped")));
    }

    #[test]
    fn is_entry_projection_is_exempt_from_dead_candidates_in_envelope_mode() {
        // Mode A parity with the Mode B overlay union in `analyze::assemble`: an `is_entry`-marked
        // projection with zero fan-in (a crate root / test harness file, loaded by convention) must not
        // read as dead, while an unmarked zero-fan-in sibling still does.
        let mut entry = projection("lib.jsp", 5);
        entry.is_entry = true;
        let orphan = projection("orphan.jsp", 5);
        let out = analyze_envelope(&envelope(vec![entry, orphan]), &config());
        let dead: Vec<&str> = out
            .findings
            .iter()
            .filter(|f| f.rule_id == "dead-candidates")
            .map(|f| f.file.as_str())
            .collect();
        assert_eq!(dead, vec!["orphan.jsp"], "got findings: {:?}", out.findings);
    }

    // --- Config-diagnostics parity with `assemble` (the envelope-path diagnostics asymmetry) ---

    #[test]
    fn disabled_rules_typo_self_reports_in_envelope_mode() {
        let mut cfg = config();
        cfg.rule_config.disabled_rules = vec!["no-such-rule".to_string()];
        let out = analyze_envelope(&envelope(vec![projection("a.jsp", 5)]), &cfg);
        assert!(
            out.warnings
                .iter()
                .any(|w| w.contains("matching no known rule id") && w.contains("no-such-rule")),
            "got: {:?}",
            out.warnings
        );
    }

    #[test]
    fn unmatched_suppression_and_global_exclude_warn_in_envelope_mode() {
        let mut cfg = config();
        cfg.rule_config.suppressions = vec![zzop_core::Suppression {
            rule: "circular".to_string(),
            path: None,
            glob: Some("*.stories.tsx".to_string()),
        }];
        cfg.rule_config.global_excludes = vec![zzop_core::GlobalExclude {
            path: Some("legacy/".to_string()),
            glob: None,
        }];
        let out = analyze_envelope(&envelope(vec![projection("a.jsp", 5)]), &cfg);
        // One warning per dead filter: the suppression glob and the top-level exclude path.
        assert_eq!(
            out.warnings
                .iter()
                .filter(|w| w.contains("matched no files"))
                .count(),
            2,
            "got: {:?}",
            out.warnings
        );
    }

    #[test]
    fn matched_filters_and_valid_disabled_rules_stay_silent_in_envelope_mode() {
        let mut cfg = config();
        cfg.rule_config.disabled_rules = vec!["circular".to_string()];
        cfg.rule_config.suppressions = vec![zzop_core::Suppression {
            rule: "circular".to_string(),
            path: Some("a.jsp".to_string()),
            glob: None,
        }];
        let out = analyze_envelope(&envelope(vec![projection("a.jsp", 5)]), &cfg);
        assert!(
            !out.warnings
                .iter()
                .any(|w| w.contains("matching no known rule id") || w.contains("matched no files")),
            "got: {:?}",
            out.warnings
        );
    }

    #[test]
    fn symbol_scan_dsl_rule_fires_against_envelope_symbols() {
        let pack: RulePackDef = serde_json::from_str(
            r#"{"id":"t","framework":"any","rules":[{"id":"r","severity":"info","message":"m","matcher":{"type":"symbol-scan","file_pattern":"\\.jsp$","name_pattern":"^Bad"}}]}"#,
        )
        .unwrap();
        let mut a = projection("a.jsp", 5);
        a.symbols.push(SourceSymbol {
            id: "a.jsp#BadName".to_string(),
            file: "a.jsp".to_string(),
            name: "BadName".to_string(),
            kind: SourceSymbolKind::Function,
            line: 4,
            exported: true,
            is_default: false,
            body_start: None,
            body_end: None,
            write_sites: Vec::new(),
        });
        let env = envelope(vec![a]);
        let mut cfg = config();
        cfg.packs = vec![pack];
        let out = analyze_envelope(&env, &cfg);
        assert!(out.findings.iter().any(|f| f.rule_id == "t/r"));
    }

    #[test]
    fn line_scan_dsl_rule_never_fires_in_envelope_mode() {
        // A LineScan rule that would match "TODO" if it ever saw source text — envelope mode carries no
        // text, so the rule is filtered out rather than silently "running clean".
        let pack: RulePackDef = serde_json::from_str(
            r#"{"id":"t","framework":"any","rules":[{"id":"r","severity":"info","message":"m","matcher":{"type":"line-scan","file_pattern":"\\.jsp$","line_pattern":"TODO"}}]}"#,
        )
        .unwrap();
        let env = envelope(vec![projection("a.jsp", 1)]);
        let mut cfg = config();
        cfg.packs = vec![pack];
        let out = analyze_envelope(&env, &cfg);
        assert!(!out.findings.iter().any(|f| f.rule_id == "t/r"));
    }

    #[test]
    fn two_runs_over_the_same_envelope_are_byte_for_byte_identical() {
        let mut a = projection("a.jsp", 5);
        a.imports.insert(
            "b".to_string(),
            ImportBinding {
                specifier: "b.jsp".to_string(),
                original: "default".to_string(),
                deferred: false,
                type_only: false,
            },
        );
        let env = envelope(vec![a, projection("b.jsp", 5)]);
        let out1 = analyze_envelope(&env, &config());
        let out2 = analyze_envelope(&env, &config());
        assert_eq!(
            serde_json::to_value(&out1.ir).unwrap(),
            serde_json::to_value(&out2.ir).unwrap()
        );
        assert_eq!(
            serde_json::to_value(&out1.findings).unwrap(),
            serde_json::to_value(&out2.findings).unwrap()
        );
    }

    #[test]
    fn router_mount_fragments_split_across_two_files_compose_into_one_http_provide() {
        use zzop_core::{RouterMountEntry, RouterMountFragment};

        // Mount file: an "app" router mounting "sub" at "/api", by exact-path specifier.
        let mut mount_file = projection("app.jsp", 4);
        mount_file.router_mount_fragments.push(RouterMountFragment {
            name: "app".to_string(),
            entries: vec![RouterMountEntry::Mount {
                prefix: "/api".to_string(),
                ident: "sub".to_string(),
                specifier: Some("sub.jsp".to_string()),
            }],
        });

        // Sub-router file: registers one verb, `POST /widgets`.
        let mut sub_file = projection("sub.jsp", 3);
        sub_file.router_mount_fragments.push(RouterMountFragment {
            name: "sub".to_string(),
            entries: vec![RouterMountEntry::Verb {
                method: "POST".to_string(),
                path: "/widgets".to_string(),
                handler: Some("createWidget".to_string()),
                line: 2,
            }],
        });

        let env = envelope(vec![mount_file, sub_file]);
        let out = analyze_envelope(&env, &config());
        let provides = out.ir.ir.io.expect("expected io facts").provides;
        assert!(
            provides
                .iter()
                .any(|p| p.kind == "http" && p.key == "POST /api/widgets" && p.file == "sub.jsp"),
            "{:?}",
            provides
        );
    }

    // --- Reserved engine-internal sentinel kinds are producer-forbidden in envelopes too (Mode A parity
    // with the Mode B overlay filter — `apply_adapter_overlays`'s own tests in
    // `tests/analyze_adapter_overlay.rs` cover the identical contract for overlays) ---

    #[test]
    fn nest_global_prefix_provide_is_dropped_and_warned_in_envelope_mode() {
        let mut a = projection("legacy.jsp", 3);
        a.io.provides.push(IoProvide {
            body: None,
            kind: zzop_parser_typescript::NEST_GLOBAL_PREFIX_KIND.to_string(),
            key: "api".to_string(),
            file: "legacy.jsp".to_string(),
            line: 1,
            symbol: None,
        });
        // An ordinary sibling route, so `io` is `Some` and we can also pin it comes through untouched —
        // envelope mode never runs `apply_and_strip_global_prefix` at all (module doc), so there is no
        // tree-wide re-prefix step to prove absent here the way the Mode B e2e test does; this instead
        // pins that the drop itself does not disturb an ordinary sibling provide.
        a.io.provides.push(IoProvide {
            body: None,
            kind: "http".to_string(),
            key: "GET /widgets".to_string(),
            file: "legacy.jsp".to_string(),
            line: 2,
            symbol: None,
        });
        let env = envelope(vec![a]);
        let out = analyze_envelope(&env, &config());

        let provides = out.ir.ir.io.expect("expected io facts").provides;
        assert!(
            !provides
                .iter()
                .any(|p| p.kind == zzop_parser_typescript::NEST_GLOBAL_PREFIX_KIND),
            "{:?}",
            provides
        );
        assert!(
            provides
                .iter()
                .any(|p| p.kind == "http" && p.key == "GET /widgets"),
            "{:?}",
            provides
        );
        assert!(
            out.warnings.iter().any(|w| w.contains("test-parser/1")
                && w.contains("dropped 1 reserved engine-internal io entry")
                && w.contains("nest-global-prefix")),
            "{:?}",
            out.warnings
        );
    }

    #[test]
    fn client_base_prefix_consume_is_dropped_and_warned_in_envelope_mode() {
        // `IoConsume`-side counterpart of the provide test above: an envelope emitting
        // `zzop_parser_typescript::CLIENT_BASE_PREFIX_KIND` directly is producer-forbidden the same way.
        let mut a = projection("legacy.jsp", 2);
        a.io.consumes.push(IoConsume {
            kind: zzop_parser_typescript::CLIENT_BASE_PREFIX_KIND.to_string(),
            key: Some("/api".to_string()),
            file: "legacy.jsp".to_string(),
            line: 1,
            raw: None,
            method: None,
            body: None,
            client: Some("axios".to_string()),
        });
        let env = envelope(vec![a]);
        let out = analyze_envelope(&env, &config());

        let consumes = out.ir.ir.io.map(|io| io.consumes).unwrap_or_default();
        assert!(
            !consumes
                .iter()
                .any(|c| c.kind == zzop_parser_typescript::CLIENT_BASE_PREFIX_KIND),
            "{:?}",
            consumes
        );
        assert!(
            out.warnings.iter().any(|w| w.contains("test-parser/1")
                && w.contains("dropped 1 reserved engine-internal io entry")
                && w.contains("client-base-prefix")),
            "{:?}",
            out.warnings
        );
    }

    #[test]
    fn ordinary_io_kinds_are_not_dropped_or_warned_in_envelope_mode() {
        // Control case: an envelope whose `io` carries only ordinary (non-reserved) kinds must pass
        // through in full, with no drop warning at all — the reserved-kind filter must not have
        // false-positive reach (mirrors `overlay_with_only_ordinary_io_kinds_merges_with_no_drop_warning`).
        let mut a = projection("a.jsp", 3);
        a.io.provides.push(IoProvide {
            body: None,
            kind: "http".to_string(),
            key: "GET /widgets".to_string(),
            file: "a.jsp".to_string(),
            line: 2,
            symbol: None,
        });
        a.io.consumes.push(IoConsume {
            kind: "http".to_string(),
            key: Some("/widgets".to_string()),
            file: "a.jsp".to_string(),
            line: 3,
            raw: None,
            method: Some("GET".to_string()),
            body: None,
            client: None,
        });
        let env = envelope(vec![a]);
        let out = analyze_envelope(&env, &config());

        let io = out.ir.ir.io.expect("expected io facts");
        assert_eq!(io.provides.len(), 1);
        assert_eq!(io.consumes.len(), 1);
        assert!(
            !out.warnings.iter().any(|w| w.contains("dropped")),
            "{:?}",
            out.warnings
        );
    }

    // --- `EngineConfig::mounts` parity: Mode A must apply config mounts too (audited consistency gap —
    // `apply_config_mounts` used to run only in the native `analyze::assemble` path, so a tree analyzed
    // via `analyze_envelope` with the same `mounts` config silently froze un-mounted keys). ---

    #[test]
    fn config_mount_prepends_gateway_prefix_to_an_http_provide_key_in_envelope_mode() {
        let mut a = projection("users.jsp", 5);
        a.io.provides.push(IoProvide {
            body: None,
            kind: "http".to_string(),
            key: "GET /users".to_string(),
            file: "users.jsp".to_string(),
            line: 1,
            symbol: None,
        });
        let env = envelope(vec![a]);
        let mut cfg = config();
        cfg.mounts = vec![crate::MountRule {
            dir: String::new(),
            at: "/gw".to_string(),
        }];
        let out = analyze_envelope(&env, &cfg);
        let provides = out.ir.ir.io.expect("expected io facts").provides;
        assert!(
            provides
                .iter()
                .any(|p| p.kind == "http" && p.key == "GET /gw/users"),
            "{:?}",
            provides
        );
    }

    #[test]
    fn config_mount_matching_nothing_emits_the_same_had_no_effect_warning_as_the_native_path_in_envelope_mode(
    ) {
        let mut a = projection("users.jsp", 5);
        a.io.provides.push(IoProvide {
            body: None,
            kind: "http".to_string(),
            key: "GET /users".to_string(),
            file: "users.jsp".to_string(),
            line: 1,
            symbol: None,
        });
        let env = envelope(vec![a]);
        let mut cfg = config();
        cfg.mounts = vec![crate::MountRule {
            dir: "nowhere".to_string(),
            at: "/gw".to_string(),
        }];
        let out = analyze_envelope(&env, &cfg);
        assert!(
            out.warnings.iter().any(|w| w.contains(
                "topology mount \"gw\" (dir \"nowhere\") had no effect: 0 http provides matched"
            )),
            "{:?}",
            out.warnings
        );
    }

    #[test]
    fn config_mount_leaves_non_http_provide_kinds_untouched_in_envelope_mode() {
        let mut a = projection("router.jsp", 5);
        a.io.provides.push(IoProvide {
            body: None,
            kind: "trpc".to_string(),
            key: "widgets.list".to_string(),
            file: "router.jsp".to_string(),
            line: 1,
            symbol: None,
        });
        let env = envelope(vec![a]);
        let mut cfg = config();
        cfg.mounts = vec![crate::MountRule {
            dir: String::new(),
            at: "/gw".to_string(),
        }];
        let out = analyze_envelope(&env, &cfg);
        let provides = out.ir.ir.io.expect("expected io facts").provides;
        assert!(
            provides
                .iter()
                .any(|p| p.kind == "trpc" && p.key == "widgets.list"),
            "{:?}",
            provides
        );
    }

    mod resolve_envelope_specifier_tests {
        use super::super::resolve_envelope_specifier;
        use std::collections::HashSet;

        #[test]
        fn relative_dot_slash_resolves_against_the_emitting_files_own_directory() {
            let all: HashSet<&str> = ["a/b.ts", "a/sibling.ts"].into_iter().collect();
            assert_eq!(
                resolve_envelope_specifier("./sibling", "a/b.ts", &all),
                Some("a/sibling.ts".to_string())
            );
        }

        #[test]
        fn parent_relative_dot_dot_slash_walks_up_one_directory() {
            let all: HashSet<&str> = ["a/b/c.ts", "a/x.ts"].into_iter().collect();
            assert_eq!(
                resolve_envelope_specifier("../x", "a/b/c.ts", &all),
                Some("a/x.ts".to_string())
            );
        }

        #[test]
        fn exact_match_wins_over_relative_join() {
            // "./x" from "a/b.ts" would join to "a/x" — but an exact path literally named "./x" must win
            // outright per the documented precedence.
            let all: HashSet<&str> = ["./x", "a/x.ts"].into_iter().collect();
            assert_eq!(
                resolve_envelope_specifier("./x", "a/b.ts", &all),
                Some("./x".to_string())
            );
        }

        #[test]
        fn extension_guessing_finds_a_real_source_file_behind_an_extensionless_join() {
            let all: HashSet<&str> = ["a/sibling.tsx"].into_iter().collect();
            assert_eq!(
                resolve_envelope_specifier("./sibling", "a/b.ts", &all),
                Some("a/sibling.tsx".to_string())
            );
        }

        #[test]
        fn unresolvable_specifier_is_none() {
            let all: HashSet<&str> = ["a/b.ts"].into_iter().collect();
            assert_eq!(
                resolve_envelope_specifier("some-package", "a/b.ts", &all),
                None
            );
            assert_eq!(
                resolve_envelope_specifier("./missing", "a/b.ts", &all),
                None
            );
        }
    }
}
