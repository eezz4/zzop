//! zzop-cache — the file-level analysis cache.
//!
//! Two separate on-disk entries per file: the Common IR slice (keyed by content hash + parser
//! fingerprint) and per-file rule findings (same key + ruleset fingerprint), so a rule-pack-only
//! change invalidates findings but keeps the parsed IR reusable. Whole-graph passes are never cached —
//! they're a cheap linear combination of the per-file IRs. Every entry is file-independent, so this is
//! safe to drive from a `rayon` file-parallel walk.

mod hash;
mod ir_slice;
mod key;
mod store;

pub use ir_slice::FileIrSlice;
pub use key::CacheKey;
pub use store::AnalysisCache;

/// The directory zzop writes its OWN derived artifacts into, relative to the analyzed tree (or, for a
/// config-file run, relative to the config file's directory). Derived state only — deleting it is always
/// safe, it is regenerated on the next run — which is exactly why it is a DOT directory, and why the
/// user-authored sibling `zzop/` (custom rule packs, adapter overlays) deliberately is NOT: authored
/// source is written, reviewed, committed and diffed by people, and a dot directory hides it from file
/// explorers and search tools while signalling "generated, safe to delete" about something that is not.
///
/// **This is the T1 single definition of that name.** Two crates need it and neither may spell it as a
/// literal of its own:
/// - `zzop-config` derives [`DEFAULT_CACHE_DIR`] from it (the default a config-file/zero-config run
///   resolves against its base directory).
/// - `zzop-engine`'s `dispatch::DEFAULT_SKIP_DIRS` lists it so the tree walker never walks zzop's own
///   output as source. That entry is load-bearing, not tidiness: the walker runs `hidden(false)`, so a
///   dot directory IS walked, and `.gitignore` only covers users who have a git tree and remembered to
///   write the rule. The removed JS CLI's `.zzop-cache` is in that same list for exactly this reason —
///   after a blind field test observed the analyzed file count growing every run.
pub const TOOL_DIR: &str = ".zzop";

/// Default on-disk cache directory (`.zzop/cache`), relative to the resolution base — the directory of
/// the `zzop.config.jsonc` for a config-file run, or the analyzed root for a config-less one. Applied by
/// the config front-end (`zzop-config`), NOT by `zzop-engine`/`zzop-facade`: an embedder calling the
/// library directly still gets "no cache unless you name a directory", so nothing but the product
/// front-end ever creates a directory in someone's repo unasked.
///
/// Overriding is the `cacheDir` config key: a string picks another directory, and a JSON-falsy value
/// (`null`, canonically) turns the cache off entirely.
pub const DEFAULT_CACHE_DIR: &str = ".zzop/cache";

#[cfg(test)]
mod dir_tests {
    use super::{DEFAULT_CACHE_DIR, TOOL_DIR};

    /// Seals the one relation the two path constants have to each other: the default cache directory
    /// must live UNDER the tool directory. If it ever did not, `zzop-engine`'s skip-list entry (which
    /// names `TOOL_DIR`) would stop covering the cache, and the next run would walk the cache it just
    /// wrote as source — the self-scan pollution the skip list exists to prevent.
    #[test]
    fn the_default_cache_dir_lives_under_the_tool_dir() {
        assert!(
            DEFAULT_CACHE_DIR.starts_with(&format!("{TOOL_DIR}/")),
            "{DEFAULT_CACHE_DIR} must be under {TOOL_DIR}/"
        );
    }

    /// Value pin (T2 style, for the surfaces that cannot import these symbols): both names appear
    /// verbatim in shipped documentation and in this repo's own `.gitignore` (`**/.zzop/`), so a change
    /// here is a public, documented change — never a silent one.
    #[test]
    fn the_on_disk_names_are_pinned() {
        assert_eq!(TOOL_DIR, ".zzop");
        assert_eq!(DEFAULT_CACHE_DIR, ".zzop/cache");
    }
}
