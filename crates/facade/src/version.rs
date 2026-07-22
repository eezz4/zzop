//! The `version_string()` entry point — part of the `zzop-facade` public surface, reporting this
//! build's version plus parser fingerprints to a direct embedder.

/// `version_string()`: this build's release version plus every parser's
/// `PARSER_FINGERPRINT` (`zzop-cache`'s cache-key ingredient — see `zzop_parser_typescript::PARSER_FINGERPRINT`'s
/// doc), so a host app can log/report exactly which parser build produced a given analysis without needing
/// its own copy of those constants.
///
/// The version is `CARGO_PKG_VERSION` — the workspace `[workspace.package] version` (the release SSOT since
/// the 2026-07-22 version reform; `ZZOP_RELEASE_VERSION` and the `0.0.0` placeholder are gone). Every crate
/// shares it via `version.workspace = true`, and CI verifies the release tag matches it, so a build's
/// reported version equals its release tag by construction.
pub fn version_string() -> String {
    format!(
        "zzop/{} zzop-parser-typescript={} zzop-parser-prisma={} zzop-parser-python-3={} \
         zzop-parser-java-21={} zzop-parser-rust={} zzop-parser-go={} zzop-parser-sql={} \
         zzop-parser-csharp={}",
        env!("CARGO_PKG_VERSION"),
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
