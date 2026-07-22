//! The `check_endpoint` query core (`endpoint_summary`): a DEFINITIVE answer to "is io key X provided/
//! consumed/joined?" — resolves trees the same way `analyze_summary`/`cross_summary` do (shared
//! `zzop-config` front-end and `crate::trees::zero_config_trees`'s paths mode, never re-implemented),
//! runs the SAME `analyzeTrees` engine path, then hands the output to the shared facade query core
//! (`zzop_facade::query_io_json`) so both surfaces — the `check_endpoint` tool and the
//! `zzop endpoint` CLI subcommand — go through this one function and give identical
//! answers. Every mode routes through `analyzeTrees` — even a single `path` — because the query's
//! sealed verdict vocabulary (linked/provided-only/...) is made of cross-layer JOIN facts, and the
//! join runs fine over one tree (intra-tree edges included); a plain `analyze` output would be
//! rejected by the query core as pre-join.

/// Runs the analysis for the resolved trees and returns the facade query core's JSON with the
/// host-layer honesty channels stamped on top: `config` (which config file was honored, or null)
/// and `configWarnings` (the config front-end's own disclosures — e.g. paths mode's "contains a
/// zzop.config.jsonc that paths mode does NOT load"). The query core stays pure (it never sees the
/// config front-end); the two fields ride the reply exactly like every sibling tool's, so
/// `check_endpoint` cannot silently pretend a dropped config was honored. Pretty-printed for parity
/// with the other tools — query-core keys untouched. Shared by the MCP tool and the
/// `zzop endpoint` CLI subcommand.
pub fn endpoint_summary(
    pattern: &str,
    path: Option<&str>,
    paths: &[String],
    config_path: Option<&str>,
) -> Result<String, String> {
    let loaded = resolve_trees_request(path, paths, config_path)?;
    let out = zzop_facade::analyze_trees_json(&loaded.request.to_string())?;
    let query = serde_json::json!({ "pattern": pattern });
    let result = zzop_facade::query_io_json(&out, &query.to_string())?;
    let mut v: serde_json::Value = serde_json::from_str(&result).map_err(|e| e.to_string())?;
    // The facade's own `suggestions` is substring-driven and comes back empty on a realistic typo
    // (`atricles` for `articles`) even though a near-miss key exists — fall back to a deterministic
    // nearest-key ranking (see `crate::suggest`) ONLY when the substring pass found nothing, so a
    // genuinely nonexistent pattern still gets an empty list rather than a forced guess.
    if v["verdict"] == "not-found" && v["suggestions"].as_array().is_some_and(Vec::is_empty) {
        if let Ok(analysis) = serde_json::from_str::<serde_json::Value>(&out) {
            let fallback = crate::suggest::nearest_keys(&analysis["crossLayer"], pattern);
            if !fallback.is_empty() {
                v["suggestions"] = serde_json::json!(fallback);
            }
        }
    }
    v["config"] = loaded
        .config_path
        .as_deref()
        .map(|p| serde_json::Value::String(p.display().to_string()))
        .unwrap_or(serde_json::Value::Null);
    v["configWarnings"] = serde_json::json!(loaded.warnings);
    serde_json::to_string_pretty(&v).map_err(|e| e.to_string())
}

/// Tree resolution, shared vocabulary with the sibling summary functions:
/// - `path` — one tree, resolved exactly like `analyze_summary` (`zzop_config::load_for_root`:
///   `<path>/zzop.config.jsonc` honored when present, zero-config defaults otherwise). A
///   single-tree request is wrapped into `{trees: [request]}` (see the module doc for why).
/// - `paths` — 2+ config-free tree roots, via `crate::trees::zero_config_trees` (identical to
///   `cross_summary`'s paths mode, disclosure warnings included).
/// - `configPath` — an explicit config file/directory (`zzop_config::load_config_file`); unlike
///   `cross_summary`, a single-tree config is NOT an error here — it wraps like `path` does, since
///   an endpoint query is meaningful over one tree.
fn resolve_trees_request(
    path: Option<&str>,
    paths: &[String],
    config_path: Option<&str>,
) -> Result<zzop_config::LoadedRequest, String> {
    match (path, paths.is_empty(), config_path) {
        (Some(p), true, None) => {
            // Absolutized at the host boundary (see the sibling `crate::paths`) — required by
            // `zzop-config`'s absolute-root contract, and it makes the facade's dir-name sourceId
            // default real for a relative argument (`.` has no `file_name` until absolutized): an
            // unnamed single tree is named after its root's basename at the shared facade
            // chokepoint (`zzop_facade`'s `apply_source_id_default` — formerly a local default
            // here, hoisted so every host and entry point shares one naming rule).
            let root = crate::paths::absolutize(p);
            if !root.exists() {
                return Err(format!("path does not exist: {p}"));
            }
            let loaded = zzop_config::load_for_root(&root).map_err(|e| e.to_string())?;
            Ok(wrap_single_tree(loaded))
        }
        (None, false, None) => crate::trees::zero_config_trees("check_endpoint", paths),
        (None, true, Some(cp)) => {
            // Absolutized like `cross --config` (see there) — a relative configPath works from
            // any cwd. A single-root config with no explicit sourceId is named after the TREE
            // root's basename (not the config file's own directory) by the same facade default —
            // the mapper resolves `root` to absolute against the config's directory before the
            // request reaches the facade.
            let loaded = zzop_config::load_config_file(&crate::paths::absolutize(cp))
                .map_err(|e| e.to_string())?;
            Ok(wrap_single_tree(loaded))
        }
        (None, true, None) => Err(
            "pass `path` (one tree root), `paths` (2+ tree roots), or `configPath` (a zzop.config.jsonc)"
                .to_string(),
        ),
        _ => Err("pass exactly ONE of `path`, `paths`, `configPath`".to_string()),
    }
}

/// A `Method::Analyze` request becomes a one-entry `analyzeTrees` request; an `AnalyzeTrees`
/// request passes through untouched.
fn wrap_single_tree(mut loaded: zzop_config::LoadedRequest) -> zzop_config::LoadedRequest {
    if loaded.method == zzop_config::Method::Analyze {
        loaded.request = serde_json::json!({ "trees": [loaded.request] });
        loaded.method = zzop_config::Method::AnalyzeTrees;
    }
    loaded
}
