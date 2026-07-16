//! MCP tool surface: definitions (`tools/list`) and dispatch (`tools/call`), plus the CLI entry points
//! (`zzop-mcp analyze|cross|endpoint`) that share the same handlers. Handlers are config-driven: `zzop-config`
//! turns `zzop.config.jsonc` (or its absence) into the facade request, `zzop-facade` runs the same
//! engine code path the Node addon uses, and `crate::output` shapes the reply (full counts + capped
//! lists + explicit truncation disclosure). Every tree-resolving reply (`analyze_repo`/`cross_repo`/
//! `check_endpoint` — the two validators take no config, so they carry neither field) says which
//! config file was honored (`config`: path or null) and carries the config front-end's own warnings
//! (`configWarnings`) separately from the engine's `warnings` — two different honesty channels,
//! never merged.

mod definitions;
mod endpoint;
mod paths;
mod siblings;
#[cfg(test)]
mod tests;
mod trees;

use crate::output::{self, FindingFilters};
use trees::zero_config_trees;

pub use definitions::list;
pub use endpoint::check_endpoint;

/// `tools/call` dispatch. Tool-level failures return a normal MCP result with `isError: true` (the MCP
/// convention — protocol errors are only for malformed JSON-RPC, which `server` handles before us).
pub fn call(params: Option<&serde_json::Value>) -> serde_json::Value {
    let name = params
        .and_then(|p| p.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let args = params.and_then(|p| p.get("arguments"));
    let outcome = match name {
        "analyze_repo" => match args.and_then(|a| a.get("path")).and_then(|v| v.as_str()) {
            Some(path) => FindingFilters::from_args(args)
                .and_then(|filters| analyze_with_filters(path, &filters)),
            None => Err("missing `path` argument".into()),
        },
        "cross_repo" => {
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
            match (paths.is_empty(), config_path) {
                (false, Some(_)) => {
                    Err("pass either `paths` or `configPath`, not both".to_string())
                }
                (true, None) => Err(
                    "pass `paths` (2+ tree roots) or `configPath` (a zzop.config.jsonc whose trees define the join)"
                        .to_string(),
                ),
                _ => FindingFilters::from_args(args)
                    .and_then(|filters| cross_repo_with_filters(&paths, config_path, &filters)),
            }
        }
        "check_endpoint" => endpoint::call_from_args(args),
        "validate_envelope" => {
            match args
                .and_then(|a| a.get("envelopeJson"))
                .and_then(|v| v.as_str())
            {
                Some(envelope) => Ok(zzop_facade::validate_envelope_only_json(envelope)),
                None => Err("missing `envelopeJson` argument".into()),
            }
        }
        "validate_rule_pack" => {
            match args
                .and_then(|a| a.get("packJson"))
                .and_then(|v| v.as_str())
            {
                Some(pack) => Ok(zzop_facade::validate_rule_pack_json(pack)),
                None => Err("missing `packJson` argument".into()),
            }
        }
        other => Err(format!("unknown tool: {other}")),
    };
    match outcome {
        Ok(text) => serde_json::json!({ "content": [{ "type": "text", "text": text }] }),
        Err(e) => serde_json::json!({
            "content": [{ "type": "text", "text": format!("zzop error: {e}") }],
            "isError": true
        }),
    }
}

/// CLI `zzop-mcp analyze <path>` — default filters.
pub fn analyze(path: &str) -> Result<String, String> {
    analyze_with_filters(path, &default_filters())
}

/// CLI `zzop-mcp cross <path>...` / `zzop-mcp cross --config <path>` — default filters.
pub fn cross_repo(paths: &[String], config_path: Option<&str>) -> Result<String, String> {
    cross_repo_with_filters(paths, config_path, &default_filters())
}

fn default_filters() -> FindingFilters {
    FindingFilters {
        min_severity: None,
        rule: None,
        limit: None,
    }
}

/// Analyze ONE tree: config auto-discovery via `zzop-config`, execution via `zzop-facade` (the same
/// engine code path as the Node addon), summary-first shaping. A config declaring multiple trees is a
/// guided error — that analysis is `cross_repo`'s job.
fn analyze_with_filters(path: &str, filters: &FindingFilters) -> Result<String, String> {
    // Absolutized at the host boundary (see `paths`): `zzop-config` requires an absolute root.
    let root = paths::absolutize(path);
    if !root.exists() {
        return Err(format!("path does not exist: {path}"));
    }
    let loaded = zzop_config::load_for_root(&root).map_err(|e| e.to_string())?;
    // `disclosure` is the facade's run-global blindness-class registry (which failure classes zzop
    // does/does NOT detect) — the meta-honesty channel an AI consumer needs alongside the active
    // `warnings`; it rides at the top level of every facade output and is forwarded, never dropped.
    let (output_view, disclosure) = match loaded.method {
        zzop_config::Method::Analyze => {
            let out = zzop_facade::analyze_json(&loaded.request.to_string())?;
            let v = serde_json::from_str::<serde_json::Value>(&out).map_err(|e| e.to_string())?;
            let disclosure = v["disclosure"].clone();
            (v, disclosure)
        }
        zzop_config::Method::AnalyzeTrees => {
            let tree_count = loaded.request["trees"]
                .as_array()
                .map(Vec::len)
                .unwrap_or(0);
            if tree_count > 1 {
                return Err(format!(
                    "the config at {} defines {tree_count} trees — use the cross_repo tool with configPath to run the cross-layer join, or point analyze_repo at one tree root directly",
                    loaded
                        .config_path
                        .as_deref()
                        .unwrap_or(&root)
                        .display()
                ));
            }
            let out = zzop_facade::analyze_trees_json(&loaded.request.to_string())?;
            let v = serde_json::from_str::<serde_json::Value>(&out).map_err(|e| e.to_string())?;
            (v["trees"][0]["output"].clone(), v["disclosure"].clone())
        }
    };
    let findings = output_view["findings"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let summary = serde_json::json!({
        "path": path,
        "config": loaded.config_path.as_deref().map(|p| p.display().to_string()),
        "fileCount": output_view["fileCount"],
        "degraded": output_view["degraded"],
        // Positive pack-load confirmation ({id, rules, source}[], id-sorted, small and bounded — one
        // entry per loaded pack, never per finding) — forwarded whole, no cap needed.
        "packsLoaded": output_view["packsLoaded"],
        "findings": output::shape_findings(&findings, filters),
        "warnings": output_view["warnings"],
        // Per-tree structural coverage census, forwarded whole (a handful of scalars) — carries the
        // `joinContributionZero` blindness ASSERTION; a summary that drops the engine's own "this
        // tree contributed nothing to the join" fact is not a disclosure.
        "coverage": output_view["coverage"],
        "configWarnings": loaded.warnings,
        "disclosure": disclosure,
    });
    serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())
}

/// Cross-repo analysis — zzop's headline. Config-first mode (`config_path`) runs the config's `trees`;
/// paths mode builds zero-config trees tagged by directory name (bundled packs + git defaults still
/// injected) and DISCLOSES any per-tree zzop.config.jsonc it deliberately did not load.
fn cross_repo_with_filters(
    paths: &[String],
    config_path: Option<&str>,
    filters: &FindingFilters,
) -> Result<String, String> {
    let loaded = match config_path {
        Some(cp) => {
            // Absolutized like every path argument (see `paths`), so a relative `--config` works
            // from any cwd and the config's own directory resolves absolute for the mapper.
            let loaded =
                zzop_config::load_config_file(&paths::absolutize(cp)).map_err(|e| e.to_string())?;
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
            loaded
        }
        None => zero_config_trees(paths)?,
    };
    let out = zzop_facade::analyze_trees_json(&loaded.request.to_string())?;
    let v = serde_json::from_str::<serde_json::Value>(&out).map_err(|e| e.to_string())?;

    let empty = Vec::new();
    let trees = v["trees"].as_array().unwrap_or(&empty);
    // Sibling-directory scope disclosure (both modes — the engine echoes each tree's absolute root):
    // when every analyzed root sits under one common parent, that parent's unanalyzed immediate
    // subdirectories are enumerated as a configWarnings entry — the join never silently narrows to
    // "only the trees you happened to pass" (see `siblings`).
    let mut config_warnings = loaded.warnings;
    let roots: Vec<std::path::PathBuf> = trees
        .iter()
        .filter_map(|t| t["root"].as_str().map(std::path::PathBuf::from))
        .collect();
    if let Some(w) = siblings::sibling_scope_warning(&roots) {
        config_warnings.push(w);
    }
    let sources: Vec<serde_json::Value> = trees
        .iter()
        .map(|t| {
            serde_json::json!({
                "sourceId": t["sourceId"],
                "path": t["root"],
                "fileCount": t["output"]["fileCount"],
                "findingCount": t["output"]["findings"].as_array().map(Vec::len).unwrap_or(0),
                // Per-tree pack-load confirmation — bounded like analyze_repo's (see there).
                "packsLoaded": t["output"]["packsLoaded"],
                "warnings": t["output"]["warnings"],
                // Per-tree coverage census incl. `joinContributionZero` — see analyze_with_filters.
                "coverage": t["output"]["coverage"],
            })
        })
        .collect();
    let cl = &v["crossLayer"];
    let bucket_len = |key: &str| cl[key].as_array().map(Vec::len).unwrap_or(0);
    let edges = cl["edges"].as_array().cloned().unwrap_or_default();
    let (edges_shown, edges_truncated) = output::shape_list(&edges, output::DEFAULT_EDGES_LIMIT);
    let cl_findings = v["crossLayerFindings"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    // WHICH keys sit in each non-edge bucket, not just how many — capped per bucket with the
    // remainder disclosed (`bucketKeysTruncated`), same never-silent stance as `edgesTruncated`.
    let (bucket_keys, bucket_keys_truncated) = output::bucket_keys(cl);

    let mut summary = serde_json::json!({
        "config": loaded.config_path.as_deref().map(|p| p.display().to_string()),
        "sources": sources,
        "buckets": {
            "edges": edges.len(),
            "unconsumedProvides": bucket_len("unconsumedProvides"),
            "unprovidedConsumes": bucket_len("unprovidedConsumes"),
            "unresolvedConsumes": bucket_len("unresolvedConsumes"),
            "externalConsumes": bucket_len("externalConsumes"),
            "ambiguousConsumes": bucket_len("ambiguousConsumes"),
        },
        "bucketKeys": bucket_keys,
        "edges": edges_shown,
        "crossLayerFindings": output::shape_findings(&cl_findings, filters),
        "configWarnings": config_warnings,
        // Run-global blindness-class registry — the meta-honesty channel (see analyze_with_filters).
        "disclosure": v["disclosure"],
    });
    if let Some(truncated) = edges_truncated {
        summary["edgesTruncated"] = truncated;
    }
    if let Some(truncated) = bucket_keys_truncated {
        summary["bucketKeysTruncated"] = truncated;
    }
    serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())
}
