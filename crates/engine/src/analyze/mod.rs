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
// `apply_config_mounts` is re-exported here (not just privately `use`d below) for the same reason as the
// trio above: `envelope::analyze_envelope` (Mode A) reaches it by this path too, at the structurally
// equivalent seam its own call site documents — origin-agnostic deployment topology must apply
// regardless of which assembler produced `io_provides`.
pub(crate) use compose::{
    apply_config_mounts, compose_router_mount_provides, compose_trpc_provides,
    late_resolve_cross_file_consumes,
};
// `envelope::analyze_envelope` also reaches the config-diagnostics quartet by this path (config-
// diagnostics parity with `assemble` — a `disabled_rules` typo / dead exclude filter self-reports on
// both entry points).
pub(crate) use diagnostics::{
    run_diagnostics, unmatched_global_exclude_warnings, unmatched_suppression_warnings,
    zero_packs_warning,
};
// `envelope::analyze_envelope` also imports these four native-analysis delegates by this path (same
// convention `circular_findings`'s own doc describes) — re-exported, not merely imported, so they stay
// reachable at `crate::analyze::<name>`.
pub(crate) use native_rules::{
    circular_findings, dead_candidate_findings, dep_stats_from_dep, unreachable_findings,
};

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
