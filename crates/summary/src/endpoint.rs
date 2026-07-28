//! The `check_endpoint` query core (`endpoint_summary`): a DEFINITIVE answer to "is io key X provided/
//! consumed/joined?" — resolves trees the same way `analyze_summary`/`cross_summary` do (shared
//! `zzop-config` front-end and `zzop_config::trees`'s paths mode, never re-implemented),
//! runs the SAME `analyzeTrees` engine path, then hands the output to the shared facade query core
//! (`zzop_facade::query_io_json`) so both surfaces — the `check_endpoint` tool and the
//! `zzop endpoint` CLI subcommand — go through this one function and give identical
//! answers. Every mode routes through `analyzeTrees` — even a single `path` — because the query's
//! sealed verdict vocabulary (linked/provided-only/...) is made of cross-layer JOIN facts, and the
//! join runs fine over one tree (intra-tree edges included); a plain `analyze` output would be
//! rejected by the query core as pre-join.
//!
//! The three-mode tree resolution itself lives in `zzop_config::trees::resolve_trees_request` — hoisted out
//! of this module when `facts_json` grew the identical `path`/`paths`/`configPath` contract, so the
//! two single-tree-tolerant entry points cannot drift on which config methods they accept.

/// Runs the analysis for the resolved trees and returns the facade query core's JSON with the
/// host-layer honesty channels stamped on top: `config` (which config file was honored, or null)
/// and `configWarnings` (the config front-end's own disclosures — e.g. paths mode's "loaded each tree's
/// own zzop.config.jsonc", which is why `config` reads null there without meaning "no config was
/// read"). The query core stays pure (it never sees the
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
    let loaded =
        // Surface-neutral operation name, never one host's tool spelling — see `zzop_config::trees`'s
        // WIRE NEUTRALITY note for the vocabulary leak this closes.
        zzop_config::trees::resolve_trees_request("the endpoint query", path, paths, config_path)?;
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
            let (fallback, total) = crate::suggest::nearest_keys(&analysis["crossLayer"], pattern);
            if !fallback.is_empty() {
                // Same disclosure the facade's own substring pass makes, and under the same key: this
                // list REPLACES the facade's (empty) one, so its cap has to replace the facade's
                // (absent) truncation count too, or the reply ships a silently shortened list.
                if total > fallback.len() {
                    v["suggestionsTruncated"] = serde_json::json!(total - fallback.len());
                }
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
