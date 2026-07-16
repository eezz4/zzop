//! The `check_endpoint` tool: a DEFINITIVE answer to "is io key X provided/consumed/joined?" —
//! resolves trees the same way `analyze_repo`/`cross_repo` do (shared `zzop-config` front-end and
//! the parent module's `zero_config_trees` paths mode, never re-implemented), runs the SAME
//! `analyzeTrees` engine path, then hands the output to the shared facade query core
//! (`zzop_facade::query_io_json`) so this tool and the JS CLI's `zzop endpoint` give identical
//! answers. Every mode routes through `analyzeTrees` — even a single `path` — because the query's
//! sealed verdict vocabulary (linked/provided-only/...) is made of cross-layer JOIN facts, and the
//! join runs fine over one tree (intra-tree edges included); a plain `analyze` output would be
//! rejected by the query core as pre-join.

use std::path::{Path, PathBuf};

/// `tools/call` argument extraction for `check_endpoint`: `pattern` required, then exactly one of
/// `path` (one tree) / `paths` (2+ tree roots) / `configPath`.
pub fn call_from_args(args: Option<&serde_json::Value>) -> Result<String, String> {
    let Some(pattern) = args.and_then(|a| a.get("pattern")).and_then(|v| v.as_str()) else {
        return Err("missing `pattern` argument".into());
    };
    let path = args.and_then(|a| a.get("path")).and_then(|v| v.as_str());
    let paths: Vec<String> = args
        .and_then(|a| a.get("paths"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let config_path = args
        .and_then(|a| a.get("configPath"))
        .and_then(|v| v.as_str());
    check_endpoint(pattern, path, &paths, config_path)
}

/// Runs the analysis for the resolved trees and returns the facade query core's JSON with the
/// host-layer honesty channels stamped on top: `config` (which config file was honored, or null)
/// and `configWarnings` (the config front-end's own disclosures — e.g. paths mode's "contains a
/// zzop.config.jsonc that paths mode does NOT load"). The query core stays pure (it never sees the
/// config front-end); the two fields ride the reply exactly like every sibling tool's, so
/// `check_endpoint` cannot silently pretend a dropped config was honored. Pretty-printed for parity
/// with the other tools — query-core keys untouched. Shared by the MCP tool and the
/// `zzop-mcp endpoint` CLI subcommand.
pub fn check_endpoint(
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
    v["config"] = loaded
        .config_path
        .as_deref()
        .map(|p| serde_json::Value::String(p.display().to_string()))
        .unwrap_or(serde_json::Value::Null);
    v["configWarnings"] = serde_json::json!(loaded.warnings);
    serde_json::to_string_pretty(&v).map_err(|e| e.to_string())
}

/// Tree resolution, shared vocabulary with the sibling tools:
/// - `path` — one tree, resolved exactly like `analyze_repo` (`zzop_config::load_for_root`:
///   `<path>/zzop.config.jsonc` honored when present, zero-config defaults otherwise). A
///   single-tree request is wrapped into `{trees: [request]}` (see the module doc for why).
/// - `paths` — 2+ config-free tree roots, via the parent module's `zero_config_trees` (identical
///   to `cross_repo`'s paths mode, disclosure warnings included).
/// - `configPath` — an explicit config file/directory (`zzop_config::load_config_file`); unlike
///   `cross_repo`, a single-tree config is NOT an error here — it wraps like `path` does, since an
///   endpoint query is meaningful over one tree.
fn resolve_trees_request(
    path: Option<&str>,
    paths: &[String],
    config_path: Option<&str>,
) -> Result<zzop_config::LoadedRequest, String> {
    match (path, paths.is_empty(), config_path) {
        (Some(p), true, None) => {
            // Absolutized at the host boundary (see the sibling `paths` module) — required by
            // `zzop-config`'s absolute-root contract, and it makes the dir-name sourceId default
            // below real for a relative argument (`.` has no `file_name` until absolutized).
            let root = super::paths::absolutize(p);
            if !root.exists() {
                return Err(format!("path does not exist: {p}"));
            }
            let mut loaded = zzop_config::load_for_root(&root).map_err(|e| e.to_string())?;
            default_source_id_to_dir_name(&mut loaded, &root, p);
            Ok(wrap_single_tree(loaded))
        }
        (None, false, None) => super::zero_config_trees(paths),
        (None, true, Some(cp)) => {
            // Absolutized like `cross --config` (see there) — a relative configPath works from
            // any cwd.
            let mut loaded = zzop_config::load_config_file(&super::paths::absolutize(cp))
                .map_err(|e| e.to_string())?;
            // Same unnamed-source fix as `path` mode: a single-root config with no explicit
            // sourceId would tag every match `source: ""` — name it after the TREE root the
            // config resolved (not the config file's own directory).
            if let Some(root) = loaded
                .request
                .get("root")
                .and_then(|v| v.as_str())
                .map(PathBuf::from)
            {
                let given = root.display().to_string();
                default_source_id_to_dir_name(&mut loaded, &root, &given);
            }
            Ok(wrap_single_tree(loaded))
        }
        (None, true, None) => Err(
            "pass `path` (one tree root), `paths` (2+ tree roots), or `configPath` (a zzop.config.jsonc)"
                .to_string(),
        ),
        _ => Err("pass exactly ONE of `path`, `paths`, `configPath`".to_string()),
    }
}

/// Single-`path` mode's sourceId default: with no config-provided `sourceId`, the one tree's
/// matches would carry `source: ""` (an unnamed tree) through every query bucket — so the tree is
/// named after its directory, mirroring `zero_config_trees`' naming exactly. A config-provided
/// `sourceId` is never overridden, and a config declaring `trees` (`Method::AnalyzeTrees`) is left
/// untouched — its trees name themselves.
fn default_source_id_to_dir_name(
    loaded: &mut zzop_config::LoadedRequest,
    root: &Path,
    given: &str,
) {
    if loaded.method != zzop_config::Method::Analyze {
        return;
    }
    let has_source_id = loaded
        .request
        .get("sourceId")
        .and_then(|v| v.as_str())
        .is_some_and(|s| !s.is_empty());
    if has_source_id {
        return;
    }
    let source_id = root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(given)
        .to_string();
    loaded.request["sourceId"] = serde_json::Value::String(source_id);
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
