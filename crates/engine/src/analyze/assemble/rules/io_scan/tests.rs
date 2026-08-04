//! Profiling wiring for the whole-tree `Matcher::IoScan` pass — the channel `super::run`'s doc used to
//! declare a v1 gap. The load-bearing claim is not merely "an id appears": the per-file pass ALREADY
//! emits a placeholder `RuleTiming` for an io-scan rule (`dsl::eval_pack_impl`'s no-op `IoScan` arm times
//! a rule that did nothing, always `findings: 0`), so these tests pin the part only this pass can
//! provide — an io-scan rule's own whole-tree time AND its real finding count, keyed `"{pack}/{rule}"`.

use std::collections::{BTreeSet, HashMap};

use zzop_core::{AttributeStore, IoProvide, RulePackDef};

use crate::EngineConfig;

/// One io-scan rule (fires on an `/admin/` http provide) plus one line-scan rule that this pass must
/// ignore entirely — the negative half of the wiring: only rules this pass actually ran may be timed.
fn pack() -> RulePackDef {
    zzop_core::parse_dsl_pack(
        r#"{
          "id": "t",
          "rules": [
            {
              "id": "admin-route",
              "severity": "warning",
              "message": "admin route",
              "matcher": {
                "type": "io-scan",
                "file_pattern": "\\.ts$",
                "direction": "provides",
                "kind": "http",
                "key_pattern": "/admin(/|$)"
              }
            },
            {
              "id": "never-here",
              "severity": "info",
              "message": "line rule",
              "matcher": { "type": "line-scan", "file_pattern": "\\.ts$", "line_pattern": "TODO" }
            }
          ]
        }"#,
    )
    .expect("test pack parses")
}

fn provides() -> Vec<IoProvide> {
    vec![
        IoProvide {
            response: None,
            kind: "http".to_string(),
            key: "GET /admin/users".to_string(),
            file: "src/routes.ts".to_string(),
            line: 12,
            symbol: None,
            body: None,
        },
        IoProvide {
            response: None,
            kind: "http".to_string(),
            key: "GET /public/health".to_string(),
            file: "src/routes.ts".to_string(),
            line: 20,
            symbol: None,
            body: None,
        },
    ]
}

fn config(profile_rules: bool) -> EngineConfig {
    EngineConfig {
        source_id: "fixture".to_string(),
        packs: vec![pack()],
        profile_rules,
        ..EngineConfig::default()
    }
}

/// `root` points at a directory with no such file on purpose: `anchor_line` yields `None` for an
/// unreadable file, which every gate treats as "nothing to check" — so the rule's outcome here depends
/// only on the IO facts, no fixture source text needed.
fn run_pass(profile_rules: bool) -> (Vec<zzop_core::Finding>, HashMap<String, (u128, usize)>) {
    let mut rule_time = HashMap::new();
    let findings = super::run(
        std::path::Path::new("."),
        &config(profile_rules),
        &provides(),
        &[],
        &AttributeStore::default(),
        &BTreeSet::new(),
        &mut rule_time,
    );
    (findings, rule_time)
}

#[test]
fn profiled_run_records_the_io_scan_rule_under_its_pack_qualified_id() {
    let (findings, rule_time) = run_pass(true);
    assert_eq!(findings.len(), 1, "{findings:?}");

    let (_nanos, timed_findings) = rule_time
        .get("t/admin-route")
        .copied()
        .unwrap_or_else(|| panic!("io-scan rule missing from rule_timings: {rule_time:?}"));
    assert_eq!(
        timed_findings, 1,
        "the timed finding count must be this pass's own, not the per-file placeholder's 0"
    );
}

#[test]
fn profiled_run_times_no_rule_this_pass_did_not_run() {
    let (_, rule_time) = run_pass(true);
    assert!(
        !rule_time.contains_key("t/never-here"),
        "line-scan rule is the per-file pass's to time, not this one's: {rule_time:?}"
    );
    assert_eq!(rule_time.len(), 1, "{rule_time:?}");
}

#[test]
fn profiling_leaves_findings_byte_identical_and_costs_nothing_when_off() {
    let (profiled, _) = run_pass(true);
    let (unprofiled, rule_time) = run_pass(false);
    assert!(
        rule_time.is_empty(),
        "profiling off must record nothing: {rule_time:?}"
    );
    assert_eq!(
        serde_json::to_value(&profiled).unwrap(),
        serde_json::to_value(&unprofiled).unwrap(),
        "splitting the pack per rule must not perturb findings or their order"
    );
}
