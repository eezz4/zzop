//! Zero-config tree building for the config-free "paths mode" shared by `cross_repo` and
//! `check_endpoint` (`paths` argument / trailing CLI paths). Because this helper is shared, any error text it produces must take the calling
//! tool's own name as a parameter rather than hardcoding one sibling's name (a live-fire misfire:
//! `check_endpoint` with a single `paths` entry reported "cross_repo needs at least 2 paths").
//!
//! [`load_trees_request`] is the layer above: the whole "exactly one source (paths XOR config), then
//! build the `analyzeTrees` request" step, shared verbatim by `cross_summary` and `manifest_json` so
//! the two multi-tree entry points cannot drift on source-mode exclusivity or config-method gating.

use crate::paths;

/// The multi-tree request loader: config-first mode (`config_path` — the config's `trees` define the
/// join) or config-free paths mode ([`zero_config_trees`]). Source-mode exclusivity is enforced HERE,
/// not (only) in the hosts — without it a future host passing both would get a silently-narrowed join
/// (config wins, paths ignored), exactly the per-host-drift class this crate exists to close.
///
/// `tool_name` names the CALLER in both error messages, for the same reason `zero_config_trees` takes
/// it (see the module doc's misattribution incident).
pub(crate) fn load_trees_request(
    tool_name: &str,
    paths: &[String],
    config_path: Option<&str>,
) -> Result<zzop_config::LoadedRequest, String> {
    if config_path.is_some() && !paths.is_empty() {
        return Err(format!(
            "{tool_name} takes either `paths` or `configPath`, not both — pass exactly one source"
        ));
    }
    match config_path {
        Some(cp) => {
            // Absolutized like every path argument (see `crate::paths`), so a relative `--config` works
            // from any cwd and the config's own directory resolves absolute for the mapper.
            let loaded = zzop_config::load_config_file(&self::paths::absolutize(cp))
                .map_err(|e| e.to_string())?;
            if loaded.method != zzop_config::Method::AnalyzeTrees {
                return Err(format!(
                    "the config at {} defines a single tree — use analyze_repo for it, or declare `trees` (2+, or \"auto\") for a cross-layer join",
                    loaded
                        .config_path
                        .as_deref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| cp.to_string())
                ));
            }
            Ok(loaded)
        }
        None => zero_config_trees(tool_name, paths),
    }
}

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
