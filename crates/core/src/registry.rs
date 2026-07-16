//! Rule registry — unifies the three layers (native / DSL / JS) under a single registry and metadata.
//! "Native" is only where a rule is compiled, not "always runs" — every rule is toggled/gated via metadata
//! (enabled / severity / appliesTo).
//!
//! ## Config-driven gating
//! `RuleConfig` is the one user-facing shape all three rule layers (and native analyses) are gated through:
//! `disabled_rules` (a pack/analysis skipped entirely), `suppressions` (finding-level accept-list),
//! `severity_overrides` (per-rule severity remap — see `apply_severity_override` doc for why this exists).
//! A resolve-with-defaults spread that composes a "default config" for `disabled_rules`/`suppressions` is
//! intentionally NOT implemented here: this crate has no such notion yet (that lives with whatever loads
//! user config into `RuleConfig` — out of this module's scope).
//!
//! Split across submodules (paths under `crate::registry::` are unchanged): `config` (the `RuleConfig`
//! gating surface), `merge` (the deterministic finding merge/sort), `native_stub` (the vocabulary-free
//! native-analysis registration mechanism). The core registry types stay in this root file.

mod config;
mod merge;
mod native_stub;
#[cfg(test)]
mod tests;

pub use config::{
    apply_severity_override, global_exclude_matches_path, is_enabled, is_suppressed,
    suppression_matches_path, GlobalExclude, RuleConfig, Suppression,
};
pub use merge::merge_findings;
pub use native_stub::register_native_analysis_stub;

use crate::{finding::Finding, ir::CommonIr, Severity};

/// Where a rule executes — the toggle experience is identical, only the dispatch path differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleKind {
    /// Native rule statically linked into core (whole-graph). rules/native/*.
    Native,
    /// Declarative DSL rule pack (language/environment). rules/dsl/*.json — interpreted natively by the engine, shipped as data.
    Dsl,
    /// A build-free JS/TS quick-custom rule running over the IR in the Node host (escape hatch for arbitrary logic).
    Js,
}

/// A single toggle/gating metadata shared by all three layers. Overridable via config.
#[derive(Debug, Clone)]
pub struct RuleMeta {
    pub id: String,
    pub kind: RuleKind,
    /// Applicable framework ("any" | "react" | "prisma" | ...). Gates on the target environment.
    pub framework: String,
    /// on/off — even a native analysis can be turned off (e.g. disable circular).
    pub enabled: bool,
    /// Default severity (overridable via config).
    pub default_severity: Severity,
}

impl RuleMeta {
    /// Target gating — false skips this rule for the target.
    pub fn applies_to(&self, _target: &str) -> bool {
        self.enabled
    }
}

/// The trait a native rule implements. DSL packs / JS rules are adapted into this shape by the loader.
/// oxlint-style optimization: the engine traverses the IR once and dispatches to subscribed rules (no per-rule re-walk).
pub trait RuleDescriptor {
    fn meta(&self) -> &RuleMeta;
    /// Run against one tree's Common IR -> findings.
    fn run(&self, ir: &CommonIr) -> Vec<Finding>;
}

/// Native rules register dynamically at boot; DSL packs are added by the loader.
#[derive(Default)]
pub struct RuleRegistry {
    rules: Vec<Box<dyn RuleDescriptor>>,
}

impl RuleRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, rule: Box<dyn RuleDescriptor>) {
        self.rules.push(rule);
    }

    /// Run only the rules that apply to the target and are enabled (gating).
    /// TODO(Phase 4): replace with a single-traversal + node-kind subscription dispatch (currently per-rule run).
    pub fn run_all(&self, ir: &CommonIr, target: &str) -> Vec<Finding> {
        self.rules
            .iter()
            .filter(|r| r.meta().applies_to(target))
            .flat_map(|r| r.run(ir))
            .collect()
    }

    /// Every registered rule's metadata — the enumeration a `--list`/`--rulepacks`-style command or the
    /// config-vs-registry cross-check (an unknown `disabled_rules` id) would read. Registration order
    /// (native analyses first via `register_native_analyses`, then whatever the caller adds after).
    pub fn metas(&self) -> Vec<&RuleMeta> {
        self.rules.iter().map(|r| r.meta()).collect()
    }
}
