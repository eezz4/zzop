//! Version reporting — the SINGLE owner of "which zzop is this?", in two forms over one value:
//! [`version`] (the bare version string) and [`version_string`] (the same version plus the derived
//! extraction fingerprints, for diagnostics and the `tool` stamp of manifest/facts/graph). One module,
//! so the two forms can never disagree.
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

/// `version_string()`: this build's release version plus one `zzop-parser-<x>=<id>/<hash>` token per
/// parser crate and a `zzop-engine=<hash>` token — `zzop_engine::parser_fingerprints()` /
/// `zzop_engine::engine_fingerprint()`, the derived values that key the per-file cache.
///
/// **Build, not just frontend — this value answers "are these two analyses comparable".** Each parser
/// token's value joins the frontend's human-readable id (`rust/syn-2`, which never has to move) to the
/// source hash `crates/engine/build.rs` derives over that parser crate's whole dependency closure; the
/// engine token covers the one producer of cached bytes that is structurally absent from every parser
/// closure (`crates/engine` itself, plus the workspace manifest/lockfile). So an extraction change moves
/// this string with no hand-stamped version moving anywhere — the same edit that would invalidate a warm
/// cache restamps the `tool` field, one value with one owner. (Until 2026-08-03 this function printed
/// the frozen ids alone and its doc had to WARN "do not use this value to answer are-these-comparable";
/// the derived hashes are what retired that warning.)
///
/// Scope of the claim, stated so it cannot be over-read: "comparable" means the extraction/join
/// substrate the `tool`-stamped surfaces carry (`zzop manifest`'s identity rows, `zzop facts`'
/// `commonIr`/`crossLayer`, `zzop graph`). A build differing only in bundled rule-pack JSON moves
/// findings but none of those surfaces — and pack content self-invalidates in the cache via
/// `ruleset_fingerprint`, not this stamp.
///
/// The version comes from [`version`] itself, not a second `env!` of its own — the two forms report one
/// value by construction. (`ZZOP_RELEASE_VERSION` and the `0.0.0` placeholder are gone; every crate
/// shares the workspace version via `version.workspace = true`.)
pub fn version_string() -> String {
    let parsers = zzop_engine::parser_fingerprints();
    // By-name lookup rather than by-position iteration, so the literal `zzop-parser-<x>=` tokens stay
    // in THIS format string — they are what `scripts/check-version-lists-parsers.sh` and the
    // `capability_matrix` environment pin extract their parser inventory from.
    let fp = |name: &str| -> &str {
        parsers
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.as_str())
            .expect("zzop_engine::parser_fingerprints() names every parser crate")
    };
    format!(
        "zzop/{} zzop-parser-typescript={} zzop-parser-prisma={} zzop-parser-python-3={} \
         zzop-parser-java-21={} zzop-parser-rust={} zzop-parser-go={} zzop-parser-sql={} \
         zzop-parser-csharp={} zzop-engine={}",
        version(),
        fp("zzop-parser-typescript"),
        fp("zzop-parser-prisma"),
        fp("zzop-parser-python-3"),
        fp("zzop-parser-java-21"),
        fp("zzop-parser-rust"),
        fp("zzop-parser-go"),
        fp("zzop-parser-sql"),
        fp("zzop-parser-csharp"),
        zzop_engine::engine_fingerprint(),
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

    /// The diagnostic form reports EVERY parser's fingerprint plus the engine's — the whole reason the
    /// second form exists. `scripts/check-version-lists-parsers.sh` guards the other direction: a newly
    /// added parser crate must appear in the format string above.
    #[test]
    fn version_string_includes_parser_and_engine_fingerprints() {
        let v = super::version_string();
        for token in [
            "zzop-parser-typescript=",
            "zzop-parser-prisma=",
            "zzop-parser-python-3=",
            "zzop-parser-java-21=",
            "zzop-parser-rust=",
            "zzop-parser-go=",
            "zzop-parser-sql=",
            "zzop-parser-csharp=",
            "zzop-engine=",
        ] {
            assert!(v.contains(token), "missing {token} in: {v}");
        }
    }

    /// Every token's value is the DERIVED form, not the frozen id alone: it ends in the 16-hex source
    /// hash `crates/engine/build.rs` computes, and the parser tokens keep the human-readable id ahead
    /// of it (`<id>/<hash>`). This is what makes the stamp answer "are these two analyses comparable" —
    /// the exact property the pre-2026-08-03 string lacked, when it printed ids that deliberately do
    /// not move on an extraction change.
    #[test]
    fn version_string_values_carry_the_derived_source_hash() {
        let v = super::version_string();
        let value_of = |token: &str| -> &str {
            v.split_whitespace()
                .find_map(|t| t.strip_prefix(token))
                .unwrap_or_else(|| panic!("missing {token} in: {v}"))
        };
        for token in [
            "zzop-parser-typescript=",
            "zzop-parser-prisma=",
            "zzop-parser-python-3=",
            "zzop-parser-java-21=",
            "zzop-parser-rust=",
            "zzop-parser-go=",
            "zzop-parser-sql=",
            "zzop-parser-csharp=",
        ] {
            let value = value_of(token);
            let (id, hash) = value
                .rsplit_once('/')
                .unwrap_or_else(|| panic!("{token} value is not <id>/<hash>: {value}"));
            assert!(
                !id.is_empty(),
                "{token} lost its human-readable id: {value}"
            );
            assert!(
                hash.len() == 16 && hash.chars().all(|c| c.is_ascii_hexdigit()),
                "{token} hash half is not a 16-hex source hash: {value}"
            );
        }
        let engine = value_of("zzop-engine=");
        assert!(
            engine.len() == 16 && engine.chars().all(|c| c.is_ascii_hexdigit()),
            "zzop-engine value is not a 16-hex source hash: {engine}"
        );
    }

    /// Byte-identical across calls within one build — the stamp's "no source change, no movement"
    /// half, at the surface (the compile-time half is pinned in `zzop-engine`'s own cache tests).
    #[test]
    fn version_string_is_byte_identical_across_calls() {
        assert_eq!(super::version_string(), super::version_string());
    }
}
