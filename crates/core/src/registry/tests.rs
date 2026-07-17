//! Uses synthetic example rule/analysis ids throughout, not real ids from the owning rules crates.
use std::collections::BTreeMap;

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
        glob: None,
    }
}

fn suppress_glob(rule: &str, glob: &str) -> Suppression {
    Suppression {
        rule: rule.to_string(),
        path: None,
        glob: Some(glob.to_string()),
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
fn glob_suppression_matches_full_path_with_brace_and_double_star() {
    // The review's motivating case: exempt Next.js app-router convention files anywhere under `app/`.
    let config = RuleConfig {
        suppressions: vec![suppress_glob(
            "dead-candidates",
            "**/app/**/{page,layout}.tsx",
        )],
        ..Default::default()
    };
    assert!(is_suppressed(
        &config,
        "dead-candidates",
        Some("web/app/(lang)/[lang]/page.tsx")
    ));
    assert!(is_suppressed(
        &config,
        "dead-candidates",
        Some("app/dashboard/layout.tsx")
    ));
    // A real dead file that is NOT an app-router convention file still fires.
    assert!(!is_suppressed(
        &config,
        "dead-candidates",
        Some("app/dashboard/helper.tsx")
    ));
    // `**` spans separators, so a nested app-router file still matches.
    assert!(is_suppressed(
        &config,
        "dead-candidates",
        Some("app/x/page.tsx")
    ));
    // `*` does NOT cross a path segment: a bare filename glob must not match a nested path.
    let single = RuleConfig {
        suppressions: vec![suppress_glob("dead-candidates", "*.tsx")],
        ..Default::default()
    };
    assert!(is_suppressed(&single, "dead-candidates", Some("page.tsx")));
    assert!(!is_suppressed(
        &single,
        "dead-candidates",
        Some("app/page.tsx")
    ));
}

#[test]
fn invalid_glob_suppresses_nothing() {
    // An unbalanced brace produces an invalid regex; it must fail safe (match nothing), not panic.
    let config = RuleConfig {
        suppressions: vec![suppress_glob("dead-candidates", "**/app/{page")],
        ..Default::default()
    };
    assert!(!is_suppressed(
        &config,
        "dead-candidates",
        Some("app/page.tsx")
    ));
}

#[test]
fn glob_takes_precedence_over_path_and_never_matches_a_fileless_finding() {
    let config = RuleConfig {
        suppressions: vec![suppress_glob("circular", "**/*.tsx")],
        ..Default::default()
    };
    assert!(!is_suppressed(&config, "circular", None));
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
fn global_exclude_glob_drops_a_finding_of_any_rule_id() {
    // The motivating case: one top-level exclude entry, applied rule-agnostically — no rule field to
    // match on, so ANY rule id in a matching file is suppressed.
    let config = RuleConfig {
        global_excludes: vec![GlobalExclude {
            path: None,
            glob: Some("**/*.stories.tsx".to_string()),
        }],
        ..Default::default()
    };
    assert!(is_suppressed(
        &config,
        "dead-candidates",
        Some("web/Button.stories.tsx")
    ));
    assert!(is_suppressed(
        &config,
        "circular",
        Some("web/Button.stories.tsx")
    ));
    assert!(!is_suppressed(&config, "circular", Some("web/Button.tsx")));
}

#[test]
fn filter_less_global_exclude_matches_nothing() {
    // A `GlobalExclude` with neither `path` nor `glob` (unreachable from the CLI, possible via a raw
    // addon request) must match NOTHING — the opposite of `Suppression`'s filter-less semantics —
    // because rule-agnostically it would otherwise drop every finding of the whole run silently. See
    // `global_exclude_matches_path`'s doc.
    let entry = GlobalExclude {
        path: None,
        glob: None,
    };
    assert!(!global_exclude_matches_path(&entry, "src/anything.ts"));
    let config = RuleConfig {
        global_excludes: vec![entry],
        ..Default::default()
    };
    assert!(!is_suppressed(&config, "circular", Some("src/anything.ts")));
}

#[test]
fn empty_global_excludes_changes_nothing() {
    // A whole config with an empty (default) global_excludes must not alter any existing suppression
    // behavior — the per-rule tests above still hold unmodified.
    let config = RuleConfig {
        suppressions: vec![suppress("nplus1", Some("legacy/"))],
        global_excludes: Vec::new(),
        ..Default::default()
    };
    assert!(is_suppressed(&config, "nplus1", Some("src/legacy/old.ts")));
    assert!(!is_suppressed(&config, "nplus1", Some("src/fresh/new.ts")));
    assert!(!is_suppressed(
        &config,
        "other-rule",
        Some("src/legacy/old.ts")
    ));
}

#[test]
fn global_exclude_path_substring_matches_across_rules() {
    let config = RuleConfig {
        global_excludes: vec![GlobalExclude {
            path: Some("legacy/".to_string()),
            glob: None,
        }],
        ..Default::default()
    };
    assert!(is_suppressed(
        &config,
        "any-rule-a",
        Some("src/legacy/x.ts")
    ));
    assert!(is_suppressed(
        &config,
        "any-rule-b",
        Some("src/legacy/y.ts")
    ));
    assert!(!is_suppressed(
        &config,
        "any-rule-a",
        Some("src/fresh/x.ts")
    ));
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
    overrides.insert("be-security/sql-taint".to_string(), Severity::Critical);
    let config = RuleConfig {
        severity_overrides: overrides,
        ..Default::default()
    };
    let f = finding("be-security/sql-taint", Severity::Warning, "C.java", 1);
    let overridden = apply_severity_override(&config, f);
    assert_eq!(overridden.severity, Severity::Critical);
}

#[test]
fn severity_override_leaves_unmatched_rule_unchanged() {
    let config = RuleConfig::default();
    let f = finding("be-security/sql-taint", Severity::Warning, "C.java", 1);
    let unchanged = apply_severity_override(&config, f);
    assert_eq!(unchanged.severity, Severity::Warning);
}

#[test]
fn merge_findings_sorts_by_the_overridden_severity_not_the_original() {
    // Pin for the "transform applied after the sort whose key it mutates" class (opus review,
    // 2026-07-17 cross-layer batch): the severity override runs INSIDE merge_findings, before its
    // sort — a caller that re-applied overrides after merging would emit a remapped finding stuck
    // in its pre-override position. An info finding overridden to critical must sort FIRST.
    let mut overrides = BTreeMap::new();
    overrides.insert("pack/promoted".to_string(), Severity::Critical);
    let config = RuleConfig {
        severity_overrides: overrides,
        ..Default::default()
    };
    let merged = merge_findings(
        vec![vec![
            finding("pack/steady", Severity::Warning, "a.ts", 1),
            finding("pack/promoted", Severity::Info, "z.ts", 9),
        ]],
        &config,
    );
    assert_eq!(
        merged
            .iter()
            .map(|f| (f.rule_id.as_str(), f.severity))
            .collect::<Vec<_>>(),
        vec![
            ("pack/promoted", Severity::Critical),
            ("pack/steady", Severity::Warning),
        ],
    );
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
