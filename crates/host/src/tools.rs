//! Shared dispatch: the typed CLI-facing wrappers (`analyze`/`analyze_envelope`/`cross_repo`/
//! `check_endpoint`/`validate_envelope`/`validate_rule_pack`) the `zzop` CLI (package `zzop-cli-bin`)
//! calls for its `analyze`/`analyze-envelope`/`cross`/`endpoint`/`validate-*` subcommands. This module
//! is pure dispatch: no shaping/filtering/warning-merge logic lives here — that all lives in the shared
//! `zzop-summary` crate (config auto-discovery + facade call + summary assembly), so it cannot drift
//! per-host. The MCP tool schemas (`tools/list`) and `tools/call` dispatch are MCP-surface and live in
//! the `zzop-mcp` package (`packages/mcp/src/tools.rs`) instead — that dispatch calls the identical
//! `zzop_summary` functions this module calls, so a CLI query and an MCP tool call give the identical
//! answer.

#[cfg(test)]
mod tests;

use zzop_summary::FindingFilters;

/// CLI `zzop analyze <path>` — default filters. Thin re-export: all shaping lives in
/// `zzop_summary::analyze_summary`.
pub fn analyze(path: &str) -> Result<String, String> {
    zzop_summary::analyze_summary(path, &default_filters())
}

/// CLI `zzop analyze-envelope <envelope.json>` — default filters. Thin re-export: all shaping
/// (and the facade call) lives in `zzop_summary::analyze_envelope_summary`.
pub fn analyze_envelope(envelope_json: &str) -> Result<String, String> {
    zzop_summary::analyze_envelope_summary(envelope_json, &default_filters())
}

/// CLI `zzop cross <path>...` / `zzop cross --config <path>` — default filters. Thin
/// re-export: all shaping lives in `zzop_summary::cross_summary`.
pub fn cross_repo(paths: &[String], config_path: Option<&str>) -> Result<String, String> {
    zzop_summary::cross_summary(paths, config_path, &default_filters())
}

/// `check_endpoint` MCP tool / `zzop endpoint` CLI subcommand — thin re-export: the tree
/// resolution and query core both live in `zzop_summary::endpoint_summary`.
pub fn check_endpoint(
    pattern: &str,
    path: Option<&str>,
    paths: &[String],
    config_path: Option<&str>,
) -> Result<String, String> {
    zzop_summary::endpoint_summary(pattern, path, paths, config_path)
}

/// CLI `zzop validate-envelope <path>` — offline Normalized-AST envelope shape check (the
/// `validate_envelope` MCP tool's answer, from a terminal). Returns the infallible
/// `{"valid":bool,"issues":[…]}` report as a string; `main` reads `valid` for the exit code. Thin
/// re-export: the check lives in `zzop_summary` (same function the tool dispatch calls above).
pub fn validate_envelope(envelope_json: &str) -> String {
    zzop_summary::validate_envelope_only_json(envelope_json)
}

/// CLI `zzop validate-rule-pack <path>` — offline DSL rule-pack shape + matcher-regex check (the
/// `validate_rule_pack` MCP tool's answer). Same infallible report contract as [`validate_envelope`].
pub fn validate_rule_pack(pack_json: &str) -> String {
    zzop_summary::validate_rule_pack_json(pack_json)
}

fn default_filters() -> FindingFilters {
    FindingFilters::from_args(None).expect("no-args filters always parse")
}
