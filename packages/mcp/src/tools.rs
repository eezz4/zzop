//! MCP tool surface: definitions (`tools/list`) and dispatch (`tools/call`), plus the CLI entry points
//! (`zzop-mcp analyze|cross`) that share the same handlers. Handlers are config-driven: `zzop-config`
//! turns `zzop.config.jsonc` (or its absence) into the facade request, `zzop-facade` runs the same
//! engine code path the Node addon uses, and `crate::output` shapes the reply (full counts + capped
//! lists + explicit truncation disclosure). Every reply says which config file was honored (`config`:
//! path or null) and carries the config front-end's own warnings (`configWarnings`) separately from
//! the engine's `warnings` — two different honesty channels, never merged.

use crate::output::{self, FindingFilters};
use std::path::{Path, PathBuf};

/// `tools/list` result: every tool this server exposes, with input JSON Schemas. Shared filter
/// arguments (`severity`/`rule`/`limit`) are the drill-down knobs the truncation hint points at.
pub fn list() -> serde_json::Value {
    let filter_props = serde_json::json!({
        "severity": { "type": "string", "enum": ["critical", "warning", "info"], "description": "Minimum severity to include in the findings list (counts always cover everything)." },
        "rule": { "type": "string", "description": "Exact rule id to include in the findings list." },
        "limit": { "type": "integer", "minimum": 1, "maximum": 1000, "description": "Findings list cap (default 50). Truncation is always disclosed." }
    });
    serde_json::json!({
        "tools": [
            {
                "name": "analyze_repo",
                "description": "Run zzop's deterministic analysis on ONE repository/tree path. Auto-discovers <path>/zzop.config.jsonc (rules, packs, overlays, mounts — the reply's `config` field says whether one was honored); without one, zero-config defaults apply (bundled rule packs + git signals included). Returns a summary (full counts by severity/rule, engine warnings) plus a capped findings list — truncation is always disclosed. A config declaring multiple trees is redirected to cross_repo.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Absolute path to the repo/tree to analyze." },
                        "severity": filter_props["severity"],
                        "rule": filter_props["rule"],
                        "limit": filter_props["limit"]
                    },
                    "required": ["path"]
                }
            },
            {
                "name": "cross_repo",
                "description": "Analyze 2+ repos/trees and join them across the layer boundary — the cross-layer (kind,key) join (e.g. a React consume matching a Spring provide, a shared DB table, route drift). Pass EITHER `configPath` (a zzop.config.jsonc — its `trees`, including \"auto\", define the join; the config-first way) OR `paths` (explicit tree roots; config-free, each tagged by directory name — any zzop.config.jsonc inside them is NOT loaded and says so in configWarnings). Returns per-tree summaries with engine warnings, the join buckets, matched edges, and cross-layer findings (capped lists disclose truncation).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "paths": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Absolute paths to the repos/trees to join (config-free mode).",
                            "minItems": 2
                        },
                        "configPath": { "type": "string", "description": "Path to a zzop.config.jsonc (or a directory containing one) whose trees define the join (config-first mode)." },
                        "severity": filter_props["severity"],
                        "rule": filter_props["rule"],
                        "limit": filter_props["limit"]
                    }
                }
            },
            {
                "name": "validate_envelope",
                "description": "Validate a Normalized AST envelope (a custom parser's output) against the v1 contract WITHOUT running an analysis — the authoring feedback loop. Returns {valid, issues[]}; never fails on bad input. Pair with the zzop://contract/* resources (schema, guide, key-normalization fixture).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "envelopeJson": { "type": "string", "description": "The envelope JSON text to validate." }
                    },
                    "required": ["envelopeJson"]
                }
            }
        ]
    })
}

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
        "validate_envelope" => {
            match args
                .and_then(|a| a.get("envelopeJson"))
                .and_then(|v| v.as_str())
            {
                Some(envelope) => Ok(zzop_facade::validate_envelope_only_json(envelope)),
                None => Err("missing `envelopeJson` argument".into()),
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
    let root = PathBuf::from(path);
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
        "findings": output::shape_findings(&findings, filters),
        "warnings": output_view["warnings"],
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
            let loaded = zzop_config::load_config_file(Path::new(cp)).map_err(|e| e.to_string())?;
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
    let sources: Vec<serde_json::Value> = trees
        .iter()
        .map(|t| {
            serde_json::json!({
                "sourceId": t["sourceId"],
                "path": t["root"],
                "fileCount": t["output"]["fileCount"],
                "findingCount": t["output"]["findings"].as_array().map(Vec::len).unwrap_or(0),
                "warnings": t["output"]["warnings"],
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
        "edges": edges_shown,
        "crossLayerFindings": output::shape_findings(&cl_findings, filters),
        "configWarnings": loaded.warnings,
        // Run-global blindness-class registry — the meta-honesty channel (see analyze_with_filters).
        "disclosure": v["disclosure"],
    });
    if let Some(truncated) = edges_truncated {
        summary["edgesTruncated"] = truncated;
    }
    serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())
}

/// Paths mode: one zero-config tree request per path (an empty `{}` config mapped against that root —
/// bundled `packDefs` + default `git` ride along), `sourceId` = the directory name. A
/// `zzop.config.jsonc` sitting inside a path is deliberately NOT loaded in this mode — silently
/// ignoring it would be worse than saying so, so it lands in the warnings.
fn zero_config_trees(paths: &[String]) -> Result<zzop_config::LoadedRequest, String> {
    if paths.len() < 2 {
        return Err(
            "cross_repo needs at least 2 paths (e.g. the frontend and the backend)".to_string(),
        );
    }
    let mut trees: Vec<serde_json::Value> = Vec::with_capacity(paths.len());
    let mut warnings: Vec<String> = Vec::new();
    for p in paths {
        let root = PathBuf::from(p);
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
