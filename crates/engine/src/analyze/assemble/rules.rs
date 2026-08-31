//! Phase 4: every whole-graph / call-graph-BFS native analysis, gated by `EngineConfig::rule_config`
//! and timed under `EngineConfig::profile_rules`, PLUS (last, once `run_callgraph_rules`' own
//! `decorator_guarded` evidence exists) the [`io_scan`] sub-phase's whole-tree `Matcher::IoScan` DSL
//! pass — accumulates into one `Vec<Finding>` merged with the per-file DSL findings back in
//! `super::assemble`.

use std::collections::BTreeSet;
use std::time::Instant;

use zzop_core::{is_enabled, Finding, ImportMap};

use crate::analyze::native_rules::{
    circular_findings, dead_candidate_findings, run_callgraph_rules, run_schema_join_rules,
    unreachable_findings,
};
use crate::pipeline::PackageJsonScan;
use crate::EngineConfig;

use crate::analyze::record_native_timing;

mod config_entries;
mod entries;
mod framework_entries;
mod graph_inputs;
mod io_scan;
mod manifest_boundaries;

pub(super) use graph_inputs::GraphInputs;

// Re-exported (through `assemble` -> `crate::analyze`) for `envelope::ingest`'s profiled whole-tree
// io-scan pass — Mode A shares the one pack-splitting profiled evaluator instead of growing a twin.
pub(crate) use io_scan::eval_pack_timed as eval_io_scan_pack_timed;

