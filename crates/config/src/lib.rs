//! zzop-config — the config front-end shared by both native host products: the `zzop` CLI
//! (`packages/cli-bin`, whose multi-tree subcommands take `--config`; that binary's argv dispatch in
//! `main.rs` is the canonical list, and the fixed-arity `analyze` is deliberately NOT one of them — it
//! rejects any dash-shaped argument) and the `zzop-mcp` server (`packages/mcp`, whose tools take
//! `configPath`). It turns a `zzop.config.jsonc` (or its absence) into a `zzop-facade` request JSON, so
//! every host drives one engine code path (`zzop-facade`'s `build_engine_config`) from a single config
//! implementation.
//!
//! Originally ported from the removed JS front-end (its `config.js` + `mapper.js` + the embedder
//! wrapper's `withDefaults`) — that lineage is why the work splits into three layers, all load-bearing
//! (missing any one drifts silently):
//! 1. discovery/loading — the literal filename `zzop.config.jsonc`, NO ancestor walk.
//! 2. mapping — pure config→request mapping with its fail-fast validation gates; unknown keys warn
//!    (never reject) using the shared vocabulary in `config-surface.json` (embedded here).
//! 3. default-injection (`withDefaults`) — the easy-to-miss third layer that injects the bundled rule
//!    packs, a default `git: {}` (30-day recency collection), and a default `cacheDir`
//!    ([`zzop_cache::DEFAULT_CACHE_DIR`], `.zzop/cache`) right before the engine call. Skipping it means
//!    0 DSL rules, 0 git signals and a permanently cold cache with no crash — the worst kind of drift.
//!    Here the bundled packs are injected as inline `packDefs` (embedded at compile time by `build.rs`)
//!    instead of a bundled directory path, so the binary needs no sidecar files.
//!
//! Path resolution: `root`/`cacheDir`/`packsDir` are resolved against the config file's OWN directory
//! (a server host's process cwd is meaningless). Overlay paths are tree-root-relative. Host-boundary
//! absolutization of the incoming path arguments themselves lives in `paths` — the seam every tree root
//! and `--config`/`configPath` file crosses before this crate's lexical resolution sees it.
//!
//! A fourth layer sits on top of those three, for the same "one implementation, no per-host drift"
//! reason: REQUEST ASSEMBLY (`trees`) — which source mode a call is in (`path` XOR `paths` XOR
//! `configPath`), whether a single-tree config is refused or wrapped into a one-entry `analyzeTrees`
//! request, and the paths-mode tree list. "What is a valid config" and "what request does
//! that config become" answer to the same owner.
//!
//! One consequence worth stating plainly, because it is the only place this crate causes a WRITE: with
//! the `cacheDir` default in place, an otherwise read-only analysis creates `.zzop/cache/` inside the
//! analyzed tree on its first run. `"cacheDir": null` opts out (see `mapper::options`); any other
//! directory can be named instead. One `base_dir` means one cache directory, so a config file's trees
//! (`trees: "auto"` included) all share ONE — but a caller that maps a bare `{}` per path
//! (`zzop-summary`'s multi-root paths mode) has a base per path and therefore one `.zzop/` per path.
//! Ignore it with `**/.zzop/`, which covers both — never a `zzop*` glob, which would also swallow the
//! user-authored `zzop/` one character away.
//!
//! Non-fatal by design (do not "fix" with `?`): unreadable/invalid overlay files, duplicate
//! `sourceId`s from `trees: "auto"`, and unknown config keys are all WARNINGS, never errors — the
//! pipeline threads a warnings collector instead of failing. The same channel carries this crate's
//! one PROACTIVE disclosure — `workspaces::single_tree_workspace_warning`, emitted whenever a run
//! resolves ONE tree over a root whose workspace manifest names 2+ packages (no config file, or a
//! config that never declares `trees`), so a monorepo never degrades to a single tree — and thus no
//! cross-layer join — in silence.

use std::path::PathBuf;

pub mod jsonc;
#[cfg(test)]
mod lib_tests;
mod load;
pub mod mapper;
pub mod paths;

pub use load::{
    load_config_file, load_for_root, load_for_root_vocabulary_only, missing_config_error,
    MISSING_CONFIG_MARKER,
};
pub mod template;
#[cfg(test)]
mod template_tests;
#[cfg(test)]
pub(crate) mod test_support;
pub mod trees;
pub mod workspaces;

