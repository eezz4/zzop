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

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

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

// ---------------------------------------------------------------------------------------------
// Config-driven gating
// ---------------------------------------------------------------------------------------------

/// One accepted-finding entry. `path` is a plain substring, not a glob — see `is_suppressed`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Suppression {
    /// The finding's stable rule id (a DSL pack rule id `"<pack>/<rule>"`, a native analysis id, or a JS
    /// quick-rule id) — matched for exact equality.
    pub rule: String,
    /// Optional path filter. Absent = suppress `rule` everywhere; present = suppress only findings whose
    /// file contains this string (case-sensitive substring containment, no glob engine — the plain-substring
    /// match is intentional/deterministic, not a shortcut).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// The one user-facing config shape every rule layer (native / DSL / JS) and every native analysis is
/// gated through. Covers the enabled/severity/disabled/suppressions surface — deliberately NOT
/// vocabulary/threshold plumbing (out of scope here; see module doc).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RuleConfig {
    /// Rule/pack/native-analysis ids to skip entirely. Exact string match against a rule's full id — no
    /// prefix/glob semantics.
    pub disabled_rules: Vec<String>,
    /// Per-rule severity remap, keyed by the same id space as `disabled_rules`. Exists because one unified
    /// registry spans native + DSL + JS and a user may want to promote/demote a specific id without forking
    /// the pack. `BTreeMap` (not `HashMap`) so config round-trips (serialize/compare/hash) are
    /// deterministic.
    pub severity_overrides: BTreeMap<String, Severity>,
    /// Finding-level accept-list. See `is_suppressed`.
    pub suppressions: Vec<Suppression>,
}

/// True if a finding for `rule` (optionally in `file`) is suppressed by `config.suppressions`: an entry
/// matches when its `rule` equals `rule` AND (the entry has no `path`, OR `file` is present and contains
/// the entry's `path`). Multiple entries for the same rule are OR-ed.
pub fn is_suppressed(config: &RuleConfig, rule: &str, file: Option<&str>) -> bool {
    config.suppressions.iter().any(|entry| {
        if entry.rule != rule {
            return false;
        }
        match &entry.path {
            None => true,
            Some(path) => file.is_some_and(|f| f.contains(path.as_str())),
        }
    })
}

/// True if `rule_id` is NOT in `config.disabled_rules` — exact string match, no prefix/glob semantics (see
/// `disabled_rules`'s own doc). Applies uniformly to a bare native-analysis/JS-quick-rule id, a whole DSL
/// pack id, or a full `"<pack>/<rule>"` id — the registry does not distinguish kinds here, it only compares
/// strings. All three id shapes are honored end to end: pack ids and `"<pack>/<rule>"` ids are both enforced
/// by `zzop_engine::pipeline::run_file_pass` before a pack ever reaches per-file evaluation (a disabled pack
/// id drops the whole pack; a disabled `"<pack>/<rule>"` id drops just that rule, via `gate_pack_rules`),
/// while bare native/JS ids are enforced at their own call sites (e.g. `register_native_analyses`'s ids
/// checked directly against `is_enabled` before the corresponding analysis runs).
pub fn is_enabled(config: &RuleConfig, rule_id: &str) -> bool {
    !config.disabled_rules.iter().any(|d| d == rule_id)
}

/// Returns `finding` with its severity replaced by `config.severity_overrides[finding.rule_id]`, if any
/// override is configured for that id; otherwise returns `finding` unchanged. See
/// `RuleConfig::severity_overrides` doc.
pub fn apply_severity_override(config: &RuleConfig, finding: Finding) -> Finding {
    match config.severity_overrides.get(&finding.rule_id) {
        Some(&severity) => Finding {
            severity,
            ..finding
        },
        None => finding,
    }
}

/// Severity sort rank: critical first, then warning, then info (the same order used for ranking
/// recommendation groups in `recommendations.rs`). The file/line/rule-id tie-breakers below give a
/// deterministic, human-scannable "worst-first, then file order" report.
fn severity_rank(s: Severity) -> u8 {
    match s {
        Severity::Critical => 0,
        Severity::Warning => 1,
        Severity::Info => 2,
    }
}

/// Merges findings from every rule source (native analyses, DSL packs, JS quick-rules) into one
/// deterministically ordered list: drops suppressed findings (`is_suppressed`), applies severity overrides
/// (`apply_severity_override`), then sorts by severity (critical < warning < info), then file, then line,
/// then rule id (see `severity_rank` doc for the sort's provenance/design-call note). Pure — no I/O, no
/// dependency on which layer produced a given `Vec<Finding>`.
pub fn merge_findings(sources: Vec<Vec<Finding>>, config: &RuleConfig) -> Vec<Finding> {
    let mut merged: Vec<Finding> = sources
        .into_iter()
        .flatten()
        .filter(|f| !is_suppressed(config, &f.rule_id, Some(f.file.as_str())))
        .map(|f| apply_severity_override(config, f))
        .collect();
    merged.sort_by(|a, b| {
        severity_rank(a.severity)
            .cmp(&severity_rank(b.severity))
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
            .then_with(|| a.rule_id.cmp(&b.rule_id))
    });
    merged
}

