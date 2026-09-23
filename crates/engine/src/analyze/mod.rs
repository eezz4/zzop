//! Assembly + whole-graph pass — runs after the fused per-file pass (`pipeline::run_file_pass`) has
//! already dropped every parser's AST. Operates on plain `zzop_core` data: `FileArtifact`s -> one
//! tree-wide `CommonIr` -> whole-graph native analyses (circular / unreachable / dead-candidates) ->
//! `merge_findings` with the per-file DSL findings collected during the fused pass.
//!
//! Also runs the optional git-history-dependent analyses: when `EngineConfig::git` is `Some` and `root`
//! is a git repository, `zzop_git::collect` feeds real `FileNode`s (via `zzop_core::build_file_nodes`),
//! from which `zzop_metrics`' `scores`/`health`/`recommendations`/`critical`/`seams` are computed.
//!
//! Two per-file "fragment now, compose later" passes run here over data the fused pass already
//! collected — no second parse: [`late_resolve_cross_file_consumes`] re-resolves a cross-file-indirected
//! `http` CONSUME from merged constant-map fragments, and [`compose_trpc_provides`] merges tRPC router
//! fragments into whole-tree `trpc` PROVIDEs.
//!
//! `assemble` itself (the orchestrator) lives in the [`assemble`] submodule, split into sequential
//! phases — see that module's own doc for the phase list.

mod assemble;
mod compose;
mod diagnostics;
mod native_rules;

pub(crate) use assemble::assemble;
pub(crate) use diagnostics::GitCache;
// `apply_config_mounts` and `apply_config_client_base` are re-exported here (not just privately `use`d
// below) for the same reason as the trio above: `envelope::analyze_envelope` (Mode A) reaches them by
// this path too, at the structurally equivalent seam its own call site documents — an origin-agnostic
// topology declaration must apply regardless of which assembler produced `io_provides`/`io_consumes`.
pub(crate) use compose::{
    apply_config_client_base, apply_config_mounts, compose_router_mount_provides,
    compose_trpc_provides, late_resolve_cross_file_consumes, merge_const_map_fragments,
};
// The pages-api fragment composer, re-exported for `crate::file_routes` (which owns the convention
// gates and the disk re-read, and hands each scan through this typed seam).
pub(crate) use compose::compose_pages_api_provides;
// The `body`/`response` dtoRef resolution trio, re-exported for `envelope::shapes` (Mode A) — the
// SAME passes `assemble::provides` runs natively, reused so the two lanes cannot drift on merge/
// poisoning/sentinel semantics (the `eval_io_scan_pack_timed` re-export precedent below).
pub(crate) use compose::{resolve_provide_body_refs, resolve_provide_response_refs, ShapeMerge};
// `envelope::analyze_envelope` also reaches the config-diagnostics quartet by this path (config-
// diagnostics parity with `assemble` — a `disabled_rules` typo / dead exclude filter self-reports on
// both entry points).
pub use diagnostics::MIN_UNCOVERED_EXTENSION_SHARE_PCT;
pub(crate) use diagnostics::{
    compute_dsl_scope_filtered, pack_scope_warnings, rule_overrides_applied, run_diagnostics,
    skipped_dirs_warning, uncompilable_rule_warnings, unmatched_global_exclude_warnings,
    unmatched_suppression_warnings, zero_packs_warning, DslScope,
};
// `envelope::analyze_envelope` also imports these four native-analysis delegates by this path (same
// convention `circular_findings`'s own doc describes) — re-exported, not merely imported, so they stay
// reachable at `crate::analyze::<name>`.
pub(crate) use native_rules::{
    circular_findings, dead_candidate_findings, dep_stats_from_dep, unreachable_findings,
};
// The profiled whole-tree io-scan evaluator (`assemble::rules::io_scan::eval_pack_timed`), shared by
// `envelope::ingest` so Mode A's `--profile-rules` runs through the SAME accumulator granularity.
pub(crate) use assemble::rules::eval_io_scan_pack_timed;

/// Times one whole-graph native analysis (`EngineConfig::profile_rules`): `t0` is `Some` exactly when
/// profiling is on, so the caller never pays an `Instant::now()` otherwise. Native analysis ids never
/// collide with DSL `rule_id`s (always pack-prefixed with a `/`), so keying both into the same
/// `HashMap` is safe. Lives here (not in `assemble`'s own `helpers` submodule) since `native_rules`'
/// own per-file callgraph-rule loop needs it too — both are descendants of this module.
pub(crate) fn record_native_timing(
    rule_time: &mut std::collections::HashMap<String, (u128, usize)>,
    t0: Option<std::time::Instant>,
    id: &str,
    findings: usize,
) {
    let Some(t0) = t0 else { return };
    let entry = rule_time.entry(id.to_string()).or_insert((0, 0));
    entry.0 += t0.elapsed().as_nanos();
    entry.1 += findings;
}

/// Finalizes the accumulated per-rule timings into `AnalyzeOutput::rule_timings`'s documented order:
/// `nanos` descending, `rule_id` ascending tie-break — deterministic regardless of `HashMap` iteration
/// order or rayon per-file scheduling.
pub(crate) fn sort_rule_timings(
    rule_time: std::collections::HashMap<String, (u128, usize)>,
) -> Vec<zzop_core::dsl::RuleTiming> {
    let mut out: Vec<zzop_core::dsl::RuleTiming> = rule_time
        .into_iter()
        .map(|(rule_id, (nanos, findings))| zzop_core::dsl::RuleTiming {
            rule_id,
            nanos,
            findings,
        })
        .collect();
    out.sort_by(|a, b| {
        b.nanos
            .cmp(&a.nanos)
            .then_with(|| a.rule_id.cmp(&b.rule_id))
    });
    out
}

/// Read `root/rel` for a pass that is about to PARSE it, refusing what the recursion gate refuses.
///
/// 🔴 **Any pass in this module that re-reads a file off disk and hands the text to a parser goes
/// through here.** The caps live in `pipeline::fresh` and are applied as the walk projects each file —
/// but a second pass that opens the file itself never reaches that stage, so it never reaches the gate
/// either. Measured on this HEAD before this function existed (review ledger V127): ONE 51 KB `.vue`
/// file, and separately one `composables/deep.ts` inside a Nuxt tree, each took `zzop analyze` to
/// **exit 127 with zero bytes of stdout** — and the second was a file the gate had correctly refused
/// moments earlier. The refusal was right; it just did not travel.
///
/// Returns `None` for a file past a cap, which every caller already handles: they all skip unreadable
/// files, and a refused file contributes nothing here for the same reason it contributes nothing from
/// the per-file lane — no AST means no AST facts.
///
/// Passes whose input list is ALREADY gate-filtered do not need this and do not call it: `java_rels`,
/// `csharp_rels` and `prisma_rels` are built inside `collect`'s `else` branch, after the one that
/// diverts a degraded artifact, so a refused file is never in them. The lists that are NOT filtered are
/// `prescan_rels` and `ts_paths` — and those are exactly the four callers here.
pub(in crate::analyze) fn read_for_parse(root: &std::path::Path, rel: &str) -> Option<String> {
    let bytes = std::fs::read(root.join(rel)).ok()?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    if crate::pipeline::text_exceeds_recursion_caps(&text, rel).is_some() {
        return None;
    }
    Some(text)
}