/// Default config filename, discovered directly under the analyzed root (no ancestor walk).
pub const DEFAULT_CONFIG_FILENAME: &str = "zzop.config.jsonc";

/// Where DSL rule packs are discovered when a config declares no `packs.extraDirs` at all —
/// `zzop/rules/`, resolved against the mapping base directory, and used ONLY if it exists on disk.
///
/// The `zzop/` prefix is the authored half of the on-disk two-way split, and the ONLY place this crate
/// names it: `zzop/` (no dot) is what a PERSON wrote and version-controls, against [`zzop_cache::
/// TOOL_DIR`]'s `.zzop/` — one character away — which is what the TOOL derives and can delete. Nothing
/// here ever writes under `zzop/`; this constant is a read location only.
///
/// Fallback, never a merge: a declared `packs.extraDirs` wins outright (the empty array included, which
/// is therefore the explicit opt-out), so a run's pack directories always have exactly one origin and
/// precedence is never ambiguous. A base directory with no `zzop/rules/` produces no warning — a repo
/// that authored no packs has nothing to disclose, and warning on every such repo would be pure noise.
pub const DEFAULT_AUTHORED_PACKS_DIR: &str = "zzop/rules";

/// The shared config-key vocabulary (`crates/config/config-surface.json`), embedded so unknown-key
/// warnings use the exact same key list as the engine's `rule_contracts` meta test.
pub const CONFIG_SURFACE_JSON: &str = include_str!("../config-surface.json");

// Two generated tables, both embedded at compile time by build.rs:
//   `BUNDLED_PACK_SOURCES: &[(&str, &str)]`         — (relative path under rules/dsl, pack JSON source)
//   `EXAMPLE_PACK_CONTRACTS: &[(&str, &str, &str)]` — (contract-resource name, description, pack JSON
//                                                      source) for every `examples/packs/*.json`
// The second is a DISTRIBUTION concern rather than a config one, and it lives here for a plain reason:
// this crate already owns the compile-time embedding of repo files, and `crates/summary` (which serves
// the contract table) depends on it. The alternative was a second build script in a crate that has
// none. See build.rs's `example_pack_contracts` for what the rows are for.
include!(concat!(env!("OUT_DIR"), "/bundled_packs.rs"));

/// Renders an `io::Error` as a fixed-vocabulary, English, deterministic label — `NotFound (os error
/// 2)` — instead of its `Display` form. `io::Error`'s `Display` on Windows comes from
/// `FormatMessageW` in the OS UI LANGUAGE (a Korean host renders a Korean sentence), which would
/// leak locale-dependent, non-deterministic text into warnings/errors that AI-agent consumers and
/// tests read (release-audit message lens, v0.16.0). `ErrorKind`'s `Debug` names are stable English;
/// the raw OS code keeps the message diagnosable.
pub(crate) fn io_error_label(err: &std::io::Error) -> String {
    match err.raw_os_error() {
        Some(code) => format!("{:?} (os error {code})", err.kind()),
        None => format!("{:?}", err.kind()),
    }
}

/// A configuration/usage error the caller should surface verbatim and treat as caller-fixable —
/// mirrors the JS `ConfigError` (exit code 2 in the CLI; an `isError` tool result over MCP).
#[derive(Debug)]
pub struct ConfigError(pub String);

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ConfigError {}

/// Which facade entry a mapped config drives — mirrors `mapper.js`'s method-selection rule exactly:
/// `Analyze` iff exactly one tree resulted AND `trees` was never set (a single-entry `trees: [...]`
/// still selects `AnalyzeTrees`, unlike a single `roots: ["."]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Analyze,
    AnalyzeTrees,
}

/// One fully-mapped analysis request, ready for `zzop-facade`: `request` is the `AnalyzeRequest`
/// (for `Method::Analyze`) or `AnalyzeTreesRequest` (`{trees: [...]}`) JSON value, with the bundled
/// `packDefs` and default `git: {}` already injected. `warnings` carries every non-fatal note
/// (unknown keys, skipped overlays, auto-expansion reports, and the single-tree-over-a-workspace
/// disclosure) for the caller's warnings channel.
#[derive(Debug)]
pub struct LoadedRequest {
    pub method: Method,
    pub request: serde_json::Value,
    pub warnings: Vec<String>,
    /// The config file actually loaded, if any — `None` means no single file governs this request
    /// (paths mode loads one per root; an envelope has no filesystem location at all).
    pub config_path: Option<PathBuf>,
}
