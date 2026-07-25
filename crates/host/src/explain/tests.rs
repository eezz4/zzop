//! Unit tests over `super::explain_over` (the pure, parameterized lookup) — a fabricated pack list lets
//! this pin the lanes that have no REAL trigger among the bundled packs today: the ambiguous-bare-id lane
//! (see `explain_over`'s own doc) and the `symbol-scan` matcher kind (no bundled pack ships one). Everything
//! reachable through the REAL bundled data (happy path, unknown id, native id, whole-pack id, usage-shape
//! errors, and the per-matcher exclusion reporting for line/method/io-scan) is instead pinned end-to-end
//! against the real `zzop` binary in `packages/cli-bin/tests/cli.rs` — this file exists only for the lanes
//! that binary can never exercise.

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

/// The `symbol-scan` lane of the per-matcher reporting: it is the ONE kind with no exclusion field at all,
/// and it must say that rather than print an `exclude_pattern: no` that reads as "could carry one, doesn't".
/// Its suppress-marker answer stays the third one too (no line to anchor a comment against). No bundled
/// pack ships a `symbol-scan` rule, so this is the only place either answer can be pinned.
#[test]
fn a_symbol_scan_rule_reports_no_exclusion_field_and_no_usable_marker() {
    const SYMBOL_SCAN_PACK: &str = r#"{"id": "gamma", "rules": [
        {"id": "pascal", "severity": "info", "message": "m",
         "matcher": {"type": "symbol-scan", "file_pattern": "\\.tsx$", "name_pattern": "^[A-Z]"}}
    ]}"#;
    let packs = vec![parse_dsl_pack(SYMBOL_SCAN_PACK).expect("fixture pack must parse")];
    let out = explain_over(&packs, &[], "gamma/pascal").expect("full id must resolve");
    assert!(out.contains("matcher: symbol-scan"), "got: {out}");
    assert!(
        out.contains("exclusions: none (symbol-scan carries no exclusion field)"),
        "got: {out}"
    );
    assert!(
        !out.contains("exclude_pattern:"),
        "symbol-scan has no exclusion field, so no such line may be printed: {out}"
    );
    assert!(
        out.contains("suppress marker: none (symbol-scan findings have no line"),
        "got: {out}"
    );
}

/// A `line-scan` rule that sets NEITHER exclusion field still prints both field names with `no` — the
/// reader learns which exclusions this matcher kind offers, which is the half the old blanket line got
/// right and must not be lost while fixing the half it got wrong.
#[test]
fn a_line_scan_rule_with_no_exclusions_still_names_both_fields_it_could_have() {
    let packs = fabricated_packs();
    let out = explain_over(&packs, &[], "alpha/dup").expect("full id must resolve");
    assert!(out.contains("\nexclude_pattern: no"), "got: {out}");
    assert!(out.contains("\nfile_exclude_pattern: no"), "got: {out}");
}

/// The regression this lane exists for: `schema/dead-model` is the exact string real output puts in
/// `ruleId`, and it used to land in the "unknown rule id" lane — the tool answering "never heard of it"
/// about its own output while its family gate `schema-usage` got a tailored message. Seals that the label
/// resolves in BOTH forms a reader could type (namespaced as output spells it, and bare like every other
/// lane accepts) and that the answer names the id that actually gates it.
#[test]
fn a_schema_issue_label_is_named_as_a_label_with_its_gate_not_unknown() {
    let packs = fabricated_packs();
    let native = vec!["schema-usage".to_string(), "schema-structural".to_string()];
    for query in ["schema/dead-model", "dead-model"] {
        let err = explain_over(&packs, &native, query)
            .expect_err("an issue label carries no DSL rule data");
        assert!(err.contains("issue label"), "{query}: {err}");
        assert!(err.contains("schema-usage"), "{query}: {err}");
        assert!(
            !err.contains("unknown rule id"),
            "{query} must not fall to the unknown lane: {err}"
        );
    }
}

/// The gate id is DERIVED per label, not stamped from one family: a structural label must answer
/// `schema-structural`, not the `schema-usage` its sibling answers. A hand-kept list would have to get
/// this split right twice; the probe reads it off the real message-authoring function once.
#[test]
fn a_structural_issue_label_answers_its_own_family_gate() {
    let packs = fabricated_packs();
    let native = vec!["schema-usage".to_string(), "schema-structural".to_string()];
    let err = explain_over(&packs, &native, "schema/god-model").expect_err("a label is not a rule");
    assert!(err.contains("schema-structural"), "got: {err}");
    assert!(!err.contains("schema-usage"), "got: {err}");
}

