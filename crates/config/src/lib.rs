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
//! request, and the zero-config paths-mode tree list. "What is a valid config" and "what request does
//! that config become" answer to the same owner.
//!
//! One consequence worth stating plainly, because it is the only place this crate causes a WRITE: with
//! the `cacheDir` default in place, an otherwise read-only analysis creates `.zzop/cache/` inside the
//! analyzed tree on its first run. `"cacheDir": null` opts out (see `mapper::options`); any other
//! directory can be named instead. One `base_dir` means one cache directory, so a config file's trees
//! (`trees: "auto"` included) all share ONE — but a caller that maps a bare `{}` per path
//! (`zzop-summary`'s config-free paths mode) has a base per path and therefore one `.zzop/` per path.
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

use std::path::{Path, PathBuf};

pub mod jsonc;
#[cfg(test)]
mod lib_tests;
pub mod mapper;
pub mod paths;
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

// `BUNDLED_PACK_SOURCES: &[(&str, &str)]` — (relative path under rules/dsl, pack JSON source),
// embedded at compile time. See build.rs.
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
    /// The config file actually loaded, if any — `None` means the zero-config default request.
    pub config_path: Option<PathBuf>,
}

/// Loads the config for a single analyzed root: reads `<root>/zzop.config.jsonc`, or REFUSES.
///
/// Errors only on caller-fixable config problems (`ConfigError`), never on overlay/pack content issues
/// (those become warnings). When the result is a single tree over a root whose workspace manifest names
/// 2+ packages, the leading disclosure warning from [`disclose_single_tree_over_workspace`] is prepended —
/// the request stays byte-identical either way, the run just stops being silent about the join it could
/// not attempt.
///
/// # There is no zero-config run (2026-07-27, reversing the founding default)
///
/// This used to synthesize the request an empty `{}` config would produce. That became unsafe the day
/// convention vocabulary stopped falling back to built-ins (`zzop_engine::vocabulary`): no config means
/// no declaration, and no declaration means those judgments are not made — the run would analyze less
/// while reporting itself complete. Total refusal rather than "warn and continue", because a warning
/// beside a shorter findings list is exactly the disclosure readers skip. [`missing_config_error`] owns
/// the message; both hosts hit this same function, so the entry axis has no parity exception.
pub fn load_for_root(root: &Path) -> Result<LoadedRequest, ConfigError> {
    let candidate = root.join(DEFAULT_CONFIG_FILENAME);
    if candidate.is_file() {
        return load_config_file(&candidate);
    }
    Err(missing_config_error(&candidate))
}

/// The one refusal message for "this analysis has no config", shared by every lane and both hosts.
///
/// Names the ARTIFACT and never a command. A message saying `zzop init` is unactionable for an MCP client
/// (it has no shell), and one saying `resources/read` is unactionable in a terminal; the `config-template`
/// contract document is the single thing both hosts can serve, and it is the same bytes `init` writes.
/// Machine-pinned by `crates/engine/tests/rule_contracts/host_vocabulary.rs` (contracts 15 & 16), which
/// scans this crate for either host's vocabulary.
pub fn missing_config_error(candidate: &Path) -> ConfigError {
    ConfigError(format!(
        "No config file at {}.\nzzop analyzes what a config DECLARES: the convention vocabulary a \
         project picks — what it calls its auth guards, which banners mark its generated files, how it \
         names its data-access receivers — has no built-in default, so a run without a config would \
         judge less while reporting itself complete. Start from the `config-template` contract document \
         (every zzop surface can serve it) and save it as {}.",
        candidate.display(),
        candidate.display()
    ))
}

