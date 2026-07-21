//! Unit coverage for `analyze_envelope_summary` (Mode A). `analyze_summary`'s tree-mode path already
//! gets end-to-end coverage via `packages/mcp`'s `analyze_repo` e2e tests (it needs a real filesystem
//! root); the envelope path needs no filesystem at all, so its happy/error paths are cheap to pin here
//! directly against the shared shaper both functions call.

use crate::output::{FindingFilters, Verbosity, FULL_ONLY_OUTPUT_FIELDS};

use super::analyze_envelope_summary;

/// `docs/NORMALIZED_AST.md`'s worked example (also embedded as the `example-envelope` MCP contract
/// resource, `packages/mcp/src/embedded.rs`) — a minimal, valid, one-file v1 envelope.
const EXAMPLE_ENVELOPE: &str = include_str!("../../../../examples/jsp-envelope.example.json");

fn no_filters() -> FindingFilters {
    FindingFilters {
        min_severity: None,
        rule: None,
        limit: None,
        verbosity: Default::default(),
    }
}

/// Same filter set as `no_filters`, except `Verbosity::Full` — constructing `FindingFilters` directly
/// (rather than via `FindingFilters::from_args`) is the only way to reach the `Full` lane today: it is
/// STAGED, not yet caller-reachable (see `output::Verbosity`'s doc) — every real tool argument still
/// parses to `Summary`, which is separately pinned by `output::tests`'s `FindingFilters::from_args`
/// coverage.
fn full_filters() -> FindingFilters {
    FindingFilters {
        min_severity: None,
        rule: None,
        limit: None,
        verbosity: Verbosity::Full,
    }
}

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

/// Safety-net pin for the staged `Verbosity::Full` lane (`output::Verbosity`'s doc): nothing previously
/// pinned (a) that today's `Summary` reply never leaks a `FULL_ONLY_OUTPUT_FIELDS` name, (b) that a
/// `Full` reply is an exact superset of the `Summary` reply (never DROPS a `Summary` field on the way to
/// `Full`), or (c) that every name `FULL_ONLY_OUTPUT_FIELDS` declares is actually emitted (not a const
/// naming a field the shaper forgot to insert). The `Full` lane is dead in production today — every real
/// caller builds `FindingFilters` with `Summary` — but it is slated for a later MCP<->CLI parity flip, so
/// this exercises it directly via `full_filters()` against the envelope path (no filesystem needed).
///
/// NOTE: `crates/summary/src/cross.rs`'s per-source `Full` path (`cross_summary`'s `sources[]` shaping,
/// its own `insert_full_output_fields` call site) is NOT covered here. `cross.rs` has no test module and
/// no filesystem-tree fixture helper of its own (unlike this module's envelope fixture, `cross_summary`
/// needs 2+ real directory roots via `crate::trees::zero_config_trees`), so wiring one up is out of scope
/// for this safety-net pin — left deliberately uncovered rather than forced.
#[test]
fn full_verbosity_reply_is_a_superset_of_summary_and_carries_every_full_only_field() {
    fn top_level_keys(json: &str) -> std::collections::BTreeSet<String> {
        serde_json::from_str::<serde_json::Value>(json)
            .expect("valid JSON")
            .as_object()
            .expect("root object")
            .keys()
            .cloned()
            .collect()
    }

    let summary_out = analyze_envelope_summary(EXAMPLE_ENVELOPE, &no_filters())
        .expect("Summary-verbosity envelope analysis should succeed");
    let full_out = analyze_envelope_summary(EXAMPLE_ENVELOPE, &full_filters())
        .expect("Full-verbosity envelope analysis should succeed");

    let summary_keys = top_level_keys(&summary_out);
    let full_keys = top_level_keys(&full_out);

    // (a) Summary_keys ∩ FULL_ONLY_OUTPUT_FIELDS == ∅ — no full-only field leaks into today's reply.
    for field in FULL_ONLY_OUTPUT_FIELDS {
        assert!(
            !summary_keys.contains(*field),
            "Summary reply must not carry the full-only field {field:?}, got keys: {summary_keys:?}"
        );
    }

    // (b) Full_keys ⊇ Summary_keys — Full is a strict superset, never a reshape that drops a field.
    assert!(
        summary_keys.is_subset(&full_keys),
        "Full reply must carry every Summary key too — missing: {:?}",
        summary_keys.difference(&full_keys).collect::<Vec<_>>()
    );

    // (c) every FULL_ONLY_OUTPUT_FIELDS name is actually emitted on the Full reply.
    for field in FULL_ONLY_OUTPUT_FIELDS {
        assert!(
            full_keys.contains(*field),
            "Full reply is missing declared full-only field {field:?}, got keys: {full_keys:?}"
        );
    }
}
