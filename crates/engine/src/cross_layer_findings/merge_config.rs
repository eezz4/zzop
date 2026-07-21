//! The two union `RuleConfig`s the cross-layer run derives from its trees' per-tree configs; the
//! contract prose lives on `compute_cross_layer_findings`'s own doc.

use std::collections::BTreeMap;
use std::path::PathBuf;

use zzop_core::{RuleConfig, Severity};

use crate::EngineConfig;

/// `(gate, merge_config)`:
///
/// - `gate` — disabledRules union (exclude-only): a cross-layer rule is disabled if its id appears
///   in ANY tree's `disabled_rules`. This is a joint-analysis output no single tree fully owns, so
///   any one tree opting out opts the whole run out of that rule.
/// - `merge_config` — severity-overrides union for the final `merge_findings` call. Cross-layer
///   findings are run-level while `severity_overrides` is a per-tree knob, so an override on ANY
///   tree takes effect; on a conflict (two trees override the SAME cross-layer rule id to
///   different severities) the FIRST-declaring tree wins, mirroring `analyze_trees`'
///   `host_owners` first-declarer precedent: trees may legitimately disagree, and first-declared
///   keeps the choice deterministic and input-order-stable. The override must be carried INTO the
///   merge (it runs before the sort there) — applying it after would leave a remapped finding in
///   its pre-override position (opus review, 2026-07-17 batch; sealed by
///   `merge_findings_sorts_by_the_overridden_severity_not_the_original` in zzop-core). Per-file
///   `suppressions` are a per-tree lever with no run-level meaning here, so they stay empty.
pub(super) fn union_configs(trees: &[(PathBuf, EngineConfig)]) -> (RuleConfig, RuleConfig) {
    let mut disabled_union: Vec<String> = Vec::new();
    for (_, config) in trees {
        disabled_union.extend(config.rule_config.disabled_rules.iter().cloned());
    }
    let gate = RuleConfig {
        disabled_rules: disabled_union,
        ..RuleConfig::default()
    };
    let mut severity_union: BTreeMap<String, Severity> = BTreeMap::new();
    for (_, config) in trees {
        for (rule_id, severity) in &config.rule_config.severity_overrides {
            severity_union.entry(rule_id.clone()).or_insert(*severity);
        }
    }
    let merge_config = RuleConfig {
        severity_overrides: severity_union,
        ..RuleConfig::default()
    };
    (gate, merge_config)
}
