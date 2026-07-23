//! Unit coverage for `analyze_envelope_summary` (Mode A). `analyze_summary`'s tree-mode path already
//! gets end-to-end coverage via `crates/host`'s `analyze_repo` e2e tests (it needs a real filesystem
//! root); the envelope path needs no filesystem at all, so its happy/error paths are cheap to pin here
//! directly against the shared shaper both functions call.

use crate::output::FindingFilters;

use super::analyze_envelope_summary;

/// `docs/NORMALIZED_AST.md`'s worked example (also embedded as the `example-envelope` MCP contract
/// resource, `crates/host/src/embedded.rs`) — a minimal, valid, one-file v1 envelope.
const EXAMPLE_ENVELOPE: &str = include_str!("../../../../examples/jsp-envelope.example.json");

fn no_filters() -> FindingFilters {
    FindingFilters {
        min_severity: None,
        rule: None,
        limit: None,
    }
}

/// The raw `AnalyzeOutputView` fields the shaped summary reply deliberately drops or compacts —
/// available only via a direct `zzop-facade` embedding (see `docs/contracts/surface-parity.json`'s
/// `omit`/`carry-conditional` rows for `ir`/`nodes`/`scores`/`seams`/`folders`/`layerCoChurn`/`cache`/
/// `ruleTimings`/`health`/`recommendations`/`critical`). `health`/`recommendations`/`critical` ride only
/// as the un-compacted originals of the reply's compact `architecture` object.
const RAW_FACADE_ONLY_FIELDS: &[&str] = &[
    "ir",
    "nodes",
    "scores",
    "seams",
    "folders",
    "layerCoChurn",
    "cache",
    "ruleTimings",
    "health",
    "recommendations",
    "critical",
];

#[test]
fn valid_envelope_shapes_to_a_summary_with_findings_and_coverage_keys() {
    let out = analyze_envelope_summary(EXAMPLE_ENVELOPE, &no_filters())
        .expect("a valid Mode A envelope must analyze cleanly");
    let v: serde_json::Value = serde_json::from_str(&out).expect("summary must be valid JSON");
    // Same `AnalyzeOutputView`-shaped keys `analyze_summary` produces (surface-parity: one view type,
    // one registry) — MINUS the filesystem-only `path`/`config` echo, which envelope mode has neither
    // of, and PLUS never an `architecture`/`gitWindow` key (no git signals ran).
    assert!(v.get("findings").is_some(), "{v}");
    assert!(v.get("coverage").is_some(), "{v}");
    assert!(v.get("packsLoaded").is_some(), "{v}");
    assert!(v.get("disclosure").is_some(), "{v}");
    assert!(
        v.get("path").is_none(),
        "envelope mode has no filesystem root to echo: {v}"
    );
    assert!(
        v.get("config").is_none(),
        "envelope mode has no config file to echo: {v}"
    );
    assert!(
        v.get("architecture").is_none(),
        "envelope mode never runs git signals: {v}"
    );
}

#[test]
fn invalid_envelope_reports_a_clear_error() {
    let err = analyze_envelope_summary("{}", &no_filters())
        .expect_err("a structurally invalid envelope must not analyze");
    // Same underlying `zzop_core::validate_envelope` verdict `validate_envelope`'s own error path
    // surfaces — consistent wording across the two tools rather than a second, drifting message.
    assert!(err.contains("envelope"), "{err}");
}

#[test]
fn empty_envelope_json_is_a_named_argument_error() {
    let err = analyze_envelope_summary("   ", &no_filters())
        .expect_err("blank envelopeJson must be rejected before reaching the facade");
    assert!(err.contains("envelopeJson is empty"), "{err}");
}

/// Safety-net pin: nothing else pins that the shaped reply never leaks a raw `RAW_FACADE_ONLY_FIELDS`
/// name. That contract matters on its own — an accidental future forward of e.g. `nodes` or `scores`
/// into the reply would silently blow the token-bomb guard this crate's shaping exists to enforce.
#[test]
fn summary_reply_never_leaks_a_raw_facade_only_field() {
    let summary_out = analyze_envelope_summary(EXAMPLE_ENVELOPE, &no_filters())
        .expect("envelope analysis should succeed");
    let summary_keys: std::collections::BTreeSet<String> =
        serde_json::from_str::<serde_json::Value>(&summary_out)
            .expect("valid JSON")
            .as_object()
            .expect("root object")
            .keys()
            .cloned()
            .collect();

    for field in RAW_FACADE_ONLY_FIELDS {
        assert!(
            !summary_keys.contains(*field),
            "summary reply must not carry the raw facade-only field {field:?}, got keys: {summary_keys:?}"
        );
    }
}
