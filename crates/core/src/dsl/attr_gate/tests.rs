//! Unit coverage of the attribute-gate post-pass. Each test pins one contract of
//! `super::apply_attr_gates`; the end-to-end wiring (that the engine actually calls it on both analysis
//! paths, and that a real pack's rule goes silent) is pinned in `rules/dsl/reliability/env_outside_config.rs`.

use serde_json::json;

use super::apply_attr_gates;
use crate::attributes::{Attribute, AttributeStore, EntityRef};
use crate::dsl::{LineScan, Matcher, RuleDef, RulePackDef};
use crate::{Finding, RuleConfig, Severity};

fn pack(matcher: LineScan) -> RulePackDef {
    RulePackDef {
        id: "p".to_string(),
        framework: "any".to_string(),
        schema_version: 1,
        fragments: Default::default(),
        rules: vec![RuleDef {
            id: "r".to_string(),
            severity: Severity::Info,
            message: "m".to_string(),
            matcher: Matcher::LineScan(matcher),
        }],
    }
}

fn finding(rule_id: &str, file: &str) -> Finding {
    Finding {
        rule_id: rule_id.to_string(),
        severity: Severity::Info,
        file: file.to_string(),
        line: 1,
        message: "m".to_string(),
        evidence_paths: Vec::new(),
        data: None,
    }
}

fn scope_store(prefix: &str, key: &str) -> AttributeStore {
    AttributeStore::from_attrs(vec![Attribute {
        target: EntityRef::PathScope {
            prefix: prefix.to_string(),
        },
        key: key.to_string(),
        value: json!(true),
    }])
}

#[test]
fn attr_absent_drops_only_findings_in_the_declared_scope() {
    let packs = vec![pack(LineScan {
        attr_absent: Some("env-config-module".to_string()),
        ..LineScan::default()
    })];
    let mut findings = vec![
        finding("p/r", "src/config/db.ts"),
        finding("p/r", "src/app/page.tsx"),
    ];
    let warnings = apply_attr_gates(
        &packs,
        &RuleConfig::default(),
        &scope_store("src/config", "env-config-module"),
        &mut findings,
    );
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].file, "src/app/page.tsx");
    assert!(
        warnings.is_empty(),
        "a satisfied gate discloses nothing: {warnings:?}"
    );
}

#[test]
fn attr_present_keeps_only_findings_in_the_declared_scope() {
    // The mirror polarity. Pinned because the pair is the whole surface `io-scan` already exposes, and an
    // asymmetric line-scan version would be a trap for an author porting a rule between the two.
    let packs = vec![pack(LineScan {
        attr_present: Some("generated".to_string()),
        ..LineScan::default()
    })];
    let mut findings = vec![finding("p/r", "gen/api.ts"), finding("p/r", "src/app.ts")];
    apply_attr_gates(
        &packs,
        &RuleConfig::default(),
        &scope_store("gen", "generated"),
        &mut findings,
    );
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].file, "gen/api.ts");
}

#[test]
fn an_undeclared_required_key_drops_every_finding_and_discloses_once() {
    // THE contract this whole task exists for: undeclared means the rule did not run, and "did not run"
    // is stated out loud rather than left to look like a clean result.
    let packs = vec![pack(LineScan {
        attr_absent: Some("env-config-module".to_string()),
        require_attr_declared: Some("env-config-module".to_string()),
        ..LineScan::default()
    })];
    let mut findings = vec![
        finding("p/r", "src/a.ts"),
        finding("p/r", "src/b.ts"),
        finding("other/rule", "src/c.ts"),
    ];
    let warnings = apply_attr_gates(
        &packs,
        &RuleConfig::default(),
        &AttributeStore::default(),
        &mut findings,
    );
    assert_eq!(
        findings.len(),
        1,
        "only the unrelated rule's finding survives: {findings:?}"
    );
    assert_eq!(findings[0].rule_id, "other/rule");
    assert_eq!(warnings.len(), 1, "one disclosure per silenced rule");
    let w = &warnings[0];
    assert!(w.contains("p/r"), "names the rule: {w}");
    assert!(w.contains("env-config-module"), "names the key: {w}");
    assert!(w.contains("2 candidate sites"), "states the volume: {w}");
    assert!(w.contains("overlays"), "says how to declare it: {w}");
    assert!(w.contains("\"off\""), "says how to opt out instead: {w}");
}

#[test]
fn a_silenced_rule_with_nothing_to_say_discloses_nothing() {
    // Honest-channel boundary (`output-philosophy` §1): a tree with no candidate sites has a real `0`,
    // and warning about it would train an agent to ignore the warning that matters.
    let packs = vec![pack(LineScan {
        require_attr_declared: Some("env-config-module".to_string()),
        ..LineScan::default()
    })];
    let mut findings = vec![finding("other/rule", "src/c.ts")];
    let warnings = apply_attr_gates(
        &packs,
        &RuleConfig::default(),
        &AttributeStore::default(),
        &mut findings,
    );
    assert_eq!(findings.len(), 1);
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn a_declared_key_re_enables_the_rule_even_when_every_value_is_falsy() {
    // `declares` is about vocabulary, not truthiness: a producer that says "nothing here is a config
    // module" has still answered the question, so the rule must run rather than disclose.
    let packs = vec![pack(LineScan {
        attr_absent: Some("env-config-module".to_string()),
        require_attr_declared: Some("env-config-module".to_string()),
        ..LineScan::default()
    })];
    let mut findings = vec![finding("p/r", "src/a.ts")];
    let store = AttributeStore::from_attrs(vec![Attribute {
        target: EntityRef::PathScope {
            prefix: "src/config".to_string(),
        },
        key: "env-config-module".to_string(),
        value: json!(false),
    }]);
    let warnings = apply_attr_gates(&packs, &RuleConfig::default(), &store, &mut findings);
    assert_eq!(findings.len(), 1, "the rule ran: {findings:?}");
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn a_disabled_rule_is_neither_filtered_nor_disclosed() {
    // A rule the user turned off must not report that it did not run — it did not run on purpose, and a
    // warning there would be noise the user cannot act on without re-enabling the thing they disabled.
    let packs = vec![pack(LineScan {
        require_attr_declared: Some("env-config-module".to_string()),
        ..LineScan::default()
    })];
    let mut findings = vec![finding("p/r", "src/a.ts")];
    let rule_config = RuleConfig {
        disabled_rules: vec!["p/r".to_string()],
        ..RuleConfig::default()
    };
    let warnings = apply_attr_gates(
        &packs,
        &rule_config,
        &AttributeStore::default(),
        &mut findings,
    );
    assert_eq!(findings.len(), 1);
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn a_pack_with_no_attribute_gate_is_untouched_in_order_and_content() {
    // The no-op contract: every shipped pack today declares no gate, so this pass must be provably inert
    // for them — including finding ORDER, which the output's determinism contract depends on.
    let packs = vec![pack(LineScan::default())];
    let before = vec![
        finding("p/r", "b.ts"),
        finding("other/rule", "a.ts"),
        finding("p/r", "c.ts"),
    ];
    let mut findings = before.clone();
    let warnings = apply_attr_gates(
        &packs,
        &RuleConfig::default(),
        &scope_store("src", "anything"),
        &mut findings,
    );
    assert!(warnings.is_empty());
    let files: Vec<&str> = findings.iter().map(|f| f.file.as_str()).collect();
    assert_eq!(files, vec!["b.ts", "a.ts", "c.ts"]);
}
