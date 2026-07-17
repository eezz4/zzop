//! `cross_repo`'s cross-layer join summary assembly (`cross_summary`) — split out of `tools.rs`
//! unchanged when this crate's shaping logic moved out of `packages/mcp` (see the crate doc: hosts
//! are thin protocol facades, all shaping logic lives here).

use crate::output::{self, FindingFilters};

/// Cross-repo analysis — zzop's headline. Config-first mode (`config_path`) runs the config's `trees`;
/// paths mode builds zero-config trees tagged by directory name (bundled packs + git defaults still
/// injected) and DISCLOSES any per-tree zzop.config.jsonc it deliberately did not load.
pub fn cross_summary(
    paths: &[String],
    config_path: Option<&str>,
    filters: &FindingFilters,
) -> Result<String, String> {
    let loaded = match config_path {
        Some(cp) => {
            // Absolutized like every path argument (see `crate::paths`), so a relative `--config` works
            // from any cwd and the config's own directory resolves absolute for the mapper.
            let loaded = zzop_config::load_config_file(&crate::paths::absolutize(cp))
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
            loaded
        }
        None => crate::trees::zero_config_trees("cross_repo", paths)?,
    };
    let out = zzop_facade::analyze_trees_json(&loaded.request.to_string())?;
    let v = serde_json::from_str::<serde_json::Value>(&out).map_err(|e| e.to_string())?;

    let empty = Vec::new();
    let trees = v["trees"].as_array().unwrap_or(&empty);
    // Sibling-directory scope disclosure (both modes — the engine echoes each tree's absolute root):
    // when every analyzed root sits under one common parent, that parent's unanalyzed immediate
    // subdirectories are enumerated as a configWarnings entry — the join never silently narrows to
    // "only the trees you happened to pass" (see `crate::siblings`).
    let mut config_warnings = loaded.warnings;
    let roots: Vec<std::path::PathBuf> = trees
        .iter()
        .filter_map(|t| t["root"].as_str().map(std::path::PathBuf::from))
        .collect();
    if let Some(w) = crate::siblings::sibling_scope_warning(&roots) {
        config_warnings.push(w);
    }
    // Config-loader warnings first, then each tree output's facade-level `configWarnings` entries
    // (tree order) — merged into the one config-honesty channel, see `crate::config_warnings`.
    let mut config_warnings: Vec<serde_json::Value> = config_warnings
        .into_iter()
        .map(serde_json::Value::String)
        .collect();
    for t in trees {
        config_warnings.extend(crate::config_warnings::facade_config_warnings(&t["output"]));
    }
    let sources: Vec<serde_json::Value> = trees
        .iter()
        .map(|t| {
            let mut source = serde_json::json!({
                "sourceId": t["sourceId"],
                "path": t["root"],
                "fileCount": t["output"]["fileCount"],
                "findingCount": t["output"]["findings"].as_array().map(Vec::len).unwrap_or(0),
                // Per-tree pack-load confirmation — bounded like analyze_repo's (see there).
                "packsLoaded": t["output"]["packsLoaded"],
                "warnings": t["output"]["warnings"],
                // Per-tree coverage census incl. `joinContributionZero` — see analyze_summary.
                "coverage": t["output"]["coverage"],
            });
            // Per-tree rule-override confirmation — omitted (not null) when absent, same `.get()`
            // guard as analyze_summary (see there for why this diverges from packsLoaded's bare
            // index).
            if let Some(rule_overrides_applied) = t["output"].get("ruleOverridesApplied") {
                source["ruleOverridesApplied"] = rule_overrides_applied.clone();
            }
            source
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
    // `bucket_key_sites` locates the FIRST site (`file:line`) backing each listed key, so e.g. an
    // `unresolvedConsumes` key is no longer a bare string with no call site to go look at.
    let (bucket_keys, bucket_keys_truncated, bucket_key_sites) = output::bucket_keys(cl);

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
        "bucketKeySites": bucket_key_sites,
        "edges": edges_shown,
        "crossLayerFindings": output::shape_findings(&cl_findings, filters),
        "configWarnings": config_warnings,
        // Run-global blindness-class registry — the meta-honesty channel (see analyze_summary).
        "disclosure": v["disclosure"],
    });
    if let Some(truncated) = edges_truncated {
        summary["edgesTruncated"] = truncated;
    }
    if let Some(truncated) = bucket_keys_truncated {
        summary["bucketKeysTruncated"] = truncated;
    }
    // Run-level warnings (distinct from sources[].warnings) — e.g. the parallel-implementation
    // tripwire ("0 cross-source edges but N duplicate/ambiguous findings"). `.get()`-defensive:
    // the field is new on MultiAnalyzeOutputView; forwarded only when present and non-empty so
    // older/edge outputs don't grow a null field.
    if let Some(run_warnings) = v.get("warnings").and_then(|w| w.as_array()) {
        if !run_warnings.is_empty() {
            summary["warnings"] = serde_json::Value::Array(run_warnings.clone());
        }
    }
    serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())
}
