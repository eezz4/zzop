//! Unit coverage for `analyze_envelope_summary` (Mode A). `analyze_summary`'s tree-mode path already
//! gets end-to-end coverage via this crate's own `tests/host_dispatch.rs` (it needs a real filesystem
//! root); the envelope path needs no filesystem at all, so its happy/error paths are cheap to pin here
//! directly against the shared shaper both functions call.

use crate::output::FindingFilters;

use super::analyze_envelope_summary;

/// `docs/NORMALIZED_AST.md`'s worked example (also embedded as the `example-envelope` MCP contract
/// resource, `crate::contracts`) — a minimal, valid, one-file v1 envelope.
const EXAMPLE_ENVELOPE: &str = include_str!("../../../../docs/contracts/example-envelope.json");

fn no_filters() -> FindingFilters {
    FindingFilters {
        min_severity: None,
        rule: None,
        limit: None,
    }
}

/// The SHIPPED surface-parity registry — the same document
/// `crates/engine/tests/rule_contracts/surface_parity.rs` reads, and the reason the two lists below are
/// derivations rather than literals. It carries one row per `AnalyzeOutputView` top-level field
/// (completeness enforced there, against the facade's own pinned key set), so reading it here means a
/// field the facade GROWS reaches this file's subject set on the same commit that adds it.
fn analyze_output_view_rows() -> serde_json::Map<String, serde_json::Value> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/contracts/surface-parity.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let mut rows = serde_json::from_str::<serde_json::Value>(&text)
        .expect("the surface-parity registry must be valid JSON")["analyzeOutputView"]
        .as_object()
        .expect("the registry must carry an analyzeOutputView block")
        .clone();
    rows.retain(|k, _| !k.starts_with('_'));
    rows
}

/// The registry rows whose field is forwarded to the reply UNDER ITS OWN NAME (shaped, capped or
/// conditional, but spelled the same). Every OTHER `carry-conditional` row rides under a different key
/// — `health`/`recommendations`/`critical` exist in the reply only as the compact `architecture`
/// object's ingredients — so the derivation below treats them exactly like an `omit` row: their RAW
/// form must never appear at the top level.
///
/// A whitelist, deliberately: a new `carry-conditional` field fails closed (it lands in the denylist
/// and this file goes red) until someone states which of the two lanes it took.
const CONDITIONAL_UNDER_ITS_OWN_NAME: &[&str] =
    &["degraded", "findings", "gitWindow", "ruleOverridesApplied"];

/// The reply keys the SHAPER invents — the ones that are not `AnalyzeOutputView` fields at all, so the
/// registry has no row for them: the tree-mode `path`/`config` echo, the `degradedTruncated`
/// disclosure, and the compact `architecture` object. Also a whitelist, for the same fail-closed
/// reason: a NEW reply key that nobody declared is exactly the growth this pin exists to notice.
const SHAPER_INVENTED_KEYS: &[&str] = &["path", "config", "degradedTruncated", "architecture"];

