//! Native analysis registration mechanism — vocabulary-free.
//!
//! This crate (the kernel) carries ZERO rule vocabulary: no native analysis id, pack id, or rule id string
//! literal lives here. What stays here is only the MECHANISM every owning rules crate uses to plug its own
//! ids into the one shared registry. Each owning crate (`zzop_rules_graph`, `zzop_rules_http`,
//! `zzop_rules_cross_layer`, `zzop_rules_schema`, `zzop_metrics`) exposes its own
//! `register_native_analyses(&mut RuleRegistry)` that calls `register_native_analysis_stub` once per id it
//! owns; `zzop_engine::register_all_native` composes all five. See `rules/README.md`'s "Adding a rule"
//! section and `crates/engine/tests/rule_contracts/`'s "kernel is rule-vocabulary-free" contract test.

use super::RuleRegistry;

/// Registers one native whole-graph/whole-repo analysis id into the shared registry. "Stub" because the
/// registry entry is the id and nothing else: whole-graph analyses (circular, unreachable, criticality,
/// scores, ...) take their own inputs (`DepGraph`, `CouplingMap`, the cross-tree join, ...) and are
/// invoked directly by the orchestrator — the registry never runs anything. The entry exists SOLELY so
/// the id participates in enumeration (`ids`) and in the config id space (`is_enabled`, `is_suppressed`,
/// `apply_severity_override`); a finding's real severity is set where the finding is built.
///
/// This is the ONLY way a native analysis id enters a `RuleRegistry` — every owning rules crate's own
/// `register_native_analyses` calls this once per id it owns, so the actual id strings live in that
/// crate, never here. See this module's doc for the full split.
pub fn register_native_analysis_stub(registry: &mut RuleRegistry, id: &str) {
    registry.ids.push(id.to_string());
}
