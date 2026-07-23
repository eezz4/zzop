//! Shared version reporting. The stdio JSON-RPC 2.0 MCP server loop itself (`initialize`, `tools/*`,
//! `resources/*`) is MCP-surface, not shared, and lives in the `zzop-mcp` package instead
//! (`packages/mcp/src/server.rs`, which re-exports `version` from here). This module stays in the
//! shared `zzop-host` lib so the `zzop` CLI's `version`/`--version` subcommand and the `zzop-mcp`
//! package's `initialize`/`version` handlers read the exact same value and can never disagree.

/// The version every host binary reports (`zzop version` / MCP `serverInfo.version`) —
/// `CARGO_PKG_VERSION`, the workspace `[workspace.package] version` (the release SSOT since the
/// 2026-07-22 version reform). CI verifies the pushed `v*` tag and `.claude-plugin/plugin.json` both
/// match it, so a released build's reported version equals the release tag and the plugin's published
/// version by construction.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    // `version()` reports `CARGO_PKG_VERSION` = the workspace version (release SSOT since the 2026-07-22
    // version reform — no `ZZOP_RELEASE_VERSION` env). CI verifies the release tag matches it.
    #[test]
    fn version_reports_cargo_pkg_version() {
        assert_eq!(super::version(), env!("CARGO_PKG_VERSION"));
    }
}
