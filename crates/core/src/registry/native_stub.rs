//! Native analysis registration mechanism — vocabulary-free.
//!
//! This crate (the kernel) carries ZERO rule vocabulary: no native analysis id, pack id, or rule id string
//! literal lives here. What stays here is only the MECHANISM every owning rules crate uses to plug its own
//! ids into the one shared registry. Each owning crate (`zzop_rules_graph`, `zzop_rules_http`,
//! `zzop_rules_cross_layer`, `zzop_rules_schema`, `zzop_metrics`) exposes its own
//! `register_native_analyses(&mut RuleRegistry)` that calls `register_native_analysis_stub` once per id it
//! owns; `zzop_engine::register_all_native` composes all five. See `rules/README.md`'s "Adding a rule"
//! section and `crates/engine/tests/rule_contracts/`'s "kernel is rule-vocabulary-free" contract test.

use crate::{finding::Finding, ir::CommonIr, Severity};

use super::{RuleDescriptor, RuleKind, RuleMeta, RuleRegistry};

/// A native analysis's registry entry. Whole-graph analyses (circular, unreachable, criticality, scores,
/// ...) take their own inputs (`DepGraph`, `CouplingMap`, ...), not a single `CommonIr` — they are invoked
/// directly by the orchestrator, not through `RuleRegistry::run_all`'s per-IR dispatch. This stub exists
/// SOLELY so the analysis's id participates in the one shared registry for enumeration/gating purposes
/// (`is_enabled`, `is_suppressed`, `metas`); `run` is a deliberate no-op, never called by the orchestrator
/// for these ids.
struct NativeAnalysisStub(RuleMeta);

impl RuleDescriptor for NativeAnalysisStub {
    fn meta(&self) -> &RuleMeta {
        &self.0
    }

    fn run(&self, _ir: &CommonIr) -> Vec<Finding> {
        Vec::new()
    }
}

/// Registers one native whole-graph/whole-repo analysis id as a toggle-only stub under `RuleKind::Native`,
/// `framework: "any"` (every native analysis is stack-agnostic — operates on the graph / git history /
/// schema IR / call graph / cross-tree join, never a specific frontend/backend framework), `enabled: true`.
/// This is the ONLY way a native analysis id enters a `RuleRegistry` — every owning rules crate's own
/// `register_native_analyses` calls this once per id it owns, so the actual id strings/severities live in
/// that crate, never here. See this module's doc for the full split.
pub fn register_native_analysis_stub(
    registry: &mut RuleRegistry,
    id: &str,
    default_severity: Severity,
) {
    registry.register(Box::new(NativeAnalysisStub(RuleMeta {
        id: id.to_string(),
        kind: RuleKind::Native,
        framework: "any".to_string(),
        enabled: true,
        default_severity,
    })));
}
