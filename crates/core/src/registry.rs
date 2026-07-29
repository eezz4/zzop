//! Rule registry — the set of native analysis ids, plus the config-driven gating every rule id
//! (native analysis, DSL pack, or `"<pack>/<rule>"`) is toggled through.
//! "Native" is only where a rule is compiled, not "always runs": a native analysis id lands in the same
//! `RuleConfig` id space as a DSL pack rule, so it can be disabled / severity-overridden / suppressed
//! exactly like one.
//!
//! ## What this registry is NOT
//! It does not dispatch rules. DSL packs are interpreted straight from `RulePackDef` by
//! `zzop_engine::pipeline` (gated by `gate_pack_rules` + `is_enabled`), and each native analysis is
//! invoked directly by the orchestrator with its own inputs (`DepGraph`, `CouplingMap`, the cross-tree
//! join, ...), never through here. What the registry answers is one question — "which native analysis
//! ids exist" — for enumeration (`zzop explain`'s "this id is native, not missing" lane, the
//! coverage/capability diagnostics) and for cross-checking a config id against reality.
//!
//! ## Config-driven gating
//! `RuleConfig` is the one user-facing shape every rule id is gated through:
//! `disabled_rules` (a pack/rule/analysis skipped entirely), `suppressions` (finding-level accept-list),
//! `severity_overrides` (per-rule severity remap — see `apply_severity_override` doc for why this exists).
//! A resolve-with-defaults spread that composes a "default config" for `disabled_rules`/`suppressions` is
//! intentionally NOT implemented here: this crate has no such notion yet (that lives with whatever loads
//! user config into `RuleConfig` — out of this module's scope).
//!
//! Split across submodules (paths under `crate::registry::` are unchanged): `config` (the `RuleConfig`
//! gating surface), `merge` (the deterministic finding merge/sort), `native_stub` (the vocabulary-free
//! native-analysis registration mechanism). The registry type itself stays in this root file.

mod config;
mod merge;
mod native_stub;
mod redact;
#[cfg(test)]
mod tests;

pub use config::{
    apply_severity_override, global_exclude_matches_path, is_enabled, is_suppressed,
    suppression_matches_path, GlobalExclude, RuleConfig, Suppression,
};
pub use merge::merge_findings;
pub use native_stub::register_native_analysis_stub;
pub use redact::REDACTED;

/// The native analysis ids registered at boot — every owning rules crate's `register_native_analyses`
/// plugs its own ids in via `register_native_analysis_stub`, and `zzop_engine::register_all_native`
/// composes them all into one of these.
#[derive(Debug, Default)]
pub struct RuleRegistry {
    ids: Vec<String>,
}

impl RuleRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Every registered native analysis id — the enumeration a `--list`/`--rulepacks`-style command or
    /// the config-vs-registry cross-check (an unknown `disabled_rules` id) would read. Registration
    /// order (`register_all_native`'s crate order, then each crate's own table order).
    pub fn ids(&self) -> &[String] {
        &self.ids
    }
}
