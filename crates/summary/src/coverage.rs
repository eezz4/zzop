//! The `zzop coverage` query core (`coverage_summary`) — the host-layer half of the aggregate
//! visibility surface (H5's landing; the three-value-cell ruling lives in
//! `zzop_facade::query_coverage_json`'s module doc).
//!
//! Exactly the shape `file_summary` established: resolve trees through the shared
//! `zzop_config::trees` front end, run the SAME `analyzeTrees` engine path, hand the output to the
//! pure facade core, then stamp the two host-layer honesty channels on top — `config` and
//! `configWarnings`. The core stays pure and never sees the config front end. Argv split is
//! `facts_json`'s: one path = single-tree mode, 2+ = paths mode (each root loads its own config).

/// Answers "how much of this tree does zzop actually see?" — see
/// [`zzop_facade::query_coverage_json`] for the three-value cell rule and why there is no score.
pub fn coverage_summary(paths: &[String], config_path: Option<&str>) -> Result<String, String> {
    let (path, rest) = match paths {
        [one] => (Some(one.as_str()), &paths[..0]),
        many => (None, many),
    };
    // Surface-neutral operation name — `zzop_config::trees`' WIRE NEUTRALITY note owns that rule.
    let loaded =
        zzop_config::trees::resolve_trees_request("the coverage query", path, rest, config_path)?;
    let out = zzop_facade::analyze_trees_json(&loaded.request.to_string())?;
    let result = zzop_facade::query_coverage_json(&out)?;
    let mut v: serde_json::Value = serde_json::from_str(&result).map_err(|e| e.to_string())?;
    // Same two host-layer channels every sibling stamps, in the same order.
    v["config"] = loaded
        .config_path
        .as_deref()
        .map(|p| serde_json::Value::String(p.display().to_string()))
        .unwrap_or(serde_json::Value::Null);
    let mut warnings = loaded.warnings.clone();
    let engine_side: Vec<String> = serde_json::from_value(serde_json::json!(
        crate::config_warnings::facade_config_warnings(
            &serde_json::from_str::<serde_json::Value>(&out).unwrap_or(serde_json::Value::Null)
        )
    ))
    .unwrap_or_default();
    warnings.extend(engine_side);
    v["configWarnings"] = serde_json::json!(warnings);
    serde_json::to_string_pretty(&v).map_err(|e| e.to_string())
}
