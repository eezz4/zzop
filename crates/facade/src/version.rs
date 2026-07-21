//! The `version_string()` entry point — part of the `zzop-facade` public surface, reporting this
//! build's version plus parser fingerprints to a direct embedder.

/// `version_string()`: this build's release version plus every parser's
/// `PARSER_FINGERPRINT` (`zzop-cache`'s cache-key ingredient — see `zzop_parser_typescript::PARSER_FINGERPRINT`'s
/// doc), so a host app can log/report exactly which parser build produced a given analysis without needing
/// its own copy of those constants.
///
/// Released builds are stamped at compile time with `ZZOP_RELEASE_VERSION` (from the release tag) — the
/// same tag->binary stamping chain as `zzop-mcp`'s `server::version()` (`packages/mcp/src/server.rs`),
/// its parity mirror — so a released build's reported version equals the release tag by construction.
/// Everywhere else the env var is unset and the version falls back to `CARGO_PKG_VERSION`, the
/// workspace-wide `0.0.0` placeholder.
///
/// Note: `env!("CARGO_PKG_VERSION")` resolves to `zzop-facade`'s own crate version — identical across
/// the workspace today since every crate shares `version.workspace = true`, but the string below still
/// prefixes the number with `"zzop/"`, so a future per-crate version divergence would be worth noting.
pub fn version_string() -> String {
    format!(
        "zzop/{} zzop-parser-typescript={} zzop-parser-prisma={} zzop-parser-python-3={} \
         zzop-parser-java-21={} zzop-parser-rust={} zzop-parser-go={} zzop-parser-sql={} \
         zzop-parser-csharp={}",
        option_env!("ZZOP_RELEASE_VERSION").unwrap_or(env!("CARGO_PKG_VERSION")),
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
