//! Version reporting — the SINGLE owner of "which zzop is this?", in two forms over one value:
//! [`version`] (the bare version string) and [`version_string`] (the same version plus every parser's
//! fingerprint, for a direct embedder's diagnostics). One module, so the two forms can never disagree.
//!
//! Every reporting surface reads [`version`] through `zzop_summary`'s re-export: the `zzop` CLI's
//! `version`/`--version` subcommand, the `zzop-mcp` binary's own `version` argument, and MCP
//! `initialize`'s `serverInfo.version`. Before the 2026-07-26 `crates/host` teardown the bare form lived
//! in a separate crate from the fingerprint form, which is exactly how a value with two owners drifts.

/// The version every host binary reports (`zzop version` / `zzop-mcp version` / MCP
/// `serverInfo.version`) — `CARGO_PKG_VERSION`, the workspace `[workspace.package] version` (the release
/// SSOT since the 2026-07-22 version reform). CI verifies the pushed `v*` tag and
/// `.claude-plugin/plugin.json` both match it, so a released build's reported version equals the release
/// tag and the plugin's published version by construction.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// `version_string()`: this build's release version plus every parser's
/// `PARSER_FINGERPRINT` (`zzop-cache`'s cache-key ingredient — see `zzop_parser_typescript::PARSER_FINGERPRINT`'s
/// doc), so a host app can log/report exactly which parser build produced a given analysis without needing
/// its own copy of those constants.
///
/// The version comes from [`version`] itself, not a second `env!` of its own — the two forms report one
/// value by construction. (`ZZOP_RELEASE_VERSION` and the `0.0.0` placeholder are gone; every crate
/// shares the workspace version via `version.workspace = true`.)
pub fn version_string() -> String {
    format!(
        "zzop/{} zzop-parser-typescript={} zzop-parser-prisma={} zzop-parser-python-3={} \
         zzop-parser-java-21={} zzop-parser-rust={} zzop-parser-go={} zzop-parser-sql={} \
         zzop-parser-csharp={}",
        version(),
        zzop_parser_typescript::PARSER_FINGERPRINT,
        zzop_parser_prisma::PARSER_FINGERPRINT,
        zzop_parser_python_3::PARSER_FINGERPRINT,
        zzop_parser_java_21::PARSER_FINGERPRINT,
        zzop_parser_rust::PARSER_FINGERPRINT,
        zzop_parser_go::PARSER_FINGERPRINT,
        zzop_parser_sql::PARSER_FINGERPRINT,
        zzop_parser_csharp::PARSER_FINGERPRINT,
    )
}

#[cfg(test)]
mod tests {
    /// `version()` reports `CARGO_PKG_VERSION` = the workspace version (release SSOT since the
    /// 2026-07-22 version reform — no `ZZOP_RELEASE_VERSION` env). CI verifies the release tag matches
    /// it.
    #[test]
    fn version_reports_cargo_pkg_version() {
        assert_eq!(super::version(), env!("CARGO_PKG_VERSION"));
    }

    /// The two forms are one value: the diagnostic string's leading `zzop/<v>` token must be the bare
    /// `version()` — the drift a second `env!` here would have allowed (and did, across two crates,
    /// before the 2026-07-26 teardown put both forms in this module).
    #[test]
    fn version_string_leads_with_the_bare_version() {
        assert!(
            super::version_string().starts_with(&format!("zzop/{} ", super::version())),
            "got: {}",
            super::version_string()
        );
    }

    /// The diagnostic form reports EVERY parser's fingerprint — the whole reason the second form (and
    /// this crate's eight parser path deps) exists. `scripts/check-version-lists-parsers.sh` guards the
    /// other direction: a newly added parser crate must appear in the format string above.
    #[test]
    fn version_string_includes_parser_fingerprints() {
        let v = super::version_string();
        for parser in [
            "zzop-parser-typescript=",
            "zzop-parser-prisma=",
            "zzop-parser-python-3=",
            "zzop-parser-java-21=",
            "zzop-parser-rust=",
            "zzop-parser-go=",
            "zzop-parser-sql=",
            "zzop-parser-csharp=",
        ] {
            assert!(v.contains(parser), "missing {parser} in: {v}");
        }
    }
}
