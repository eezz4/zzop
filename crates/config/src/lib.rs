//! zzop-config — the Rust port of the JS config front-end, shared by every native host (the zzop-mcp
//! binary today; the binary's own CLI surface if it subsumes the JS CLI later). It turns a
//! `zzop.config.jsonc` (or its absence) into the SAME facade request JSON the JS CLI's
//! `mapper.js` + the `@zzop/native` JS wrapper's default-injection layer produce, so the Node path and the
//! Node-free path drive one engine code path (`zzop-facade`'s `build_engine_config`) with
//! machine-comparable inputs.
//!
//! Three JS layers are ported (all of them — missing any one drifts silently):
//! 1. `config.js`   — discovery/loading: the literal filename `zzop.config.jsonc`, NO ancestor walk.
//! 2. `mapper.js`   — pure config→request mapping with its fail-fast validation gates; unknown keys
//!    warn (never reject) using the shared vocabulary in `config-surface.json` (embedded here).
//! 3. `packages/native/index.js`'s `withDefaults` — the easy-to-miss third layer that injects the
//!    bundled rule packs and a default `git: {}` (30-day recency collection) right before the native
//!    call. Skipping it means 0 DSL rules and 0 git signals with no crash — the worst kind of drift.
//!    Here the bundled packs are injected as inline `packDefs` (embedded at compile time by
//!    `build.rs`) instead of a bundled directory path, so the binary needs no sidecar files.
//!
//! Path resolution deviates from the JS CLI in ONE documented way: the JS CLI leaves `root`/
//! `cacheDir`/`packsDir` as literal strings for the engine process cwd to resolve (which in normal
//! CLI use IS the config's directory); a server host's cwd is meaningless, so this port resolves
//! them against the config file's own directory. Overlay paths keep JS parity (tree-root-relative).
//!
//! Non-fatal by design (do not "fix" with `?`): unreadable/invalid overlay files, duplicate
//! `sourceId`s from `trees: "auto"`, and unknown config keys are all WARNINGS, never errors — the
//! JS pipeline threads a warnings collector instead of failing, and so does this port.

use std::path::{Path, PathBuf};

pub mod jsonc;
#[cfg(test)]
mod lib_tests;
pub mod mapper;
#[cfg(test)]
pub(crate) mod test_support;
pub mod workspaces;

/// Default config filename, discovered directly under the analyzed root — mirrors the JS CLI's
/// `DEFAULT_CONFIG_FILENAME` (`packages/cli/lib/config.js`), including its "no ancestor walk" rule.
pub const DEFAULT_CONFIG_FILENAME: &str = "zzop.config.jsonc";

/// The shared config-key vocabulary (`packages/cli/lib/config-surface.json`), embedded so unknown-key
/// warnings use the exact same key list as the JS CLI and the engine's `rule_contracts` meta test.
pub const CONFIG_SURFACE_JSON: &str = include_str!("../../../packages/cli/lib/config-surface.json");

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
/// (unknown keys, skipped overlays, auto-expansion reports) for the caller's warnings channel.
#[derive(Debug)]
pub struct LoadedRequest {
    pub method: Method,
    pub request: serde_json::Value,
    pub warnings: Vec<String>,
    /// The config file actually loaded, if any — `None` means the zero-config default request.
    pub config_path: Option<PathBuf>,
}

/// Loads the config for a single analyzed root: reads `<root>/zzop.config.jsonc` when present
/// (mapping it relative to `root`), else produces the zero-config default request for `root`
/// (bundled packs + default git collection still injected — zero-config must not silently lose
/// them). Errors only on caller-fixable config problems (`ConfigError`), never on overlay/pack
/// content issues (those become warnings).
pub fn load_for_root(root: &Path) -> Result<LoadedRequest, ConfigError> {
    let candidate = root.join(DEFAULT_CONFIG_FILENAME);
    if candidate.is_file() {
        return load_config_file(&candidate);
    }

    // Zero-config default: the exact same request an empty `{}` config would produce for `root` —
    // this deliberately differs from the JS CLI (which errors without a config file at all): an
    // MCP tool pointed at any repo must still work, bundled packs and default git collection included.
    let empty_config = serde_json::Value::Object(serde_json::Map::new());
    let (config, mut warnings) = maybe_expand_auto_trees(empty_config, root)?;
    let mapped = mapper::config_to_request(&config, root)?;
    warnings.extend(mapped.warnings);
    Ok(LoadedRequest {
        method: mapped.method,
        request: mapped.request,
        warnings,
        config_path: None,
    })
}

/// Loads an explicit config file (or a directory containing `zzop.config.jsonc`) and maps it with
/// the config file's directory as the resolution base. This is the multi-tree/cross-repo entry: the
/// config's `trees` (including `trees: "auto"`) defines the join.
///
/// Unlike the JS CLI's `loadConfig` (which only ever reads the exact path it is given), `path` here
/// may also name a DIRECTORY containing `zzop.config.jsonc` — a Rust-host convenience with no JS
/// counterpart (the JS CLI never receives a bare directory; `bin/zzop.js` always resolves `--config`
/// or the default filename before calling `loadConfig`).
pub fn load_config_file(path: &Path) -> Result<LoadedRequest, ConfigError> {
    let candidate = if path.is_dir() {
        path.join(DEFAULT_CONFIG_FILENAME)
    } else {
        path.to_path_buf()
    };

    if !candidate.is_file() {
        return Err(ConfigError(format!(
            "No config file at {}.\nCreate a zzop.config.jsonc there, or pass a directory that has one.",
            candidate.display()
        )));
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
    Ok(LoadedRequest {
        method: mapped.method,
        request: mapped.request,
        warnings,
        config_path: Some(candidate),
    })
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
