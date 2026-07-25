//! `zzop manifest` / `zzop diff` — the structural-drift lane, and (like `explain`) a CLI-ONLY one, so
//! it sits outside `tools`' MCP-mirrored dispatch. Thin re-exports: both functions are pure and live
//! whole in `zzop-summary` (see `zzop_summary::manifest`'s own module doc for the schema and the two
//! honesty gates), exactly like every other handler this crate forwards.
//!
//! ## Why no MCP tool twin (a judgment, recorded so it is not re-litigated by accident)
//! Recorded as a contract in `docs/contracts/surface-parity.json`'s `_cliOnlyLanes`. Three reasons,
//! in order of weight:
//! 1. **A manifest is deliberately UNCAPPED.** Every MCP reply this repo ships is cap-governed (the
//!    token-bomb guard `zzop_summary::output` exists for). Putting a manifest on that wire would
//!    force a choice between breaking that doctrine and capping the one artifact whose entire reason
//!    for existing is that the caps make the capped summaries un-diffable.
//! 2. **The workflow is file-shaped, not conversation-shaped.** A manifest earns its keep by being
//!    COMMITTED and compared by a later run (the `scripts/max-file-lines-baseline.txt` model) — a
//!    terminal/CI act. An agent that wanted this can already shell out to the same binary.
//! 3. **`diff` alone would be half a surface.** It consumes manifests; without a `manifest` twin an
//!    agent could not produce its own inputs.

/// CLI `zzop manifest <path>...` / `zzop manifest --config <path>`.
pub fn manifest(paths: &[String], config_path: Option<&str>) -> Result<String, String> {
    zzop_summary::manifest_json(paths, config_path)
}

/// CLI `zzop diff <a.json> <b.json> [--allow-tool-drift]` — pure delta over two already-produced
/// manifests (no analysis, no filesystem beyond the two files the CLI already read).
pub fn diff(a_json: &str, b_json: &str, allow_tool_drift: bool) -> Result<String, String> {
    zzop_summary::diff_manifests_json(a_json, b_json, allow_tool_drift)
}