/// Seals that the candidate gate ids come from the LIVE registry passed in, never a literal in
/// `explain.rs`: with no schema gate registered there is no family to name, so the label must fall
/// through to the unknown lane rather than assert a gate id that does not exist in this build.
#[test]
fn an_issue_label_needs_its_gate_in_the_registry_to_get_the_label_lane() {
    let packs = fabricated_packs();
    let err = explain_over(&packs, &["circular".to_string()], "schema/dead-model")
        .expect_err("no registered schema gate means no label lane");
    assert!(err.contains("unknown rule id"), "got: {err}");
}

/// The lane must not swallow the namespace: a `schema/`-prefixed string that is NOT a real label still
/// gets the unknown answer, so the prefix alone never manufactures a fake explanation.
#[test]
fn a_schema_namespaced_non_label_still_falls_to_the_unknown_lane() {
    let packs = fabricated_packs();
    let native = vec!["schema-usage".to_string(), "schema-structural".to_string()];
    let err = explain_over(&packs, &native, "schema/not-an-issue-label")
        .expect_err("an unrecognized label must fail");
    assert!(err.contains("unknown rule id"), "got: {err}");
}

/// An io-scan marker is read off the anchor line's source TEXT, and envelope mode supplies a constant
/// `None` anchor-line callback (`envelope::ingest`) — so the marker this command prints is inert there,
/// and `crates/engine/tests/analyze_io_scan_tree.rs` already pins that inertness end to end. Seals that
/// `explain` states the run-mode condition instead of handing a reader a comment that silently does
/// nothing in an envelope-fed pipeline. The other kinds must NOT carry the condition (it is false for
/// them — their marker is read from the file they scanned), which the second half checks.
#[test]
fn an_io_scan_marker_is_printed_with_its_native_parse_only_condition() {
    const IO_SCAN_PACK: &str = r#"{"id": "delta", "rules": [
        {"id": "provide-scan", "severity": "info", "message": "m",
         "matcher": {"type": "io-scan", "file_pattern": "\\.ts$", "direction": "provides"}}
    ]}"#;
    let packs = vec![parse_dsl_pack(IO_SCAN_PACK).expect("fixture pack must parse")];
    let out = explain_over(&packs, &[], "delta/provide-scan").expect("full id must resolve");
    assert!(out.contains("matcher: io-scan"), "got: {out}");
    assert!(
        out.contains("suppress marker: provide-scan-ok"),
        "got: {out}"
    );
    assert!(
        out.contains("NATIVE-PARSE RUNS ONLY"),
        "an io-scan marker must not be printed as unconditionally usable: {out}"
    );

    let line_scan = explain_over(&fabricated_packs(), &[], "alpha/dup").expect("must resolve");
    assert!(
        !line_scan.contains("NATIVE-PARSE RUNS ONLY"),
        "a line-scan marker IS read from the scanned file — the condition would be false here: {line_scan}"
    );
}

#[test]
fn a_truly_unknown_id_points_at_the_catalog() {
    let packs = fabricated_packs();
    let err = explain_over(&packs, &[], "no-such-rule-anywhere")
        .expect_err("an unrecognized id must fail");
    assert!(err.contains("unknown rule id"), "got: {err}");
    assert!(err.contains("rule-catalog"), "got: {err}");
}

/// Lane ORDER, where the two id namespaces overlap: `circular` is both a registered native analysis id
/// and a `zzop_metrics::roi::RecId` that `architecture.topRecommendation.id` can print. The native lane
/// runs first and must win — that is the accurate reading, since the recommendation IS the `circular`
/// analysis's own output ranked, and only the native id is a thing a caller can toggle.
#[test]
fn an_id_that_is_both_native_and_a_recommendation_answers_as_native() {
    let err = explain_over(&fabricated_packs(), &["circular".to_string()], "circular")
        .expect_err("a native id carries no DSL rule data");
    assert!(err.contains("native analysis id"), "got: {err}");
    assert!(!err.contains("RECOMMENDATION id"), "got: {err}");
}

/// Seals that the output-id lane is WIRED into the lookup (its own exhaustive coverage lives in
/// `output_ids.rs`): a disclosure class id must not reach the unknown lane even with an empty registry
/// and fabricated packs, because it is answered from the live disclosure registry, not from either.
#[test]
fn an_output_id_is_answered_after_every_rule_lane_misses() {
    let err = explain_over(&fabricated_packs(), &[], "stale-cache")
        .expect_err("a disclosure class is not a rule");
    assert!(err.contains("coverage-DISCLOSURE class id"), "got: {err}");
    assert!(!err.contains("unknown rule id"), "got: {err}");
}