#[test]
fn valid_envelope_shapes_to_a_summary_with_findings_and_coverage_keys() {
    let out = analyze_envelope_summary(EXAMPLE_ENVELOPE, &no_filters())
        .expect("a valid Mode A envelope must analyze cleanly");
    let v: serde_json::Value = serde_json::from_str(&out).expect("summary must be valid JSON");
    // Same `AnalyzeOutputView`-shaped keys `analyze_summary` produces (surface-parity: one view type,
    // one registry) — MINUS the filesystem-only `path`/`config` echo, which envelope mode has neither
    // of, and MINUS `architecture` (no git signals ran). `gitWindow` is NOT in that minus list and is
    // asserted present-and-null below: the facade always serializes it, and `null` on the wire IS the
    // "git did not run" signal. This comment used to claim `gitWindow` was absent too while the
    // assertions covered only `architecture` — a false claim in a pin's own prose, which is how the
    // `analyze_envelope` tool description shipped the same falsehood unchallenged.
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
    assert!(
        v.get("gitWindow").is_some_and(serde_json::Value::is_null),
        "gitWindow is ALWAYS serialized and is null when git did not run — absence would break the \
         absent-vs-null distinction `architecture` above relies on: {v}"
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
        .expect_err("a blank envelope document must be rejected before reaching the facade");
    assert!(err.contains("the envelope document is empty"), "{err}");
    // Spelling-free: `envelopeJson` is the MCP argument name and the CLI twin passes a FILE, so naming
    // the wire argument told half the callers about a knob they do not have (see
    // `crates/engine/tests/rule_contracts/host_vocabulary.rs`, the machine pin on this class).
    assert!(!err.contains("envelopeJson"), "{err}");
}

/// The shaped reply's own top-level key set — the RUNTIME artifact both pins below judge (the sibling
/// meta-test in `crates/engine/tests/rule_contracts/surface_parity.rs` judges the lane's SOURCE TEXT,
/// which is a different question: whether a key is spelled anywhere, not whether it shipped).
fn summary_reply_keys() -> std::collections::BTreeSet<String> {
    let out = analyze_envelope_summary(EXAMPLE_ENVELOPE, &no_filters())
        .expect("envelope analysis should succeed");
    serde_json::from_str::<serde_json::Value>(&out)
        .expect("valid JSON")
        .as_object()
        .expect("root object")
        .keys()
        .cloned()
        .collect()
}

/// Safety-net pin: the shaped reply never leaks a raw facade-only field. That contract matters on its
/// own — an accidental future forward of e.g. `nodes` or `scores` into the reply would silently blow
/// the token-bomb guard this crate's shaping exists to enforce.
///
/// The subject set is DERIVED from the shipped registry (every `omit` row, plus every
/// `carry-conditional` row that does not ride under its own name), not listed here. It used to be a
/// literal 11-name denylist, which meant the pin could only ever check the facade of the day it was
/// written: a facade that grows a twelfth droppable field was invisible to it.
#[test]
fn summary_reply_never_leaks_a_raw_facade_only_field() {
    let rows = analyze_output_view_rows();
    let facade_only: Vec<String> = rows
        .iter()
        .filter(|(name, row)| match row["mcpAnalyzeReply"].as_str() {
            Some("omit") => true,
            Some("carry-conditional") => !CONDITIONAL_UNDER_ITS_OWN_NAME.contains(&name.as_str()),
            _ => false,
        })
        .map(|(name, _)| name.clone())
        .collect();
    assert!(
        facade_only.len() >= 5,
        "the registry yielded {facade_only:?} as the facade-only set — too few to be the real one, so \
         this pin would vouch for nothing (has `analyzeOutputView`/`mcpAnalyzeReply` been renamed?)"
    );
    let summary_keys = summary_reply_keys();
    for field in &facade_only {
        assert!(
            !summary_keys.contains(field),
            "summary reply must not carry the raw facade-only field {field:?} (surface-parity says it \
             is dropped or rides compacted), got keys: {summary_keys:?}"
        );
    }
}

/// The other direction, and the one a denylist structurally cannot answer: every key the reply DOES
/// carry must be an `AnalyzeOutputView` field the registry knows about, or one of the shaper's own
/// declared inventions. A denylist only ever sees the fields someone thought to name; this sees the
/// reply GROWING — a new key added to the shaper with no row and no declaration is red here, which is
/// precisely the plant (`internalDebugState`) the old literal list waved through.
#[test]
fn every_summary_reply_key_is_a_registry_field_or_a_declared_shaper_invention() {
    let rows = analyze_output_view_rows();
    assert!(
        rows.len() >= 10,
        "the registry yielded {} analyzeOutputView row(s) — it has stopped parsing, so this pin would \
         vouch for nothing",
        rows.len()
    );
    let summary_keys = summary_reply_keys();
    assert!(
        !summary_keys.is_empty(),
        "the shaped reply has no keys at all — an empty subject set must be RED, never a silent pass"
    );
    let undeclared: Vec<&String> = summary_keys
        .iter()
        .filter(|k| !rows.contains_key(k.as_str()) && !SHAPER_INVENTED_KEYS.contains(&k.as_str()))
        .collect();
    assert!(
        undeclared.is_empty(),
        "these keys ship in the shaped analyze reply but are neither an AnalyzeOutputView field \
         (docs/contracts/surface-parity.json) nor a declared shaper invention: {undeclared:?} — add \
         the registry row, or declare it in SHAPER_INVENTED_KEYS with a reason"
    );
}