/// Runs every whole-graph/call-graph-BFS native analysis in the same order (and under the same
/// `is_enabled` gates) the pre-split monolithic `assemble` did, then the two whole-tree DSL sub-phases —
/// [`io_scan`] (which PRODUCES findings) and [`gate_file_findings`] (which FILTERS the per-file pass's,
/// in place through `per_file_findings`, and pushes any §0 disclosure into `warnings`). Returns the
/// combined `global_findings`, merged with `per_file_findings` back in `super::assemble`.
///
/// The two `&mut` tails are the only outputs that are not the return value, and both are here for the
/// same reason the sub-phases are: they need the assembled `AttributeStore`, which does not exist before
/// this phase.
#[allow(clippy::too_many_arguments)]
pub(super) fn run(
    root: &std::path::Path,
    config: &EngineConfig,
    graph: &GraphInputs<'_>,
    pkg_scan: &PackageJsonScan,
    tsconfigs: &std::collections::BTreeMap<String, zzop_parser_typescript::TsconfigPaths>,
    ts_paths: &std::collections::HashSet<String>,
    ts_import_pairs: &[(String, ImportMap)],
    java_rels: &[String],
    rust_workspace: &crate::pipeline::RustWorkspaceMap,
    all_symbols: &[zzop_core::ir::SourceSymbol],
    dead_export_names_by_file: &std::collections::HashMap<
        String,
        crate::dead_exports::DeadExportNames,
    >,
    prisma_rels: &[String],
    attribute_store: &zzop_core::AttributeStore,
    field_usage_tokens: &std::collections::HashSet<String>,
    query_call_sites: &[zzop_core::QueryCallSite],
    io_provides: &[zzop_core::IoProvide],
    io_consumes: &[zzop_core::IoConsume],
    rule_time: &mut std::collections::HashMap<String, (u128, usize)>,
    prescan_import_pairs: &[(String, ImportMap)],
    per_file_findings: &mut Vec<Finding>,
    warnings: &mut Vec<String>,
) -> Vec<Finding> {
    // `profile` mirrors `dsl::eval_pack_impl`'s no-op-sink convention: `Instant::now()` is only ever called
    // when profiling is on, so a non-profiled `analyze_tree` call pays zero cost for the wrapping below.
    let profile = config.profile_rules;
    // The run's declared convention vocabulary, resolved once for every whole-tree rule below.
    let vocab = config.vocabulary.resolve();
    let mut global_findings = Vec::new();
    if is_enabled(&config.rule_config, "circular") {
        let t0 = profile.then(Instant::now);
        let found = circular_findings(graph.cycles);
        record_native_timing(rule_time, t0, "circular", found.len());
        global_findings.extend(found);
    }
    if is_enabled(&config.rule_config, "unreachable") {
        // `extra_entries`: cargo-manifest-declared target files (`[[bin]]`/`[[test]]`/... explicit
        // `path = "..."` keys) — loaded by cargo directly, never via `use`/`mod`, so a positive
        // `fan_in` on one (a co-located helper referenced by a sibling) is expected, not island
        // signal. Found by the first self-analysis dogfood run: every DSL pack's co-located
        // `<pack>.rs` test target was flagged. A `fan_in == 0` file is already an implicit entry
        // inside the rule (false-positive-safe by construction), so this only matters for the
        // fan_in > 0 shape. Threading the Mode-B overlay `is_entry` union (like `dead-candidates`
        // below) for a fan_in > 0 overlay case remains a separate follow-up, as does
        // `dead_export_findings`' missing parameter.
        let t0 = profile.then(Instant::now);
        let mut unreachable_entries: std::collections::HashSet<String> =
            rust_workspace.target_roots().iter().cloned().collect();
        // A `.ts` imported ONLY by a pre-scanned file has real fan-in (via `merge_prescan_fan_in`)
        // but no `dep` edge points at it (it is not a graph node), so it reads as a false `unreachable`
        // island. A framework-mounted component is effectively an entrypoint, so seed what it imports as
        // reachable — the same "loaded by a mechanism this graph can't see" contract as the cargo targets.
        unreachable_entries.extend(graph.prescan_targets.iter().cloned());
        // Same contract for runtime asset-URL targets (worklet/worker/importScripts/`new URL`): a
        // `public/*.js` worklet has real fan-in (via `merge_asset_ref_fan_in`) but no incoming `dep`
        // edge, so it too would read as a false `unreachable` island without being seeded as an entry —
        // it IS an entrypoint, loaded by the browser's asset loader this graph can't see.
        unreachable_entries.extend(graph.asset_targets.iter().cloned());
        // Same contract for Nuxt auto-import targets: a bare symbol name is not an import statement, so
        // the composable it reaches has real fan-in and no incoming `dep` edge at all. Without this seed
        // the repair trades hundreds of `dead-candidates` FPs for the same defect under `unreachable`.
        unreachable_entries.extend(graph.auto_import_targets.iter().cloned());
        let found = unreachable_findings(graph.nodes, graph.dep, &unreachable_entries);
        record_native_timing(rule_time, t0, "unreachable", found.len());
        global_findings.extend(found);
    }
    if is_enabled(&config.rule_config, "dead-candidates") {
        let t0 = profile.then(Instant::now);
        // Every mechanism that reaches a file without an import edge — manifest fields (BOTH deployment
        // roles), overlays, tool-config text, framework directory conventions. `entries` owns the set.
        let extra_entries = entries::collect(
            root,
            ts_paths,
            &pkg_scan.all_entry_paths(),
            graph.overlay_entry_paths,
        );
        // Drop candidates on author-declared generated files, mirroring `unimported-export`' exemption: a
        // generated file is regenerated, not hand-edited, so "delete this unused file" is non-actionable
        // there. Reads only the (few) candidate files' heads. Same `has_generated_banner` detector.
        let found: Vec<_> = dead_candidate_findings(graph.nodes, graph.dep, &extra_entries)
            .into_iter()
            .filter(|f| {
                !crate::generated_banner::file_has_generated_banner(
                    root,
                    &f.file,
                    &vocab.generated_file_markers,
                )
            })
            .collect();
        record_native_timing(rule_time, t0, "dead-candidates", found.len());
        global_findings.extend(found);
    }
    if is_enabled(&config.rule_config, "unimported-export") {
        let t0 = profile.then(Instant::now);
        let found = crate::dead_exports::dead_export_findings(
            root,
            ts_paths,
            ts_import_pairs,
            all_symbols,
            dead_export_names_by_file,
            &pkg_scan.workspace_pkgs,
            tsconfigs,
            prescan_import_pairs,
            graph.auto_import_names,
            &vocab.generated_file_markers,
        );
        record_native_timing(rule_time, t0, "unimported-export", found.len());
        global_findings.extend(found);
    }

    if is_enabled(&config.rule_config, "schema-usage") {
        let t0 = profile.then(Instant::now);
        let found = crate::pipeline::schema_usage_findings(
            &config.rule_config,
            root,
            prisma_rels,
            attribute_store,
            field_usage_tokens,
            &vocab.schema_usage_skip_fields,
        );
        record_native_timing(rule_time, t0, "schema-usage", found.len());
        global_findings.extend(found);
    }

    // The schema x usage JOIN native rules — see `run_schema_join_rules`'s own doc.
    run_schema_join_rules(
        root,
        prisma_rels,
        query_call_sites,
        config,
        profile,
        rule_time,
        &mut global_findings,
    );

    // Native fullstack rule: same (METHOD, path) HTTP route provided 2+ times across the tree — a
    // whole-tree pass over `io_provides` already collected above.
    if is_enabled(&config.rule_config, "duplicate-route") {
        let t0 = profile.then(Instant::now);
        // Manifest directories = the nearest-ancestor deployment HINT the tree carries, for DISCLOSURE
        // only — the rule adds the fact to a finding whose two sites straddle a boundary and never drops
        // one. Both halves of that (which files are manifests, and why the answer may not be acted on)
        // belong to `manifest_boundaries`, which also carries why they are no longer derived from the
        // npm/Go import indexes.
        let manifest_dirs = manifest_boundaries::scan(root, io_provides);
        let found = zzop_rules_http::duplicate_route_findings(io_provides, &manifest_dirs);
        record_native_timing(rule_time, t0, "duplicate-route", found.len());
        global_findings.extend(found);
    }

    // Native fullstack rule: within one file, an earlier param route shadows a later literal route of
    // the same shape (see `zzop_rules_http::route_shadowing`'s module doc for the decidable subset).
    if is_enabled(&config.rule_config, "route-shadowing") {
        let t0 = profile.then(Instant::now);
        let found = zzop_rules_http::route_shadowing_findings(io_provides);
        record_native_timing(rule_time, t0, "route-shadowing", found.len());
        global_findings.extend(found);
    }

    // Native fullstack rule: a resolved `http` consume with no matching provide anywhere in this tree,
    // gated on this tree itself having at least one `http` provide (see
    // `zzop_rules_http::unprovided_consume`'s module doc for the zero-provides veto). `config.hosts` is
    // threaded in so an absolute-URL consume to a host this tree DECLARES it owns is re-keyed internal
    // before the rule's `://` veto — the same transform `link_cross_layer_io` runs before its own gate,
    // without which this single-tree surface vetoes a call the multi-tree join reports.
    if is_enabled(&config.rule_config, "unprovided-consume") {
        let t0 = profile.then(Instant::now);
        let found = zzop_rules_http::unprovided_consume_findings(
            io_provides,
            io_consumes,
            &config.hosts,
            vocab.api_segment_pattern,
        );
        record_native_timing(rule_time, t0, "unprovided-consume", found.len());
        global_findings.extend(found);
    }

    let mut decorator_guarded = BTreeSet::new();
    run_callgraph_rules(
        root,
        config,
        attribute_store,
        io_provides,
        ts_paths,
        ts_import_pairs,
        java_rels,
        rust_workspace,
        all_symbols,
        profile,
        rule_time,
        &mut global_findings,
        &mut decorator_guarded,
    );

    // Whole-tree `Matcher::IoScan` DSL pass — runs last, now that `decorator_guarded` (just above) is
    // fully accumulated, so `io_scan::run` can mint from it. See that fn's doc. Takes `rule_time` for the
    // same reason every rule call above does: its rules are profiled into the shared accumulator, keyed
    // `"{pack}/{rule}"` (it reads `config.profile_rules` itself rather than taking the `profile` bool,
    // since it also branches its evaluation shape on it).
    global_findings.extend(io_scan::run(
        root,
        config,
        io_provides,
        io_consumes,
        attribute_store,
        &decorator_guarded,
        rule_time,
    ));

    // The `line-scan` counterpart of the `io_scan` sub-phase: applies every enabled `line-scan` rule's
    // `attr_present`/`attr_absent`/`require_attr_declared` gates to the FUSED PER-FILE pass's own
    // findings, in place, and pushes the §0 disclosure for any rule an undeclared key silenced. LAST, so
    // it lands after every native pass and before `super::assemble`'s `merge_findings` — a gated-away
    // finding is therefore never a severity-override or suppression input.
    //
    // WHY IT IS A POST-FILTER AND NOT A CHECK INSIDE THE PER-FILE PASS: a per-file finding is cached
    // under `(content_hash, parser_fingerprint, scope, ruleset_fingerprint)` and the attribute set is
    // none of those, so gating inside the cached unit would freeze a declaration into entries that
    // outlive it — edit `zzop.config.jsonc` and warm files would keep serving findings judged against the
    // old one. The cache stores UNGATED findings and this recomputes every run, the same placement
    // `severity_overrides`/`suppressions` already use. Full reasoning, and what the placement costs a
    // pack author, in `zzop_core::dsl::apply_attr_gates`' module doc.
    warnings.extend(zzop_core::apply_attr_gates(
        &config.packs,
        &config.rule_config,
        attribute_store,
        per_file_findings,
    ));

    global_findings
}
