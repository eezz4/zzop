//! Unit tests over `super::explain_over` (the pure, parameterized lookup) — a fabricated pack list lets
//! this pin the ambiguous-bare-id lane, which has no REAL trigger among the bundled packs today (see
//! `explain_over`'s own doc). Everything reachable through the REAL bundled data (happy path, unknown
//! id, native id, whole-pack id, usage-shape errors) is instead pinned end-to-end against the real
//! `zzop` binary in `packages/cli-bin/tests/cli.rs` — this file exists only for the one lane that binary
//! can never exercise.

use super::explain_over;
use zzop_core::parse_dsl_pack;

const LINE_SCAN_PACK: &str = r#"{"id": "alpha", "rules": [
    {"id": "dup", "severity": "info", "message": "m",
     "matcher": {"type": "line-scan", "file_pattern": "\\.ts$", "line_pattern": "TODO"}}
]}"#;

const OTHER_LINE_SCAN_PACK: &str = r#"{"id": "beta", "rules": [
    {"id": "dup", "severity": "warning", "message": "m2",
     "matcher": {"type": "line-scan", "file_pattern": "\\.ts$", "line_pattern": "FIXME"}}
]}"#;

fn fabricated_packs() -> Vec<zzop_core::RulePackDef> {
    vec![
        parse_dsl_pack(LINE_SCAN_PACK).expect("fixture pack must parse"),
        parse_dsl_pack(OTHER_LINE_SCAN_PACK).expect("fixture pack must parse"),
    ]
}

#[test]
fn a_bare_id_shared_by_two_packs_is_ambiguous_and_lists_both_full_ids() {
    let packs = fabricated_packs();
    let err = explain_over(&packs, &[], "dup").expect_err("shared bare id must not resolve");
    assert!(err.contains("ambiguous"), "got: {err}");
    assert!(err.contains("alpha/dup"), "got: {err}");
    assert!(err.contains("beta/dup"), "got: {err}");
}

#[test]
fn the_full_id_form_resolves_even_though_the_bare_id_is_ambiguous() {
    let packs = fabricated_packs();
    let out =
        explain_over(&packs, &[], "alpha/dup").expect("full id must resolve deterministically");
    assert!(out.contains("id: alpha/dup"), "got: {out}");
    assert!(out.contains("suppress marker: dup-ok"), "got: {out}");
}

#[test]
fn a_whole_pack_id_is_a_hint_not_a_rule() {
    let packs = fabricated_packs();
    let err = explain_over(&packs, &[], "alpha").expect_err("a pack id names no single rule");
    assert!(err.contains("PACK"), "got: {err}");
    assert!(err.contains("alpha/dup"), "got: {err}");
}

#[test]
fn a_native_id_is_named_as_native_not_unknown() {
    let packs = fabricated_packs();
    let err = explain_over(&packs, &["circular".to_string()], "circular")
        .expect_err("a native analysis id carries no DSL rule data");
    assert!(err.contains("native analysis id"), "got: {err}");
    assert!(err.contains("rule-catalog"), "got: {err}");
}

#[test]
fn a_truly_unknown_id_points_at_the_catalog() {
    let packs = fabricated_packs();
    let err = explain_over(&packs, &[], "no-such-rule-anywhere")
        .expect_err("an unrecognized id must fail");
    assert!(err.contains("unknown rule id"), "got: {err}");
    assert!(err.contains("rule-catalog"), "got: {err}");
}
