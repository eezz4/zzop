//! The `check_file` query core (`file_summary`) — the host-layer half of D16's targeting surface.
//!
//! Exactly the shape `endpoint_summary` established, and deliberately so: resolve trees through the
//! shared `zzop_config::trees` front end (never re-implemented here), run the SAME `analyzeTrees` engine
//! path, hand the output to the pure facade core (`zzop_facade::query_file_json`), then stamp the two
//! host-layer honesty channels on top — `config` (which config file was honored, or null) and
//! `configWarnings`. The core stays pure and never sees the config front end, so the MCP tool and the
//! `zzop file` CLI subcommand cannot drift.
//!
//! `analyzeTrees` even for a single `path`, for the same reason `endpoint_summary` does it: the reply
//! names the TREE a file was found in, and a single-tree `analyze` output has no tree identity at all.
//! The join runs fine over one tree, and a cross-layer finding anchored in the target file is part of
//! what "everything about this file" means.

/// Answers "what does zzop know about THIS FILE?" — see [`zzop_facade::query_file_json`] for the sealed
/// verdict vocabulary and why the verdict is about ANALYSIS STATE rather than health.
pub fn file_summary(
    target: &str,
    source_id: Option<&str>,
    path: Option<&str>,
    paths: &[String],
    config_path: Option<&str>,
) -> Result<String, String> {
    // Surface-neutral operation name, never one host's tool spelling — `zzop_config::trees`' WIRE
    // NEUTRALITY note owns that rule.
    let loaded =
        zzop_config::trees::resolve_trees_request("the file query", path, paths, config_path)?;
    let out = zzop_facade::analyze_trees_json(&loaded.request.to_string())?;
    let mut query = serde_json::json!({ "path": target });
    if let Some(sid) = source_id {
        query["sourceId"] = serde_json::json!(sid);
    }
    let result = zzop_facade::query_file_json(&out, &query.to_string())?;
    let mut v: serde_json::Value = serde_json::from_str(&result).map_err(|e| e.to_string())?;
    // Same two host-layer channels every sibling tool stamps, in the same order — the loader's own
    // warnings first, then the engine-side config diagnostics, because they are the same kind of honesty.
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
