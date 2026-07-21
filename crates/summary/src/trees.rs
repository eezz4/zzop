//! Zero-config tree building for the config-free "paths mode" shared by `cross_repo` and
//! `check_endpoint` (`paths` argument / trailing CLI paths). Because this helper is shared, any error text it produces must take the calling
//! tool's own name as a parameter rather than hardcoding one sibling's name (a live-fire misfire:
//! `check_endpoint` with a single `paths` entry reported "cross_repo needs at least 2 paths").

use crate::paths;

/// Paths mode: one zero-config tree request per path (an empty `{}` config mapped against that root —
/// bundled `packDefs` + default `git` ride along), `sourceId` = the directory name. A
/// `zzop.config.jsonc` sitting inside a path is deliberately NOT loaded in this mode — silently
/// ignoring it would be worse than saying so, so it lands in the warnings.
///
/// `tool_name` is the CALLER's own MCP tool name (`cross_repo`, `check_endpoint`, ...) — this helper
/// is shared, so the "at least 2 paths" error must name whichever tool the caller actually is, never
/// a hardcoded sibling (see the module doc for the live-fire misattribution this parameter fixes).
pub(crate) fn zero_config_trees(
    tool_name: &str,
    paths: &[String],
) -> Result<zzop_config::LoadedRequest, String> {
    if paths.len() < 2 {
        return Err(format!(
            "{tool_name} needs at least 2 paths (e.g. the frontend and the backend)"
        ));
    }
    let mut trees: Vec<serde_json::Value> = Vec::with_capacity(paths.len());
    let mut warnings: Vec<String> = Vec::new();
    for p in paths {
        // Absolutized at the host boundary (see `paths`) — this also makes the dir-name `sourceId`
        // below real for a relative argument (`.` has no `file_name` until it is absolutized).
        let root = paths::absolutize(p);
        if !root.exists() {
            return Err(format!("path does not exist: {p}"));
        }
        let mapped = zzop_config::mapper::config_to_request(&serde_json::json!({}), &root)
            .map_err(|e| e.to_string())?;
        // The mapper's own warnings must survive this mode too (e.g. a bundled pack that failed to
        // parse) — dropping them here would make paths mode the one silent sibling.
        warnings.extend(mapped.warnings);
        let mut req = mapped.request;
        let source_id = root
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(p.as_str())
            .to_string();
        req["sourceId"] = serde_json::Value::String(source_id);
        if root.join(zzop_config::DEFAULT_CONFIG_FILENAME).is_file() {
            warnings.push(format!(
                "{p} contains a {} that paths mode does NOT load — pass configPath to honor it",
                zzop_config::DEFAULT_CONFIG_FILENAME
            ));
        }
        trees.push(req);
    }
    Ok(zzop_config::LoadedRequest {
        method: zzop_config::Method::AnalyzeTrees,
        request: serde_json::json!({ "trees": trees }),
        warnings,
        config_path: None,
    })
}
