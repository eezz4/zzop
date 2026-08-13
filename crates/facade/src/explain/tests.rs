//! Unit tests over `super::explain_over` (the pure, parameterized lookup) — a fabricated pack list lets
//! this pin the lanes that have no REAL trigger among the bundled packs today: the ambiguous-bare-id lane
//! (see `explain_over`'s own doc) and the `symbol-scan` matcher kind (no bundled pack ships one). Everything
//! reachable through the REAL bundled data (happy path, unknown id, native id, whole-pack id, usage-shape
//! errors, and the per-matcher exclusion reporting for line/method/io-scan) is instead pinned end-to-end
//! against the real `zzop` binary in `packages/cli-bin/tests/cli.rs` — this file exists only for the lanes
//! that binary can never exercise.

use super::{explain_over, Corpus};
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
    let err = explain_over(&packs, &[], "dup", Corpus::Bundled)
        .expect_err("shared bare id must not resolve");
    assert!(err.contains("ambiguous"), "got: {err}");
    assert!(err.contains("alpha/dup"), "got: {err}");
    assert!(err.contains("beta/dup"), "got: {err}");
}

#[test]
fn the_full_id_form_resolves_even_though_the_bare_id_is_ambiguous() {
    let packs = fabricated_packs();
    let out = explain_over(&packs, &[], "alpha/dup", Corpus::Bundled)
        .expect("full id must resolve deterministically");
    assert!(out.contains("id: alpha/dup"), "got: {out}");
    assert!(out.contains("suppress marker: zzop-dup-ok"), "got: {out}");
}

#[test]
fn a_whole_pack_id_is_a_hint_not_a_rule() {
    let packs = fabricated_packs();
    let err = explain_over(&packs, &[], "alpha", Corpus::Bundled)
        .expect_err("a pack id names no single rule");
    assert!(err.contains("PACK"), "got: {err}");
    assert!(err.contains("alpha/dup"), "got: {err}");
}

#[test]
fn a_native_id_is_named_as_native_not_unknown() {
    let packs = fabricated_packs();
    let err = explain_over(
        &packs,
        &["circular".to_string()],
        "circular",
        Corpus::Bundled,
    )
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
    let out =
        explain_over(&packs, &[], "gamma/pascal", Corpus::Bundled).expect("full id must resolve");
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
    let out =
        explain_over(&packs, &[], "alpha/dup", Corpus::Bundled).expect("full id must resolve");
    assert!(out.contains("\nexclude_pattern: no"), "got: {out}");
    assert!(out.contains("\nfile_exclude_pattern: no"), "got: {out}");
}

/// The regression this lane exists for: `schema/unreferenced-model-name` is the exact string real output
/// puts in `ruleId`, and it used to land in the "unknown rule id" lane — the tool answering "never heard
/// of it" about its own output. Now that the 12 issue labels are registered analyses, the namespaced form
/// is a plain native id, and the BARE form a reader might type instead resolves to it by name.
#[test]
fn a_schema_issue_id_answers_in_both_the_namespaced_and_the_bare_form() {
    let packs = fabricated_packs();
    let native = vec![
        "schema-usage".to_string(),
        "schema/unreferenced-model-name".to_string(),
    ];
    for query in ["schema/unreferenced-model-name", "unreferenced-model-name"] {
        let err = explain_over(&packs, &native, query, Corpus::Bundled)
            .expect_err("a native analysis carries no DSL rule data");
        assert!(err.contains("native analysis id"), "{query}: {err}");
        assert!(
            err.contains("schema/unreferenced-model-name"),
            "{query} must name the full id config actually matches: {err}"
        );
        assert!(
            !err.contains("unknown rule id"),
            "{query} must not fall to the unknown lane: {err}"
        );
    }
}

/// The bare lane resolves against the LIVE registry passed in, never a literal in `explain.rs`: with the
/// id absent from this build's registry there is nothing to name, so it must fall through to the unknown
/// lane rather than assert an id that does not exist here.
#[test]
fn a_bare_native_tail_needs_its_id_in_the_registry() {
    let packs = fabricated_packs();
    let err = explain_over(
        &packs,
        &["circular".to_string()],
        "unreferenced-model-name",
        Corpus::Bundled,
    )
    .expect_err("no registered schema id means no bare lane");
    assert!(err.contains("unknown rule id"), "got: {err}");
}