/// Loads an explicit config file (or a directory containing `zzop.config.jsonc`) and maps it with
/// the config file's directory as the resolution base. This is the multi-tree/cross-repo entry: the
/// config's `trees` (including `trees: "auto"`) defines the join.
///
/// Unlike the JS CLI's `loadConfig` (which only ever reads the exact path it is given), `path` here
/// may also name a DIRECTORY containing `zzop.config.jsonc` — a Rust-host convenience with no JS
/// counterpart (the JS CLI never receives a bare directory; `bin/zzop.js` always resolves `--config`
/// or the default filename before calling `loadConfig`).
///
/// A config that never declares `trees` still resolves to ONE tree, so it gets the same leading
/// [`disclose_single_tree_over_workspace`] disclosure the config-less path gets (worded for the
/// "add it to your existing config" remedy) — having a config file is not evidence that the tree
/// set was chosen.
pub fn load_config_file(path: &Path) -> Result<LoadedRequest, ConfigError> {
    let candidate = if path.is_dir() {
        path.join(DEFAULT_CONFIG_FILENAME)
    } else {
        path.to_path_buf()
    };

    // Same condition, same message as the discovery path — one owner, so the two entries can never
    // teach a user two different remedies for the identical missing file.
    if !candidate.is_file() {
        return Err(missing_config_error(&candidate));
    }

    let raw = std::fs::read_to_string(&candidate).map_err(|err| {
        ConfigError(format!(
            "Could not read config at {}: {}",
            candidate.display(),
            io_error_label(&err)
        ))
    })?;

    let stripped = jsonc::strip_json_comments(&raw);
    let parsed: serde_json::Value = serde_json::from_str(&stripped)
        .map_err(|err| ConfigError(format!("Invalid JSONC in {}: {err}", candidate.display())))?;

    if !parsed.is_object() {
        return Err(ConfigError(format!(
            "Config in {} must be a JSON object.",
            candidate.display()
        )));
    }

    let base_dir = candidate
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    let (config, mut warnings) = maybe_expand_auto_trees(parsed, &base_dir)?;
    let mapped = mapper::config_to_request(&config, &base_dir)?;
    warnings.extend(mapped.warnings);
    disclose_single_tree_over_workspace(
        mapped.method,
        &base_dir,
        &candidate,
        &config,
        &mut warnings,
    );
    Ok(LoadedRequest {
        method: mapped.method,
        request: mapped.request,
        warnings,
        config_path: Some(candidate),
    })
}

/// Prepends `workspaces::single_tree_workspace_warning` (when it fires) to `warnings` — the ONE
/// place a load discloses "this run is a single tree over a monorepo, so the cross-layer join never
/// ran". Only [`load_config_file`] reaches it now: since 2026-07-27 there is no config-less load, so
/// the config path and the config itself are always in hand rather than optional.
///
/// Gated on `Method::Analyze`, which is exactly the structural fact worth warning about: by this
/// enum's own contract it means one tree resulted AND `trees` was never declared. Keying on that
/// (rather than on "no config file was found") is what makes a config carrying only
/// `{"rules": {...}}` — a real monorepo trap, since no `trees` means no expansion and therefore no
/// expansion report to speak in its place — disclose exactly like the config-less case, while an
/// author who DID declare `trees` (any explicit array, or `"auto"`) is never second-guessed: those
/// take the `AnalyzeTrees` branch and never reach here. See that function's doc for the rest of the
/// silence contract.
///
/// Position: `insert(0, ..)`, never `push` — this is a leading disclosure, and pinning the index
/// keeps the warning list byte- AND order-deterministic no matter how many mapping notes precede it.
/// Purely additive: the request, method and tree set are untouched.
fn disclose_single_tree_over_workspace(
    method: Method,
    base_dir: &Path,
    config_path: &Path,
    config: &serde_json::Value,
    warnings: &mut Vec<String>,
) {
    if method != Method::Analyze {
        return;
    }
    if let Some(warning) = workspaces::single_tree_workspace_warning(base_dir, config_path, config)
    {
        warnings.insert(0, warning);
    }
}

/// Thin `workspaces::expand_auto_trees` gate: that function is a documented no-op for any config
/// whose `trees` is not EXACTLY the string `"auto"` (see its own module doc), so this short-circuits
/// the common case without calling into it at all. Purely an implementation courtesy — the observable
/// result is identical to an unconditional call once `workspaces.rs` is implemented; it just means
/// `zzop-config`'s own build/tests for every other config shape don't take a hard dependency on that
/// (separately owned) module being finished first.
fn maybe_expand_auto_trees(
    config: serde_json::Value,
    base_dir: &Path,
) -> Result<(serde_json::Value, Vec<String>), ConfigError> {
    if config.get("trees").and_then(serde_json::Value::as_str) == Some("auto") {
        workspaces::expand_auto_trees(config, base_dir)
    } else {
        Ok((config, Vec::new()))
    }
}
