//! MCP tool surface: definitions (`tools/list`) and dispatch (`tools/call`). This module is pure
//! protocol dispatch: extract the tool's arguments (`zzop_summary::args`), call the matching
//! `zzop_summary` function (config auto-discovery + facade call + summary assembly all live there —
//! see its crate doc), and wrap the result into the MCP reply shape. No shaping/filtering/warning-merge
//! logic lives here — if it did, it would be exactly the per-host drift the `zzop-summary` split exists
//! to prevent. The `zzop` CLI's twin subcommands (`analyze`/`cross`/`endpoint`/…) call the same
//! `zzop_summary` functions directly from `packages/cli-bin/src/main.rs` — there is no shared
//! per-product dispatch layer between the two, because a wrapper only one product traversed was drift
//! surface rather than a guard against it (2026-07-26 `crates/host` teardown).

mod definitions;
#[cfg(test)]
mod tests;

use zzop_summary::args;
use zzop_summary::FindingFilters;

pub use definitions::list;

/// `tools/call` dispatch. Tool-level failures return a normal MCP result with `isError: true` (the MCP
/// convention — protocol errors are only for malformed JSON-RPC, which `server` handles before us).
pub fn call(params: Option<&serde_json::Value>) -> serde_json::Value {
    let name = params
        .and_then(|p| p.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let args = params.and_then(|p| p.get("arguments"));
    let outcome = match name {
        "analyze_repo" => (|| {
            // `path` XOR `configPath` — the same two source modes the `zzop analyze` CLI twin takes.
            // Both are OPTIONAL here and the shared handler decides: it owns "exactly one source", so
            // the two hosts cannot drift on which combinations are legal (a `required_string` here
            // would have made "neither" this layer's error and "both" the handler's).
            let path = args::optional_string(args, "path")?;
            let config_path = args::optional_string(args, "configPath")?;
            let filters = FindingFilters::from_args(args)?;
            zzop_summary::analyze_summary(path, config_path, &filters)
        })(),
        "cross_repo" => (|| {
            // Every declared-type violation (a non-array `paths`, a non-string element inside it, a
            // non-string `configPath`) is a named error here — see `zzop_summary::args`'s module doc
            // for the silent-fallback class this replaces.
            //
            // Source-mode exclusivity is NOT decided here, exactly as in `analyze_repo` above: both
            // sources are passed through and `zzop_config::trees::load_trees_request` owns "exactly one
            // source" for every host. This arm used to re-state "not both" / "pass one" locally, which
            // gave the MCP user one sentence and the CLI twin another for the same judgment — the
            // per-host drift the shared handler exists to prevent, reintroduced by the copy meant to
            // help. Pinned by `tests::cross_repo_source_mode_errors_are_the_shared_handlers_verbatim`.
            let paths = args::optional_string_array(args, "paths")?;
            let config_path = args::optional_string(args, "configPath")?;
            let filters = FindingFilters::from_args(args)?;
            zzop_summary::cross_summary(&paths, config_path, &filters)
        })(),
        "check_file" => (|| {
            let target = args::required_string(args, "target")?;
            let source_id = args::optional_string(args, "sourceId")?;
            let path = args::optional_string(args, "path")?;
            let paths = args::optional_string_array(args, "paths")?;
            let config_path = args::optional_string(args, "configPath")?;
            zzop_summary::file_summary(target, source_id, path, &paths, config_path)
        })(),
        "check_endpoint" => (|| {
            let pattern = args::required_string(args, "pattern")?;
            let path = args::optional_string(args, "path")?;
            let paths = args::optional_string_array(args, "paths")?;
            let config_path = args::optional_string(args, "configPath")?;
            zzop_summary::endpoint_summary(pattern, path, &paths, config_path)
        })(),
        "analyze_envelope" => (|| {
            let envelope_json = args::required_string(args, "envelopeJson")?;
            let filters = FindingFilters::from_args(args)?;
            zzop_summary::analyze_envelope_summary(envelope_json, &filters)
        })(),
        "validate_envelope" => args::required_string(args, "envelopeJson")
            .map(zzop_summary::validate_envelope_only_json),
        "validate_rule_pack" => {
            args::required_string(args, "packJson").map(zzop_summary::validate_rule_pack_json)
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