// ---------------------------------------------------------------------------------------------
// Native analysis registration mechanism — vocabulary-free
// ---------------------------------------------------------------------------------------------
// This crate (the kernel) carries ZERO rule vocabulary: no native analysis id, pack id, or rule id string
// literal lives here. What stays here is only the MECHANISM every owning rules crate uses to plug its own
// ids into the one shared registry. Each owning crate (`zzop_rules_graph`, `zzop_rules_schema`,
// `zzop_metrics`) exposes its own `register_native_analyses(&mut RuleRegistry)` that calls
// `register_native_analysis_stub` once per id it owns; `zzop_engine::register_all_native` composes all
// three. See `rules/README.md`'s "Adding a rule" section and `packages/engine/tests/rule_contracts.rs`'s
// "kernel is rule-vocabulary-free" contract test.

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
/// that crate, never here. See this section's module-level doc for the full split.
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

#[cfg(test)]
mod tests {
    //! Uses synthetic example rule/analysis ids throughout, not real ids from the owning rules crates.
    use super::*;

    fn finding(rule_id: &str, severity: Severity, file: &str, line: u32) -> Finding {
        Finding {
            rule_id: rule_id.to_string(),
            severity,
            file: file.to_string(),
            line,
            message: "m".to_string(),
            data: None,
        }
    }

    fn suppress(rule: &str, path: Option<&str>) -> Suppression {
        Suppression {
            rule: rule.to_string(),
            path: path.map(str::to_string),
        }
    }

    #[test]
    fn default_empty_suppressions_suppresses_nothing() {
        let config = RuleConfig::default();
        assert!(!is_suppressed(
            &config,
            "raceConditionTOCTOU",
            Some("api/x.ts")
        ));
    }

    #[test]
    fn bare_rule_no_path_suppresses_everywhere() {
        let config = RuleConfig {
            suppressions: vec![suppress("raceConditionTOCTOU", None)],
            ..Default::default()
        };
        assert!(is_suppressed(
            &config,
            "raceConditionTOCTOU",
            Some("api/x.ts")
        ));
        assert!(is_suppressed(&config, "raceConditionTOCTOU", None));
        assert!(!is_suppressed(&config, "nplus1", Some("api/x.ts")));
    }

    #[test]
    fn rule_plus_path_suppresses_only_matching_files_substring() {
        let config = RuleConfig {
            suppressions: vec![suppress("nplus1", Some("legacy/"))],
            ..Default::default()
        };
        assert!(is_suppressed(&config, "nplus1", Some("src/legacy/old.ts")));
        assert!(!is_suppressed(&config, "nplus1", Some("src/fresh/new.ts")));
        // path-qualified entry cannot match a fileless finding
        assert!(!is_suppressed(&config, "nplus1", None));
    }

    #[test]
    fn multiple_entries_for_the_same_rule_are_or_ed() {
        let config = RuleConfig {
            suppressions: vec![
                suppress("weakCrypto", Some("vendor/")),
                suppress("weakCrypto", Some("scripts/")),
            ],
            ..Default::default()
        };
        assert!(is_suppressed(&config, "weakCrypto", Some("vendor/a.ts")));
        assert!(is_suppressed(&config, "weakCrypto", Some("scripts/b.ts")));
        assert!(!is_suppressed(&config, "weakCrypto", Some("src/c.ts")));
    }

    #[test]
    fn disabled_rules_defaults_to_all_enabled() {
        let config = RuleConfig::default();
        assert!(is_enabled(&config, "circular"));
    }

    #[test]
    fn disabled_rules_skips_by_exact_id() {
        let config = RuleConfig {
            disabled_rules: vec!["circular".to_string()],
            ..Default::default()
        };
        assert!(!is_enabled(&config, "circular"));
        assert!(is_enabled(&config, "unreachable"));
        // exact match only — a full "pack/rule" id is unaffected by disabling the bare pack id.
        assert!(is_enabled(&config, "circular/sub-rule"));
    }

    #[test]
    fn disabled_rules_skips_by_full_pack_slash_rule_id_without_affecting_sibling_rules() {
        // A `"<pack>/<rule>"` entry disables only that one rule, leaving the bare pack id and every
        // other rule in the same pack enabled. The per-rule pack filtering that makes this id shape
        // take effect against real `RulePackDef`s lives in `zzop_engine::pipeline::gate_pack_rules`,
        // downstream of this crate — this test only covers `is_enabled`'s own string-matching contract.
        let config = RuleConfig {
            disabled_rules: vec!["typescript/as-cast".to_string()],
            ..Default::default()
        };
        assert!(!is_enabled(&config, "typescript/as-cast"));
        assert!(is_enabled(&config, "typescript/no-explicit-any"));
        assert!(is_enabled(&config, "typescript"));
    }