/// The lane must not swallow the namespace: a `schema/`-prefixed string that is NOT a registered id still
/// gets the unknown answer, so the prefix alone never manufactures a fake explanation.
#[test]
fn a_schema_namespaced_non_id_still_falls_to_the_unknown_lane() {
    let packs = fabricated_packs();
    let native = vec!["schema-usage".to_string(), "schema/god-model".to_string()];
    let err = explain_over(
        &packs,
        &native,
        "schema/not-an-issue-label",
        Corpus::Bundled,
    )
    .expect_err("an unregistered id must fail");
    assert!(err.contains("unknown rule id"), "got: {err}");
}

/// An EXACT native id wins over the bare-tail lane: `duplicate-route` is registered both bare
/// (`zzop_rules_http`) and as `cross-layer/duplicate-route`, and a reader typing the bare string means the
/// bare id. Without the ordering this would report an ambiguity that does not exist.
#[test]
fn an_exact_native_id_beats_a_namespaced_sibling_with_the_same_tail() {
    let packs = fabricated_packs();
    let native = vec![
        "duplicate-route".to_string(),
        "cross-layer/duplicate-route".to_string(),
    ];
    let err = explain_over(&packs, &native, "duplicate-route", Corpus::Bundled)
        .expect_err("native, not DSL data");
    assert!(
        !err.contains("ambiguous"),
        "an exactly-registered id must not be reported ambiguous: {err}"
    );
    assert!(
        !err.contains("bare form"),
        "the exact lane must answer first: {err}"
    );
}

/// Two namespaced ids sharing a tail and NEITHER registered bare: the bare query is genuinely ambiguous
/// and must say so with both full ids, never pick one.
#[test]
fn a_bare_tail_shared_by_two_namespaces_is_reported_ambiguous() {
    let packs = fabricated_packs();
    let native = vec![
        "cross-layer/route-shadowing".to_string(),
        "schema/route-shadowing".to_string(),
    ];
    let err = explain_over(&packs, &native, "route-shadowing", Corpus::Bundled)
        .expect_err("ambiguous bare tail");
    assert!(err.contains("ambiguous"), "got: {err}");
    assert!(err.contains("cross-layer/route-shadowing"), "got: {err}");
    assert!(err.contains("schema/route-shadowing"), "got: {err}");
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
    let out = explain_over(&packs, &[], "delta/provide-scan", Corpus::Bundled)
        .expect("full id must resolve");
    assert!(out.contains("matcher: io-scan"), "got: {out}");
    assert!(
        out.contains("suppress marker: zzop-provide-scan-ok"),
        "got: {out}"
    );
    assert!(
        out.contains("NATIVE-PARSE RUNS ONLY"),
        "an io-scan marker must not be printed as unconditionally usable: {out}"
    );

    let line_scan =
        explain_over(&fabricated_packs(), &[], "alpha/dup", Corpus::Bundled).expect("must resolve");
    assert!(
        !line_scan.contains("NATIVE-PARSE RUNS ONLY"),
        "a line-scan marker IS read from the scanned file — the condition would be false here: {line_scan}"
    );
}

#[test]
fn a_truly_unknown_id_points_at_the_catalog() {
    let packs = fabricated_packs();
    let err = explain_over(&packs, &[], "no-such-rule-anywhere", Corpus::Bundled)
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
    let err = explain_over(
        &fabricated_packs(),
        &["circular".to_string()],
        "circular",
        Corpus::Bundled,
    )
    .expect_err("a native id carries no DSL rule data");
    assert!(err.contains("native analysis id"), "got: {err}");
    assert!(!err.contains("RECOMMENDATION id"), "got: {err}");
}

/// Seals that the output-id lane is WIRED into the lookup (its own exhaustive coverage lives in
/// `output_ids.rs`): a disclosure class id must not reach the unknown lane even with an empty registry
/// and fabricated packs, because it is answered from the live disclosure registry, not from either.
#[test]
fn an_output_id_is_answered_after_every_rule_lane_misses() {
    let err = explain_over(&fabricated_packs(), &[], "stale-cache", Corpus::Bundled)
        .expect_err("a disclosure class is not a rule");
    assert!(err.contains("coverage-DISCLOSURE class id"), "got: {err}");
    assert!(!err.contains("unknown rule id"), "got: {err}");
}
