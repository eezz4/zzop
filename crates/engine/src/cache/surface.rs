//! The DISCLOSURE half of the derived cache fingerprints — the same values `build.rs` computes and
//! [`super::parser_fingerprint`] folds into every per-file cache key, exported so the one reporting
//! surface (`zzop_facade::version_string`) can stamp them into run output: the `tool` field of
//! `zzop manifest`/`zzop facts`, `zzop graph`'s `%% tool:` census line, and what
//! `zzop version --verbose` prints.
//!
//! ## Why this surface exists (2026-08-03)
//!
//! The `tool` stamp's stated purpose is "which build produced this analysis — our own extraction
//! improvement can move keys with no change to the user's code" (`crates/summary/src/facts.rs`). The
//! frozen `PARSER_FINGERPRINT` ids alone cannot serve that purpose: each is an ID that deliberately
//! does NOT move when extraction code changes (its own doc says so). The values that DO move are the
//! derived closure hashes this module surfaces — the exact ingredients that invalidate a warm cache,
//! so the stamp and the cache key are one value with one owner, and "the stamp moved" means "a cached
//! answer would have been recomputed".
//!
//! ## What is deliberately NOT here
//!
//! The per-RUN ingredients of the real cache key — `size_cap`, `io` options, scope, the
//! vocabulary/ruleset fingerprints — describe a run's configuration, not the build. The stamp answers
//! "which build"; folding run config into it would make two runs of one binary read as two tools.

/// Every parser crate's derived fingerprint, `(crate name, "<id>/<source hash>")` — the
/// config-independent base of [`super::parser_fingerprint`]'s per-language arms (the same
/// [`super::derived`] composition, over the same `FP_*` closure hashes from `build.rs`).
///
/// Deterministic by construction: the entries are a compile-time list sorted by crate name, and each
/// value is a pure format over two compile-time constants — two calls in one build are byte-identical,
/// and two builds differ exactly when some parser's dependency-closure source differs.
/// `cache::tests::parser_fingerprints_surface_matches_every_dispatch_arm` pins this list against the
/// cache key's own arms, so a ninth parser cannot be wired into dispatch without surfacing here.
pub fn parser_fingerprints() -> Vec<(&'static str, String)> {
    vec![
        (
            "zzop-parser-csharp",
            super::derived(zzop_parser_csharp::PARSER_FINGERPRINT, super::FP_CSHARP),
        ),
        (
            "zzop-parser-go",
            super::derived(zzop_parser_go::PARSER_FINGERPRINT, super::FP_GO),
        ),
        (
            "zzop-parser-java-21",
            super::derived(zzop_parser_java_21::PARSER_FINGERPRINT, super::FP_JAVA),
        ),
        (
            "zzop-parser-prisma",
            super::derived(zzop_parser_prisma::PARSER_FINGERPRINT, super::FP_PRISMA),
        ),
        (
            "zzop-parser-python-3",
            super::derived(zzop_parser_python_3::PARSER_FINGERPRINT, super::FP_PYTHON),
        ),
        (
            "zzop-parser-rust",
            super::derived(zzop_parser_rust::PARSER_FINGERPRINT, super::FP_RUST),
        ),
        (
            "zzop-parser-sql",
            super::derived(zzop_parser_sql::PARSER_FINGERPRINT, super::FP_SQL),
        ),
        (
            "zzop-parser-typescript",
            super::derived(
                zzop_parser_typescript::PARSER_FINGERPRINT,
                super::FP_TYPESCRIPT,
            ),
        ),
    ]
}

/// The engine's own source hash (`FP_ENGINE` — this crate's `src/**.rs` + `build.rs` +
/// `crates/engine/Cargo.toml` + the workspace `Cargo.toml`/`Cargo.lock`), the shared `+engine=` suffix
/// on every cache-key arm.
///
/// Surfaced separately from [`parser_fingerprints`] because it is the one producer of cached bytes
/// that is structurally absent from every parser closure (`crates/engine` is the crate that DEPENDS on
/// them — see `build.rs`). Without it on the stamp, two builds differing only in engine code
/// (`pipeline/io_projection.rs`, the `fresh.rs` gates, the stored finding text) would report
/// byte-identical `tool` strings while a warm cache treats them as different builds.
pub fn engine_fingerprint() -> &'static str {
    super::FP_ENGINE
}
