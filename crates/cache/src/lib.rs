//! zzop-cache — the file-level analysis cache.
//!
//! Two separate on-disk entries per file: the Common IR slice (keyed by content hash + parser
//! fingerprint) and per-file rule findings (same key + ruleset fingerprint), so a rule-pack-only
//! change invalidates findings but keeps the parsed IR reusable. Whole-graph passes are never cached —
//! they're a cheap linear combination of the per-file IRs. Every entry is file-independent, so this is
//! safe to drive from a `rayon` file-parallel walk.

mod evict;
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
/// - `zzop-config` derives [`DEFAULT_CACHE_DIR`] from it (the default a run whose config omits the key
///   resolves against its base directory).
/// - `zzop-engine`'s `dispatch::DEFAULT_SKIP_DIRS` lists it, alongside the removed JS CLI's
///   `.zzop-cache`, so a walk that never had a cache directory named to it still does not walk zzop's own
///   output as source: the walker runs `hidden(false)`, so a dot directory IS walked, and `.gitignore`
///   only covers users who have a git tree and remembered to write the rule.
///
/// **That skip-list entry is not what protects a run from its own output**, and must not be relied on as
/// if it were — it is a NAME in a list any caller may replace wholesale (the config front-end assigns
/// `dispatch.skip_dirs` from the declared `vocabulary.skipDirs`, which is an empty list when undeclared).
/// The structural protection lives in `zzop-engine`'s `pipeline::walking::walk_files`, on two independent
/// axes, and is sealed by BEHAVIOUR (analyze twice, assert the file count did not move) in
/// `crates/engine/tests/analyze_self_output_exclusion.rs`:
/// 1. **this constant is a RESERVED NAMESPACE** — a directory named `.zzop` is pruned wherever it appears,
///    ahead of anything configurable, so no caller can disarm it. Same standing `git` gives `.git`.
/// 2. **this run's own `cache_dir`** is pruned by resolved DIRECTORY, whatever it is named — so a cache
///    parked outside `.zzop` is covered too.
///
/// **Axis 1 landed 2026-07-29 and is a PARTIAL REVERSAL of the 2026-07-28 judgement** that what must be
/// excluded is "not the NAME `.zzop` but the one `cacheDir` this run wrote". Axis 2 alone was measurably
/// narrower than "zzop never walks its own output": it prunes the directory THIS run was told to write to,
/// so it was unarmed wherever there is no such directory to name —
/// - **caching turned off** (`"cacheDir": null` / any JSON-falsy value) — `cache_dir` is `None`, so nothing
///   was pruned and an earlier run's leftovers were walked as source. Measured on a one-file tree: 2 files
///   with caching on, **7** after switching it off over a populated `.zzop/cache`.
/// - **cacheDir moved from A to B** — B was pruned, A's leftovers were not.
///
/// What axis 1 costs: analyzing SOMEBODY ELSE'S `.zzop` tree on purpose now needs an opt-in knob that does
/// not exist. Taken knowingly — the name cannot mean two things, and everywhere else in this codebase it
/// already means "zzop's own derived output". The dotless sibling `zzop/` stays user-authored source.
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
    /// must live UNDER the tool directory, so that everything documented as covering `.zzop/` — this
    /// repo's own `.gitignore` rule (`**/.zzop/`), `zzop init`'s starter file, the shipped docs — keeps
    /// covering the cache.
    ///
    /// **What this does NOT seal**, despite what it claimed until 2026-07-29: that a second run will not
    /// walk the first run's cache. It compares one constant to another and passes no matter what any
    /// consumer of either does — a guard that seals a protection by asserting a constant exists, while
    /// leaving the wiring that consumes the constant unsealed. It sat green through the entire period in
    /// which the config front-end's wholesale `dispatch.skip_dirs` overwrite made an undeclared
    /// `vocabulary.skipDirs` disarm the skip-list entry, and the analyzed file count compounded every run.
    /// That defect is now closed structurally in `walk_files` and sealed by BEHAVIOUR (run twice, assert
    /// the file count is stable) in `crates/engine/tests/analyze_self_output_exclusion.rs` — which is
    /// where a change to this area has to stay green, not here.
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
