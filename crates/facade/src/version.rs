//! The `version()` entry point.

/// `version()`: this build's release version plus every parser's
/// `PARSER_FINGERPRINT` (`zzop-cache`'s cache-key ingredient — see `zzop_parser_typescript::PARSER_FINGERPRINT`'s
/// doc), so a host app can log/report exactly which parser build produced a given analysis without needing
/// its own copy of those constants.
///
/// The version segment follows the same tag→binary stamping chain as `zzop-mcp`'s
/// `server::version()` (`packages/mcp/src/server.rs`): release builds are stamped at compile time —
/// the prebuild workflow's addon build step exports `ZZOP_RELEASE_VERSION` from the release tag
/// (`v1.2.3` → `1.2.3`), the same tag the publish job's `sync-versions.mjs` stamps into the npm
/// packages — so a released addon's reported version equals the `@zzop/native`/`@zzop/cli` package
/// version by construction. Everywhere else (local dev, CI tests, workflow_dispatch runs on a branch
/// ref) the env var is unset and the version falls back to `CARGO_PKG_VERSION`, the workspace-wide
/// `0.0.0` placeholder.
///
/// Note: `env!("CARGO_PKG_VERSION")` resolves to `zzop-facade`'s own crate version, not
/// `zzop-napi`'s — identical today since every workspace crate shares `version.workspace = true`, but a
/// trap if versions ever diverge, since the string below still prefixes the number with `"zzop-napi/"`.
pub fn version_string() -> String {
    format!(
        "zzop-napi/{} zzop-parser-typescript={} zzop-parser-prisma={} zzop-parser-python-3={} \
         zzop-parser-java-21={} zzop-parser-rust={} zzop-parser-go={}",
        option_env!("ZZOP_RELEASE_VERSION").unwrap_or(env!("CARGO_PKG_VERSION")),
        zzop_parser_typescript::PARSER_FINGERPRINT,
        zzop_parser_prisma::PARSER_FINGERPRINT,
        zzop_parser_python_3::PARSER_FINGERPRINT,
        zzop_parser_java_21::PARSER_FINGERPRINT,
        zzop_parser_rust::PARSER_FINGERPRINT,
        zzop_parser_go::PARSER_FINGERPRINT,
    )
}