    #[test]
    fn severity_override_replaces_matching_rule_severity() {
        let mut overrides = BTreeMap::new();
        overrides.insert("java-security/sql-taint".to_string(), Severity::Critical);
        let config = RuleConfig {
            severity_overrides: overrides,
            ..Default::default()
        };
        let f = finding("java-security/sql-taint", Severity::Warning, "C.java", 1);
        let overridden = apply_severity_override(&config, f);
        assert_eq!(overridden.severity, Severity::Critical);
    }

    #[test]
    fn severity_override_leaves_unmatched_rule_unchanged() {
        let config = RuleConfig::default();
        let f = finding("java-security/sql-taint", Severity::Warning, "C.java", 1);
        let unchanged = apply_severity_override(&config, f);
        assert_eq!(unchanged.severity, Severity::Warning);
    }

    #[test]
    fn merge_findings_drops_suppressed_and_sorts_severity_file_line_rule() {
        let config = RuleConfig {
            suppressions: vec![suppress("noisy", None)],
            ..Default::default()
        };
        let a = vec![
            finding("noisy", Severity::Critical, "z.ts", 1),
            finding("b-rule", Severity::Info, "b.ts", 5),
        ];
        let b = vec![
            finding("a-rule", Severity::Critical, "a.ts", 10),
            finding("c-rule", Severity::Warning, "a.ts", 2),
        ];
        let merged = merge_findings(vec![a, b], &config);
        let ids: Vec<&str> = merged.iter().map(|f| f.rule_id.as_str()).collect();
        // "noisy" suppressed; critical (a-rule) before warning (c-rule) before info (b-rule).
        assert_eq!(ids, vec!["a-rule", "c-rule", "b-rule"]);
    }

    #[test]
    fn merge_findings_applies_severity_overrides_before_sorting() {
        let mut overrides = BTreeMap::new();
        overrides.insert("promoted".to_string(), Severity::Critical);
        let config = RuleConfig {
            severity_overrides: overrides,
            ..Default::default()
        };
        let findings = vec![vec![
            finding("kept-warning", Severity::Warning, "a.ts", 1),
            finding("promoted", Severity::Info, "b.ts", 1),
        ]];
        let merged = merge_findings(findings, &config);
        assert_eq!(merged[0].rule_id, "promoted");
        assert_eq!(merged[0].severity, Severity::Critical);
    }

    #[test]
    fn merge_findings_ties_break_on_file_then_line_then_rule_id() {
        let config = RuleConfig::default();
        let findings = vec![vec![
            finding("z-rule", Severity::Warning, "a.ts", 3),
            finding("a-rule", Severity::Warning, "a.ts", 3),
            finding("m-rule", Severity::Warning, "a.ts", 1),
            finding("m-rule", Severity::Warning, "b.ts", 1),
        ]];
        let merged = merge_findings(findings, &config);
        let keys: Vec<(String, u32, String)> = merged
            .iter()
            .map(|f| (f.file.clone(), f.line, f.rule_id.clone()))
            .collect();
        assert_eq!(
            keys,
            vec![
                ("a.ts".to_string(), 1, "m-rule".to_string()),
                ("a.ts".to_string(), 3, "a-rule".to_string()),
                ("a.ts".to_string(), 3, "z-rule".to_string()),
                ("b.ts".to_string(), 1, "m-rule".to_string()),
            ]
        );
    }

    #[test]
    fn register_native_analysis_stub_registers_one_native_enabled_toggle_point() {
        let mut registry = RuleRegistry::new();
        register_native_analysis_stub(&mut registry, "example-analysis", Severity::Warning);
        let metas = registry.metas();
        assert_eq!(metas.len(), 1);
        let meta = metas[0];
        assert_eq!(meta.id, "example-analysis");
        assert_eq!(meta.kind, RuleKind::Native);
        assert_eq!(meta.framework, "any");
        assert!(meta.enabled);
        assert_eq!(meta.default_severity, Severity::Warning);
    }

    #[test]
    fn gating_config_toggles_a_native_analysis_stub_id() {
        let mut registry = RuleRegistry::new();
        register_native_analysis_stub(&mut registry, "example-analysis", Severity::Warning);
        register_native_analysis_stub(&mut registry, "other-analysis", Severity::Info);
        let config = RuleConfig {
            disabled_rules: vec!["example-analysis".to_string()],
            ..Default::default()
        };
        let enabled_ids: Vec<&str> = registry
            .metas()
            .iter()
            .filter(|m| is_enabled(&config, &m.id))
            .map(|m| m.id.as_str())
            .collect();
        assert!(!enabled_ids.contains(&"example-analysis"));
        assert!(enabled_ids.contains(&"other-analysis"));
    }
}
