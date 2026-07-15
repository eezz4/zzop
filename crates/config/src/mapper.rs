//! The config→facade-request mapper — the port of `packages/cli/lib/mapper.js`'s `configToRequest`
//! (plus `collectConfigWarnings`), working on `serde_json::Value` exactly as the JS works on plain
//! objects (no config structs: unknown keys must flow through to the warning walk, not fail serde).
//!
//! Contract anchors (verbatim behaviors the port must keep — see the JS source for the full set):
//! - Severity aliases: off/none/disable/disabled; critical/error/err/high; warning/warn/medium;
//!   info/information/note/low. `"off"` routes to `disabledRules`, everything else to
//!   `severityOverrides` with the engine's lowercase values.
//! - `isGlobPattern` = the exact character class `[*?{}]` — `[`/`]` deliberately excluded so
//!   Next.js-style `app/[locale]/` paths stay substring matches (`path` key), not globs (`glob`).
//! - `trees` wins over `roots` silently when both are set; single `roots` entry with no `trees` key
//!   selects `analyze`, everything else `analyzeTrees` (a single-entry `trees` array included).
//! - `mountedAt`/`mounts`/`hosts` fail-fast validation gates (leading `/`, no scheme, no `{}`
//!   placeholder, no whitespace; `dir` tree-relative, forward slashes; hosts bare, no `://`, no `/`).
//! - Overlay files are read HERE (tree-root-relative), read/parse failures become warnings that skip
//!   the overlay, never errors.
//! - Unknown keys at every scoped level warn (never reject) with the `config-surface.json`
//!   vocabulary (`crate::CONFIG_SURFACE_JSON`).
//! - The `withDefaults` layer folds in here for native hosts: bundled packs are injected as inline
//!   `packDefs` (`crate::BUNDLED_PACK_SOURCES`), and `git: {}` is injected when the config has no
//!   `git` key, so zero-config still collects git signals (30-day default) exactly like the JS CLI.
//! - CLI-presentation keys (`failOn`/`format`/`report.*`) are NOT forwarded into the request.
//!
//! Deliberate deviation (documented in the crate doc): `root`/`cacheDir`/`packsDir` resolve against
//! `base_dir` (the config file's directory) instead of the process cwd.
//!
//! ## Implementation notes beyond the JS source
//! - JS's `collectConfigWarnings` and `configToRequest` are two independently-called functions (the
//!   CLI calls both, and `configToRequest`'s own overlay resolution throws its warnings away since
//!   `collectConfigWarnings` recomputes them). This port threads ONE warnings `Vec` end to end (see
//!   `MappedRequest`), so overlay read/parse warnings are collected exactly once, at the same real
//!   resolution call that builds `adapterOverlays` — not recomputed by a second pass.
//! - `serde_json::Value::Object` iterates its keys in sorted (`BTreeMap`) order, not JS's
//!   source-text insertion order (`serde_json`'s `preserve_order` feature is not enabled here).
//!   This only ever affects the ORDER of generated warning/array entries whose own content is a set
//!   (unknown-key warnings, `disabledRules`), never their content — deep-equal comparisons (as JSON
//!   values, not raw strings) are unaffected either way.
//! - `JSON.stringify(value)` (used in JS's severity error text) is mirrored with
//!   `serde_json::to_string`, which matches byte-for-byte for every primitive JSON value; object/array
//!   key order can differ for the same `BTreeMap`-vs-insertion-order reason above, an irrelevant edge
//!   case for a severity value (which is never itself an object/array in a well-formed config).

use std::path::{Path, PathBuf};

use crate::{ConfigError, Method};

/// The result of mapping one config object: the facade request value (with defaults injected),
/// the method it targets, and every non-fatal warning collected along the way.
#[derive(Debug)]
pub struct MappedRequest {
    pub method: Method,
    pub request: serde_json::Value,
    pub warnings: Vec<String>,
}

/// Maps a parsed config object (post-JSONC, post-`trees:"auto"`-expansion) to a facade request.
/// `base_dir` is the config file's directory — the resolution base for `root`/`cacheDir`/`packsDir`
/// (deviation from JS cwd semantics, see module doc); overlays stay tree-root-relative (JS parity).
pub fn config_to_request(
    config: &serde_json::Value,
    base_dir: &Path,
) -> Result<MappedRequest, ConfigError> {
    use serde_json::{Map, Value};

    if !config.is_object() {
        return Err(ConfigError("Config must be a JSON object.".to_string()));
    }

    let mut warnings = collect_config_warnings(config);

    let shared = build_shared_options(config, base_dir)?;

    // Top-level `overlays` (shared across every tree) — shape-validated here, resolved per tree below.
    let shared_overlay_paths = match config.get("overlays") {
        Some(v) => validate_overlays_array(v, "overlays")?,
        None => Vec::new(),
    };

    let trees_key_present = config.get("trees").is_some();
    let mut trees: Vec<Map<String, Value>> = Vec::new();

    if let Some(trees_value) = config.get("trees") {
        // `trees: "auto"` must already be expanded (by `workspaces::expand_auto_trees`, called from
        // `load_for_root`/`load_config_file` right after parsing) before it reaches this pure mapper.
        // A caller invoking `config_to_request` directly with the raw sentinel gets an actionable
        // error instead of the generic "must be a non-empty array" below.
        if trees_value.as_str() == Some("auto") {
            return Err(ConfigError(
                "trees: \"auto\" must be expanded before config_to_request — call \
                 workspaces::expand_auto_trees(config, base_dir) first (load_for_root/load_config_file \
                 do this automatically)."
                    .to_string(),
            ));
        }

        let arr = trees_value.as_array().filter(|a| !a.is_empty());
        let Some(arr) = arr else {
            return Err(ConfigError(
                "trees, when present, must be a non-empty array of { root, sourceId }.".to_string(),
            ));
        };

        for (i, tree_val) in arr.iter().enumerate() {
            let tree_obj = tree_val.as_object();
            let raw_root = tree_obj
                .and_then(|o| o.get("root"))
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty());
            let Some(raw_root) = raw_root else {
                return Err(ConfigError(format!(
                    "trees[{i}] must be an object with a non-empty \"root\" string."
                )));
            };
            let tree_obj = tree_obj.expect("raw_root extraction requires an object");

            let tree_overlay_paths = match tree_obj.get("overlays") {
                Some(v) => validate_overlays_array(v, &format!("trees[{i}].overlays"))?,
                None => Vec::new(),
            };

            let resolved_root = resolve_path(base_dir, raw_root);
            let source_id = tree_obj
                .get("sourceId")
                .and_then(Value::as_str)
                .unwrap_or(raw_root);

            let mut tree_request = shared.clone();
            tree_request.insert(
                "root".to_string(),
                Value::String(path_to_string(&resolved_root)),
            );
            tree_request.insert("sourceId".to_string(), Value::String(source_id.to_string()));

            let (overlays, overlay_warnings) = resolve_overlays_for_root(
                &resolved_root,
                raw_root,
                &shared_overlay_paths,
                &tree_overlay_paths,
            );
            warnings.extend(overlay_warnings);
            if !overlays.is_empty() {
                tree_request.insert("adapterOverlays".to_string(), Value::Array(overlays));
            }

            // Connection topology — `trees[]` entries only (the `roots` shorthand below never reads
            // these keys at all).
            if let Some(v) = tree_obj.get("mountedAt") {
                let s = validate_mount_at(v, &format!("trees[{i}].mountedAt"))?;
                tree_request.insert("mountedAt".to_string(), Value::String(s));
            }
            if let Some(v) = tree_obj.get("mounts") {
                let arr = validate_mounts_array(v, &format!("trees[{i}].mounts"))?;
                if !arr.is_empty() {
                    tree_request.insert("mounts".to_string(), Value::Array(arr));
                }
            }
            if let Some(v) = tree_obj.get("hosts") {
                let arr = validate_hosts_array(v, &format!("trees[{i}].hosts"))?;
                if !arr.is_empty() {
                    tree_request.insert("hosts".to_string(), Value::Array(arr));
                }
            }

            trees.push(tree_request);
        }
    } else {
        let roots: Vec<String> = match config.get("roots") {
            None => vec![".".to_string()],
            Some(v) => {
                let arr = v.as_array().filter(|a| !a.is_empty());
                let Some(arr) = arr else {
                    return Err(ConfigError(
                        "roots must be a non-empty array of directory paths.".to_string(),
                    ));
                };
                let mut out = Vec::with_capacity(arr.len());
                for r in arr {
                    match r.as_str().filter(|s| !s.is_empty()) {
                        Some(s) => out.push(s.to_string()),
                        None => {
                            return Err(ConfigError(
                                "roots entries must be non-empty strings.".to_string(),
                            ))
                        }
                    }
                }
                out
            }
        };

        // Multiple roots => give each tree a distinct sourceId (its raw root string) so cross-source
        // analysis works; a single root needs no source tag (it takes the single-tree `Analyze` path).
        let multiple = roots.len() > 1;
        for root in &roots {
            let resolved_root = resolve_path(base_dir, root);
            let mut tree_request = shared.clone();
            tree_request.insert(
                "root".to_string(),
                Value::String(path_to_string(&resolved_root)),
            );
            if multiple {
                tree_request.insert("sourceId".to_string(), Value::String(root.clone()));
            }

            let (overlays, overlay_warnings) =
                resolve_overlays_for_root(&resolved_root, root, &shared_overlay_paths, &[]);
            warnings.extend(overlay_warnings);
            if !overlays.is_empty() {
                tree_request.insert("adapterOverlays".to_string(), Value::Array(overlays));
            }

            trees.push(tree_request);
        }
    }

    // `withDefaults` analog (packages/native/index.js): every tree request gets a `git: {}` default
    // when the config named no `git` key at all, and the bundled DSL packs inline as `packDefs` — a
    // native host has no sidecar `rules/` directory to point a `packsDir` string at, so the packs
    // themselves ride inside the request instead.
    let pack_defs = parse_pack_defs(crate::BUNDLED_PACK_SOURCES, &mut warnings);
    for tree in &mut trees {
        tree.entry("git".to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        tree.insert("packDefs".to_string(), Value::Array(pack_defs.clone()));
    }

    let method = if trees.len() == 1 && !trees_key_present {
        Method::Analyze
    } else {
        Method::AnalyzeTrees
    };

    let request = match method {
        Method::Analyze => Value::Object(trees.into_iter().next().expect("exactly one tree")),
        Method::AnalyzeTrees => {
            let mut top = Map::new();
            top.insert(
                "trees".to_string(),
                Value::Array(trees.into_iter().map(Value::Object).collect()),
            );
            Value::Object(top)
        }
    };

    Ok(MappedRequest {
        method,
        request,
        warnings,
    })
}

// ---------------------------------------------------------------------------------------------------
// Path resolution — the documented deviation from JS: `root`/`cacheDir`/`packs.extraDirs` entries are
// resolved to absolute against `base_dir` here (a server host's process cwd is meaningless, unlike a
// CLI's), while overlay paths resolve against each TREE'S resolved root (JS parity, see
// `resolve_overlays_for_root`). Resolution is purely LEXICAL (`.`/`..` segments collapsed against the
// path text, no symlink following, no filesystem access, no existence requirement) — the same
// contract Node's `path.resolve` gives the JS mapper, so a nonexistent `cacheDir` or `packsDir` still
// resolves cleanly (existence is the engine's problem at load time, not this mapper's).
// ---------------------------------------------------------------------------------------------------

/// Resolves `raw` against `base_dir`: absolute inputs are normalized as-is, relative inputs are
/// joined onto `base_dir` first. Assumes `base_dir` is itself already absolute (the caller's
/// responsibility — an MCP host always hands this crate an absolute repo root).
fn resolve_path(base_dir: &Path, raw: &str) -> PathBuf {
    let candidate = Path::new(raw);
    let joined = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        base_dir.join(candidate)
    };
    normalize_lexically(&joined)
}

/// Collapses `.`/`..` path components purely lexically (no filesystem access), mirroring Node's
/// `path.resolve`/`path.normalize` semantics: a `..` pops the previous real (`Normal`) component when
/// there is one to pop, otherwise it is kept (there is nothing left under this path to collapse into,
/// e.g. `..` past a root or past another un-collapsed `..`).
fn normalize_lexically(path: &Path) -> PathBuf {
    use std::path::Component;

    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(result.components().next_back(), Some(Component::Normal(_))) {
                    result.pop();
                } else {
                    result.push("..");
                }
            }
            other => result.push(other.as_os_str()),
        }
    }
    result
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

// ---------------------------------------------------------------------------------------------------
// Severity normalization — the SINGLE source of truth for turning friendly config severities into the
// engine's `Severity` serde values (see `crates/core/src/finding.rs`'s `#[serde(rename_all =
// "lowercase")]` and `crates/facade/src/lib.rs`'s `AnalyzeRequest::severity_overrides`).
// ---------------------------------------------------------------------------------------------------

/// A normalized severity: either the `"off"` sentinel (routes to `disabledRules`) or one of the three
/// engine severity strings (routes to `severityOverrides`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SeverityValue {
    Off,
    Engine(&'static str),
}

/// Friendly alias -> normalized severity, in the exact order the JS `SEVERITY_ALIASES` object
/// literal declares them — this order is load-bearing: it is reproduced verbatim in the "Expected one
/// of: ..." error text below.
const SEVERITY_ALIASES: &[(&str, SeverityValue)] = &[
    ("off", SeverityValue::Off),
    ("none", SeverityValue::Off),
    ("disable", SeverityValue::Off),
    ("disabled", SeverityValue::Off),
    ("critical", SeverityValue::Engine("critical")),
    ("error", SeverityValue::Engine("critical")),
    ("err", SeverityValue::Engine("critical")),
    ("high", SeverityValue::Engine("critical")),
    ("warning", SeverityValue::Engine("warning")),
    ("warn", SeverityValue::Engine("warning")),
    ("medium", SeverityValue::Engine("warning")),
    ("info", SeverityValue::Engine("info")),
    ("information", SeverityValue::Engine("info")),
    ("note", SeverityValue::Engine("info")),
    ("low", SeverityValue::Engine("info")),
];

/// Normalizes a friendly severity value (trimmed, case-insensitive) to `SeverityValue`, or a
/// `ConfigError` for a non-string value or an unrecognized alias. `context` (typically a rule id)
/// appends ` for "<context>"` to either error, matching `normalizeSeverity`'s JS text exactly.
fn normalize_severity(
    value: &serde_json::Value,
    context: Option<&str>,
) -> Result<SeverityValue, ConfigError> {
    let context_suffix = |c: Option<&str>| c.map(|c| format!(" for \"{c}\"")).unwrap_or_default();

    let Some(s) = value.as_str() else {
        return Err(ConfigError(format!(
            "Invalid severity {}{}: expected a string.",
            json_stringify(value),
            context_suffix(context)
        )));
    };
    let key = s.trim().to_lowercase();
    match SEVERITY_ALIASES.iter().find(|(alias, _)| *alias == key) {
        Some((_, sev)) => Ok(*sev),
        None => {
            let valid = SEVERITY_ALIASES
                .iter()
                .map(|(alias, _)| *alias)
                .collect::<Vec<_>>()
                .join(", ");
            Err(ConfigError(format!(
                "Unknown severity {}{}. Expected one of: {valid}.",
                json_stringify(value),
                context_suffix(context)
            )))
        }
    }
}

/// `JSON.stringify(value)` equivalent for a severity error's offending value — matches byte-for-byte
/// for every JSON primitive (strings, numbers, booleans, null), which covers every value a config
/// author could plausibly write for a severity field.
fn json_stringify(value: &serde_json::Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}

/// A `rules[].exclude`/top-level `exclude` entry is a glob (full-path, anchored, engine-side) when it
/// carries a glob metacharacter; otherwise it is a plain substring filter. `[`/`]` are deliberately
/// NOT glob characters so raw Next.js dynamic-segment paths like `app/[locale]/` stay substring
/// matches instead of being (mis)parsed as a character class.
fn is_glob_pattern(s: &str) -> bool {
    s.chars().any(|c| matches!(c, '*' | '?' | '{' | '}'))
}

// ---------------------------------------------------------------------------------------------------
// Adapter overlays — see the crate-level doc's `withDefaults`/overlay summary and `mapper.js`'s own
// "Adapter overlays" section for the full rationale. Read/parse failures are NEVER fatal: they are
// pushed onto the shared warnings `Vec` and the offending overlay is skipped.
// ---------------------------------------------------------------------------------------------------

/// Validates an `overlays` array's SHAPE (must be an array of non-empty strings) and extracts it as
/// owned `String`s. A shape violation is a config-authoring mistake (like any other mistyped array
/// field in this module) and fails fast with a `ConfigError` — unlike a per-file read/parse failure,
/// handled separately by `resolve_overlays_for_root`, which never throws.
fn validate_overlays_array(
    value: &serde_json::Value,
    label: &str,
) -> Result<Vec<String>, ConfigError> {
    let arr = value
        .as_array()
        .ok_or_else(|| ConfigError(format!("{label} must be an array of file paths.")))?;
    let mut out = Vec::with_capacity(arr.len());
    for entry in arr {
        match entry.as_str().filter(|s| !s.is_empty()) {
            Some(s) => out.push(s.to_string()),
            None => {
                return Err(ConfigError(format!(
                    "{label} entries must be non-empty strings (paths to overlay JSON files)."
                )))
            }
        }
    }
    Ok(out)
}

/// Reads and parses every overlay path (shared/top-level paths, then this tree's own), resolved
/// against `resolved_root` (the tree's OWN resolved root — matching JS's "resolve relative to the
/// tree root, not the config file's directory" rule), into `NormalizedEnvelope`-shaped JSON values.
/// Never fails: an unreadable or non-JSON file is dropped with a warning naming the configured path
/// (`root_label` — the tree's RAW, pre-resolution root string, for a human-readable message), the
/// resolved absolute path, and the underlying error.
fn resolve_overlays_for_root(
    resolved_root: &Path,
    root_label: &str,
    shared_paths: &[String],
    tree_paths: &[String],
) -> (Vec<serde_json::Value>, Vec<String>) {
    let mut overlays = Vec::new();
    let mut warnings = Vec::new();

    for overlay_path in shared_paths.iter().chain(tree_paths.iter()) {
        let resolved = resolve_path(resolved_root, overlay_path);
        let raw = match std::fs::read_to_string(&resolved) {
            Ok(raw) => raw,
            Err(err) => {
                // `io_error_label`, not `{err}`: io::Error's Display renders in the OS UI language
                // on Windows (locale-dependent, non-deterministic across hosts).
                warnings.push(format!(
                    "overlay \"{overlay_path}\" for tree \"{root_label}\" (resolved to {}) could not be read: {}. This overlay is skipped.",
                    resolved.display(),
                    crate::io_error_label(&err)
                ));
                continue;
            }
        };
        match serde_json::from_str::<serde_json::Value>(&raw) {
            Ok(v) => overlays.push(v),
            Err(err) => warnings.push(format!(
                "overlay \"{overlay_path}\" for tree \"{root_label}\" (resolved to {}) is not valid JSON: {err}. This overlay is skipped.",
                resolved.display()
            )),
        }
    }

    (overlays, warnings)
}

// ---------------------------------------------------------------------------------------------------
// Connection topology — `mountedAt`/`mounts`/`hosts`, `trees[]` entries only. This module is the
// authoritative fail-fast gate for shape (the engine's own `apply_config_mounts` only defensively
// warns and skips a malformed mount as a last-resort backstop).
// ---------------------------------------------------------------------------------------------------

/// Validates one mount "at" value: a string, non-empty after trimming leading/trailing `/`, starting
/// with `/`, with no scheme separator (`://`), path-param placeholder (`{}`), or whitespace.
fn validate_mount_at(value: &serde_json::Value, label: &str) -> Result<String, ConfigError> {
    let s = value
        .as_str()
        .ok_or_else(|| ConfigError(format!("{label} must be a string.")))?;
    let trimmed_slashes = s.trim_matches('/');
    if trimmed_slashes.is_empty() {
        return Err(ConfigError(format!(
            "{label} must be a non-empty path after trimming slashes."
        )));
    }
    if !s.starts_with('/') {
        return Err(ConfigError(format!("{label} must start with \"/\".")));
    }
    if s.contains("://") {
        return Err(ConfigError(format!(
            "{label} must not contain a scheme (\"://\") — it is a path prefix, not a full URL."
        )));
    }
    if s.contains("{}") {
        return Err(ConfigError(format!(
            "{label} must not contain a path-param placeholder (\"{{}}\")."
        )));
    }
    if s.chars().any(char::is_whitespace) {
        return Err(ConfigError(format!("{label} must not contain whitespace.")));
    }
    Ok(s.to_string())
}

/// Validates one `mounts[].dir` value: a string, tree-relative (must not start with `/`), forward
/// slashes only (must not contain a backslash).
fn validate_mount_dir(value: &serde_json::Value, label: &str) -> Result<(), ConfigError> {
    let s = value
        .as_str()
        .ok_or_else(|| ConfigError(format!("{label} must be a string.")))?;
    if s.starts_with('/') {
        return Err(ConfigError(format!(
            "{label} must be tree-relative and must not start with \"/\"."
        )));
    }
    if s.contains('\\') {
        return Err(ConfigError(format!(
            "{label} must use forward slashes, not backslashes."
        )));
    }
    Ok(())
}

/// Validates a `mounts` array's shape (`[{dir, at}, ...]`) and returns it unchanged (the ORIGINAL
/// `Value`s, including any extra keys — those surface separately via `collect_config_warnings`'s
/// `mount` scope, not stripped here).
fn validate_mounts_array(
    value: &serde_json::Value,
    label: &str,
) -> Result<Vec<serde_json::Value>, ConfigError> {
    let arr = value.as_array().ok_or_else(|| {
        ConfigError(format!(
            "{label} must be an array of {{ dir, at }} objects."
        ))
    })?;
    for (i, entry) in arr.iter().enumerate() {
        if !entry.is_object() {
            return Err(ConfigError(format!(
                "{label}[{i}] must be an object with \"dir\" and \"at\" strings."
            )));
        }
        let dir = entry.get("dir").cloned().unwrap_or(serde_json::Value::Null);
        validate_mount_dir(&dir, &format!("{label}[{i}].dir"))?;
        let at = entry.get("at").cloned().unwrap_or(serde_json::Value::Null);
        validate_mount_at(&at, &format!("{label}[{i}].at"))?;
    }
    Ok(arr.clone())
}

/// Validates a `hosts` array's shape: non-empty bare-host strings — no scheme (`://`, checked first
/// since every `://` value also contains `/`, so a URL gets the URL-specific message), no path
/// separator (`/`), no whitespace.
fn validate_hosts_array(
    value: &serde_json::Value,
    label: &str,
) -> Result<Vec<serde_json::Value>, ConfigError> {
    let arr = value
        .as_array()
        .ok_or_else(|| ConfigError(format!("{label} must be an array of host strings.")))?;
    for (i, entry) in arr.iter().enumerate() {
        let s = entry
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ConfigError(format!("{label}[{i}] must be a non-empty string.")))?;
        if s.contains("://") {
            return Err(ConfigError(format!(
                "{label}[{i}] must be a bare host, not a full URL (\"://\" is not allowed)."
            )));
        }
        if s.contains('/') {
            return Err(ConfigError(format!(
                "{label}[{i}] must be a bare host, not a path (\"/\" is not allowed)."
            )));
        }
        if s.chars().any(char::is_whitespace) {
            return Err(ConfigError(format!(
                "{label}[{i}] must not contain whitespace."
            )));
        }
    }
    Ok(arr.clone())
}

// ---------------------------------------------------------------------------------------------------
// Shared per-config options — the rule/pack/git/cache knobs that are global to the config (not
// per-tree), merged into every tree request. Only fields actually set are returned, so an omitted
// config key falls through to the facade/engine default.
// ---------------------------------------------------------------------------------------------------

/// A JSON value is "falsy" exactly like JS's `||` operator would treat it: `null`, `false`, `0`
/// (any numeric zero), or `""`. Used ONLY where the JS source itself relies on `x || defaultValue`
/// (today: `config.rules`) — every other field in this module is presence-gated with `!== undefined`,
/// which `serde_json::Value::get` (`Option::is_some`) already matches directly.
fn is_json_falsy(v: &serde_json::Value) -> bool {
    match v {
        serde_json::Value::Null => true,
        serde_json::Value::Bool(b) => !*b,
        serde_json::Value::Number(n) => n.as_f64() == Some(0.0),
        serde_json::Value::String(s) => s.is_empty(),
        _ => false,
    }
}

fn build_shared_options(
    config: &serde_json::Value,
    base_dir: &Path,
) -> Result<serde_json::Map<String, serde_json::Value>, ConfigError> {
    use serde_json::{Map, Value};

    let mut shared = Map::new();
    let packs_obj = config.get("packs").and_then(Value::as_object);

    // --- packs.extraDirs -> packsDir (user dirs only; the bundled packs ride separately as inline
    // `packDefs`, injected later by the `withDefaults` step at the end of `config_to_request`). ---
    if let Some(packs) = packs_obj {
        if let Some(extra_dirs) = packs.get("extraDirs") {
            let arr = extra_dirs.as_array().ok_or_else(|| {
                ConfigError("packs.extraDirs must be an array of directory paths.".to_string())
            })?;
            let mut resolved = Vec::new();
            for entry in arr {
                if entry.as_str() == Some("") {
                    continue; // filter empty strings (JS parity)
                }
                match entry.as_str() {
                    Some(s) => {
                        resolved.push(Value::String(path_to_string(&resolve_path(base_dir, s))))
                    }
                    // Non-string entries are not type-checked by the JS source either — pass through
                    // verbatim rather than rejecting a shape the original mapper silently tolerated.
                    None => resolved.push(entry.clone()),
                }
            }
            if !resolved.is_empty() {
                shared.insert("packsDir".to_string(), Value::Array(resolved));
            }
        }

        if let Some(disabled) = packs.get("disabled") {
            if !disabled.is_array() {
                return Err(ConfigError(
                    "packs.disabled must be an array of pack ids.".to_string(),
                ));
            }
        }
    }

    // --- disabledRules: whole disabled packs (insertion-order dedup, mirroring a JS `Set`) + any
    // rule set to severity "off". ---
    let mut disabled: Vec<Value> = Vec::new();
    if let Some(Some(Value::Array(arr))) = packs_obj.map(|p| p.get("disabled")) {
        for entry in arr {
            if !disabled.contains(entry) {
                disabled.push(entry.clone());
            }
        }
    }

    // --- rules.<id> -> severityOverrides / suppressions / disabledRules. `config.rules || {}` in JS:
    // an absent OR falsy `rules` defaults to empty (no error); a present, truthy, non-object `rules`
    // (e.g. an array) is a shape error. ---
    let rules_obj = match config.get("rules") {
        None => None,
        Some(v) if is_json_falsy(v) => None,
        Some(Value::Object(m)) => Some(m),
        Some(_) => {
            return Err(ConfigError(
                "rules must be an object mapping rule ids to a severity or a rule object."
                    .to_string(),
            ))
        }
    };

    let mut severity_overrides = Map::new();
    let mut suppressions: Vec<Value> = Vec::new();

    if let Some(rules) = rules_obj {
        for (rule_id, entry) in rules {
            match entry {
                Value::String(_) => {
                    apply_severity(entry, rule_id, &mut disabled, &mut severity_overrides)?;
                }
                Value::Object(entry_obj) => {
                    if let Some(sev_val) = entry_obj.get("severity") {
                        apply_severity(sev_val, rule_id, &mut disabled, &mut severity_overrides)?;
                    }
                    if let Some(exclude_val) = entry_obj.get("exclude") {
                        let arr = exclude_val.as_array().ok_or_else(|| {
                            ConfigError(format!(
                                "rules.{rule_id}.exclude must be an array of path substrings or globs."
                            ))
                        })?;
                        for path_val in arr {
                            let path_str = path_val.as_str().ok_or_else(|| {
                                ConfigError(format!("rules.{rule_id}.exclude entries must be strings."))
                            })?;
                            suppressions.push(suppression_entry(rule_id, path_str));
                        }
                    }
                }
                _ => {
                    return Err(ConfigError(format!(
                        "rules.{rule_id} must be a severity string (e.g. \"warn\"/\"off\") or an object \
                         ({{ \"severity\": ..., \"exclude\": [...] }})."
                    )))
                }
            }
        }
    }

    if !disabled.is_empty() {
        shared.insert("disabledRules".to_string(), Value::Array(disabled));
    }
    if !severity_overrides.is_empty() {
        shared.insert(
            "severityOverrides".to_string(),
            Value::Object(severity_overrides),
        );
    }
    if !suppressions.is_empty() {
        shared.insert("suppressions".to_string(), Value::Array(suppressions));
    }

    // --- top-level exclude -> globalExcludes (rule-agnostic finding-level filter). ---
    if let Some(exclude_val) = config.get("exclude") {
        let arr = exclude_val.as_array().ok_or_else(|| {
            ConfigError("exclude must be an array of path substrings or globs.".to_string())
        })?;
        let mut global_excludes = Vec::new();
        for path_val in arr {
            let path_str = path_val
                .as_str()
                .ok_or_else(|| ConfigError("exclude entries must be strings.".to_string()))?;
            global_excludes.push(exclude_entry(path_str));
        }
        if !global_excludes.is_empty() {
            shared.insert("globalExcludes".to_string(), Value::Array(global_excludes));
        }
    }

    // --- pass-through knobs. `cacheDir` is resolved against `base_dir` like `root`/`packsDir` (the
    // documented deviation); `git`/`sizeCap` pass through untouched (no path-shaped content). ---
    if let Some(git_val) = config.get("git") {
        shared.insert("git".to_string(), git_val.clone());
    }
    if let Some(cache_dir_val) = config.get("cacheDir") {
        let resolved = match cache_dir_val.as_str() {
            Some(s) => Value::String(path_to_string(&resolve_path(base_dir, s))),
            None => cache_dir_val.clone(),
        };
        shared.insert("cacheDir".to_string(), resolved);
    }
    if let Some(size_cap_val) = config.get("sizeCap") {
        shared.insert("sizeCap".to_string(), size_cap_val.clone());
    }

    Ok(shared)
}

/// Shared `rules.<id>` / `rules.<id>.severity` handling: normalizes `sev_value`, then either records
/// `rule_id` as disabled (`"off"`) or as a `severityOverrides` entry — the exact same routing whether
/// the config wrote a bare severity string or `{severity: ...}`.
fn apply_severity(
    sev_value: &serde_json::Value,
    rule_id: &str,
    disabled: &mut Vec<serde_json::Value>,
    severity_overrides: &mut serde_json::Map<String, serde_json::Value>,
) -> Result<(), ConfigError> {
    match normalize_severity(sev_value, Some(rule_id))? {
        SeverityValue::Off => {
            let v = serde_json::Value::String(rule_id.to_string());
            if !disabled.contains(&v) {
                disabled.push(v);
            }
        }
        SeverityValue::Engine(engine) => {
            severity_overrides.insert(
                rule_id.to_string(),
                serde_json::Value::String(engine.to_string()),
            );
        }
    }
    Ok(())
}

fn suppression_entry(rule_id: &str, path_or_glob: &str) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    m.insert(
        "rule".to_string(),
        serde_json::Value::String(rule_id.to_string()),
    );
    if is_glob_pattern(path_or_glob) {
        m.insert(
            "glob".to_string(),
            serde_json::Value::String(path_or_glob.to_string()),
        );
    } else {
        m.insert(
            "path".to_string(),
            serde_json::Value::String(path_or_glob.to_string()),
        );
    }
    serde_json::Value::Object(m)
}

fn exclude_entry(path_or_glob: &str) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    if is_glob_pattern(path_or_glob) {
        m.insert(
            "glob".to_string(),
            serde_json::Value::String(path_or_glob.to_string()),
        );
    } else {
        m.insert(
            "path".to_string(),
            serde_json::Value::String(path_or_glob.to_string()),
        );
    }
    serde_json::Value::Object(m)
}

// ---------------------------------------------------------------------------------------------------
// Unknown-key warnings — the port of `collectConfigWarnings`'s scoped walk. Never rejects (the engine
// deliberately ignores unknown fields); this only makes a typo or cross-version drift visible.
// Vocabulary sourced from `crate::CONFIG_SURFACE_JSON`'s `configKeys` — the same vocabulary file the
// JS CLI and the engine's own reference-validation meta-test share, so this port can never disagree
// with either about what a valid config key is.
// ---------------------------------------------------------------------------------------------------

fn collect_config_warnings(config: &serde_json::Value) -> Vec<String> {
    let mut warnings = Vec::new();
    if !config.is_object() {
        return warnings;
    }

    let surface: serde_json::Value = serde_json::from_str(crate::CONFIG_SURFACE_JSON)
        .expect("embedded config-surface.json must be valid JSON");
    let config_keys = &surface["configKeys"];
    let known = |scope: &str| -> Vec<&str> {
        config_keys[scope]
            .as_array()
            .map(|a| a.iter().filter_map(serde_json::Value::as_str).collect())
            .unwrap_or_default()
    };

    warn_unknown_keys(Some(config), &known("top"), "", &mut warnings);
    warn_unknown_keys(
        config.get("packs"),
        &known("packs"),
        "packs.",
        &mut warnings,
    );
    warn_unknown_keys(config.get("git"), &known("git"), "git.", &mut warnings);
    warn_unknown_keys(
        config.get("report"),
        &known("report"),
        "report.",
        &mut warnings,
    );

    if let Some(trees) = config.get("trees").and_then(serde_json::Value::as_array) {
        let known_tree = known("tree");
        let known_mount = known("mount");
        for (i, tree) in trees.iter().enumerate() {
            warn_unknown_keys(
                Some(tree),
                &known_tree,
                &format!("trees[{i}]."),
                &mut warnings,
            );
            if let Some(mounts) = tree.get("mounts").and_then(serde_json::Value::as_array) {
                for (j, entry) in mounts.iter().enumerate() {
                    if entry.is_object() {
                        warn_unknown_keys(
                            Some(entry),
                            &known_mount,
                            &format!("trees[{i}].mounts[{j}]."),
                            &mut warnings,
                        );
                    }
                }
            }
        }
    }

    if let Some(rules) = config.get("rules").and_then(serde_json::Value::as_object) {
        let known_rule_object = known("ruleObject");
        for (rule_id, entry) in rules {
            if entry.is_object() {
                warn_unknown_keys(
                    Some(entry),
                    &known_rule_object,
                    &format!("rules.{rule_id}."),
                    &mut warnings,
                );
            }
        }
    }

    warnings
}

/// One scope of `collectConfigWarnings`'s walk: for every key in `obj` (a no-op if `obj` is absent or
/// not itself a JSON object) not present in `known`, push an "unknown config key" warning naming the
/// full dotted key, the scope, and the known-keys list for that scope — verbatim text match with the
/// JS source, including its `${scope}${key}` composition and the `scope.replace(/\.$/, '')` trim
/// (`scope` here always carries at most one trailing `.`, so `trim_end_matches('.')` is equivalent).
fn warn_unknown_keys(
    obj: Option<&serde_json::Value>,
    known: &[&str],
    scope: &str,
    warnings: &mut Vec<String>,
) {
    let Some(map) = obj.and_then(serde_json::Value::as_object) else {
        return;
    };
    for key in map.keys() {
        if !known.contains(&key.as_str()) {
            let where_ = if scope.is_empty() {
                "at the top level".to_string()
            } else {
                format!("under \"{}\"", scope.trim_end_matches('.'))
            };
            warnings.push(format!(
                "unknown config key \"{scope}{key}\" (ignored) — a typo, or a key from a different zzop \
                 version. Known keys {where_}: {}.",
                known.join(", ")
            ));
        }
    }
}

// ---------------------------------------------------------------------------------------------------
// Bundled packs — the `withDefaults` layer's pack-injection half. `sources` is always
// `crate::BUNDLED_PACK_SOURCES` in production; parameterized so a fabricated bad source can exercise
// the skip-on-parse-failure path in tests without depending on a real pack ever going invalid.
// ---------------------------------------------------------------------------------------------------

fn parse_pack_defs(sources: &[(&str, &str)], warnings: &mut Vec<String>) -> Vec<serde_json::Value> {
    let mut out = Vec::with_capacity(sources.len());
    for (rel_path, source) in sources {
        match serde_json::from_str::<serde_json::Value>(source) {
            Ok(v) => out.push(v),
            Err(err) => warnings.push(format!(
                "bundled pack \"{rel_path}\" failed to parse and was skipped: {err}."
            )),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempDir;
    use serde_json::json;

    fn analyze_request(v: &serde_json::Value) -> &serde_json::Map<String, serde_json::Value> {
        v.as_object().expect("Analyze request must be an object")
    }

    // --- severity aliases ------------------------------------------------------------------------

    #[test]
    fn severity_aliases_cover_every_documented_bucket() {
        for off in ["off", "none", "disable", "disabled", "OFF", " Off "] {
            assert_eq!(
                normalize_severity(&json!(off), None).unwrap(),
                SeverityValue::Off
            );
        }
        for critical in ["critical", "error", "err", "high", "CRITICAL", " Error "] {
            assert_eq!(
                normalize_severity(&json!(critical), None).unwrap(),
                SeverityValue::Engine("critical")
            );
        }
        for warning in ["warning", "warn", "medium", "WARN"] {
            assert_eq!(
                normalize_severity(&json!(warning), None).unwrap(),
                SeverityValue::Engine("warning")
            );
        }
        for info in ["info", "information", "note", "low", "INFO"] {
            assert_eq!(
                normalize_severity(&json!(info), None).unwrap(),
                SeverityValue::Engine("info")
            );
        }
    }

    #[test]
    fn unknown_severity_error_text_lists_every_alias() {
        let err = normalize_severity(&json!("bogus"), Some("circular")).unwrap_err();
        assert_eq!(
            err.0,
            "Unknown severity \"bogus\" for \"circular\". Expected one of: off, none, disable, disabled, \
             critical, error, err, high, warning, warn, medium, info, information, note, low."
        );
    }

    #[test]
    fn non_string_severity_error_text_matches_js() {
        let err = normalize_severity(&json!(5), Some("circular")).unwrap_err();
        assert_eq!(
            err.0,
            "Invalid severity 5 for \"circular\": expected a string."
        );
    }

    #[test]
    fn severity_off_routes_a_rule_into_disabled_rules() {
        let mapped = config_to_request(
            &json!({"roots": ["."], "rules": {"toctou": "off"}}),
            Path::new("/base"),
        )
        .unwrap();
        let req = analyze_request(&mapped.request);
        let disabled = req["disabledRules"].as_array().unwrap();
        assert!(disabled.iter().any(|v| v == "toctou"));
        assert!(req.get("severityOverrides").is_none());
    }

    #[test]
    fn severity_object_form_off_also_routes_to_disabled_rules() {
        let mapped = config_to_request(
            &json!({"roots": ["."], "rules": {"toctou": {"severity": "off"}}}),
            Path::new("/base"),
        )
        .unwrap();
        let req = analyze_request(&mapped.request);
        assert!(req["disabledRules"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "toctou"));
    }

    #[test]
    fn severity_warn_becomes_an_engine_severity_override() {
        let mapped = config_to_request(
            &json!({"roots": ["."], "rules": {"n-plus-one": "warn"}}),
            Path::new("/base"),
        )
        .unwrap();
        let req = analyze_request(&mapped.request);
        assert_eq!(req["severityOverrides"]["n-plus-one"], "warning");
    }

    // --- glob vs. substring split ------------------------------------------------------------------

    #[test]
    fn bracketed_dynamic_segment_stays_a_substring_path_not_a_glob() {
        assert!(!is_glob_pattern("app/[locale]/page.tsx"));
        let mapped = config_to_request(
            &json!({"roots": ["."], "exclude": ["app/[locale]/"]}),
            Path::new("/base"),
        )
        .unwrap();
        let req = analyze_request(&mapped.request);
        let excludes = req["globalExcludes"].as_array().unwrap();
        assert_eq!(excludes[0]["path"], "app/[locale]/");
        assert!(excludes[0].get("glob").is_none());
    }

    #[test]
    fn star_and_brace_patterns_are_treated_as_globs() {
        for pattern in ["**/*.spec.ts", "src/{a,b}.ts", "file?.ts"] {
            assert!(
                is_glob_pattern(pattern),
                "{pattern} should be detected as a glob"
            );
        }
        let mapped = config_to_request(
            &json!({"roots": ["."], "exclude": ["**/*.spec.ts"]}),
            Path::new("/base"),
        )
        .unwrap();
        let req = analyze_request(&mapped.request);
        assert_eq!(req["globalExcludes"][0]["glob"], "**/*.spec.ts");
    }

    #[test]
    fn rules_exclude_entries_split_glob_vs_substring_independently() {
        let mapped = config_to_request(
            &json!({"roots": ["."], "rules": {"toctou": {"exclude": ["legacy/", "**/*.gen.ts"]}}}),
            Path::new("/base"),
        )
        .unwrap();
        let req = analyze_request(&mapped.request);
        let suppressions = req["suppressions"].as_array().unwrap();
        assert_eq!(suppressions.len(), 2);
        assert_eq!(suppressions[0]["path"], "legacy/");
        assert_eq!(suppressions[1]["glob"], "**/*.gen.ts");
        for s in suppressions {
            assert_eq!(s["rule"], "toctou");
        }
    }

    // --- method selection / sourceId -----------------------------------------------------------

    #[test]
    fn single_root_selects_analyze_with_no_source_id() {
        let mapped = config_to_request(&json!({"roots": ["."]}), Path::new("/base")).unwrap();
        assert_eq!(mapped.method, Method::Analyze);
        let req = analyze_request(&mapped.request);
        assert!(req.get("sourceId").is_none());
    }

    #[test]
    fn default_config_with_no_roots_key_also_selects_analyze() {
        let base = Path::new("/base");
        let mapped = config_to_request(&json!({}), base).unwrap();
        assert_eq!(mapped.method, Method::Analyze);
        let req = analyze_request(&mapped.request);
        // Compare against the same lexical-resolution helper the mapper itself uses (rather than a
        // hand-written literal) since `resolve_path` rebuilds the path via `PathBuf::push`, which can
        // normalize separators (e.g. to `\` on Windows) relative to the raw input string.
        assert_eq!(req["root"], path_to_string(&resolve_path(base, ".")));
    }

    #[test]
    fn multiple_roots_select_analyze_trees_and_each_gets_a_raw_source_id() {
        let mapped =
            config_to_request(&json!({"roots": ["./a", "./b"]}), Path::new("/base")).unwrap();
        assert_eq!(mapped.method, Method::AnalyzeTrees);
        let trees = mapped.request["trees"].as_array().unwrap();
        assert_eq!(trees.len(), 2);
        assert_eq!(trees[0]["sourceId"], "./a");
        assert_eq!(trees[1]["sourceId"], "./b");
    }

    #[test]
    fn single_entry_trees_array_still_selects_analyze_trees() {
        let mapped =
            config_to_request(&json!({"trees": [{"root": "./api"}]}), Path::new("/base")).unwrap();
        assert_eq!(mapped.method, Method::AnalyzeTrees);
        assert_eq!(mapped.request["trees"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn tree_source_id_defaults_to_the_raw_configured_root_string() {
        let mapped = config_to_request(
            &json!({"trees": [{"root": "./api"}, {"root": "./web"}]}),
            Path::new("/base"),
        )
        .unwrap();
        let trees = mapped.request["trees"].as_array().unwrap();
        assert_eq!(trees[0]["sourceId"], "./api");
        assert_eq!(trees[1]["sourceId"], "./web");
        // The resolved `root` field, unlike `sourceId`, is absolute.
        assert_ne!(trees[0]["root"], "./api");
    }

    #[test]
    fn explicit_source_id_overrides_the_root_default() {
        let mapped = config_to_request(
            &json!({"trees": [{"root": "./api", "sourceId": "api"}]}),
            Path::new("/base"),
        )
        .unwrap();
        assert_eq!(mapped.request["trees"][0]["sourceId"], "api");
    }

    #[test]
    fn trees_wins_over_roots_silently_when_both_are_present() {
        let mapped = config_to_request(
            &json!({"roots": ["./ignored"], "trees": [{"root": "./api"}]}),
            Path::new("/base"),
        )
        .unwrap();
        assert_eq!(mapped.method, Method::AnalyzeTrees);
        let trees = mapped.request["trees"].as_array().unwrap();
        assert_eq!(trees.len(), 1);
        assert_eq!(trees[0]["sourceId"], "./api");
    }

    #[test]
    fn trees_auto_unexpanded_is_rejected_with_a_pointer_to_expand_auto_trees() {
        let err = config_to_request(&json!({"trees": "auto"}), Path::new("/base")).unwrap_err();
        assert!(err.0.contains("workspaces::expand_auto_trees"));
    }

    // --- validation gate error texts (verbatim) -----------------------------------------------

    #[test]
    fn top_level_config_must_be_a_json_object() {
        let err = config_to_request(&json!(null), Path::new("/base")).unwrap_err();
        assert_eq!(err.0, "Config must be a JSON object.");
        let err = config_to_request(&json!([1, 2]), Path::new("/base")).unwrap_err();
        assert_eq!(err.0, "Config must be a JSON object.");
    }

    #[test]
    fn rules_must_be_an_object() {
        let err = config_to_request(&json!({"rules": []}), Path::new("/base")).unwrap_err();
        assert_eq!(
            err.0,
            "rules must be an object mapping rule ids to a severity or a rule object."
        );
    }

    #[test]
    fn falsy_rules_values_are_treated_as_absent_not_an_error() {
        for v in [json!(null), json!(false), json!(0), json!("")] {
            let mapped =
                config_to_request(&json!({"roots": ["."], "rules": v}), Path::new("/base"))
                    .unwrap();
            let req = analyze_request(&mapped.request);
            assert!(req.get("severityOverrides").is_none());
        }
    }

    #[test]
    fn roots_shape_errors_match_js_text() {
        let err = config_to_request(&json!({"roots": []}), Path::new("/base")).unwrap_err();
        assert_eq!(err.0, "roots must be a non-empty array of directory paths.");
        let err = config_to_request(&json!({"roots": [""]}), Path::new("/base")).unwrap_err();
        assert_eq!(err.0, "roots entries must be non-empty strings.");
    }

    #[test]
    fn trees_shape_errors_match_js_text() {
        let err = config_to_request(&json!({"trees": []}), Path::new("/base")).unwrap_err();
        assert_eq!(
            err.0,
            "trees, when present, must be a non-empty array of { root, sourceId }."
        );
        let err = config_to_request(&json!({"trees": [{"sourceId": "x"}]}), Path::new("/base"))
            .unwrap_err();
        assert_eq!(
            err.0,
            "trees[0] must be an object with a non-empty \"root\" string."
        );
    }

    #[test]
    fn packs_extra_dirs_must_be_an_array() {
        let err = config_to_request(&json!({"packs": {"extraDirs": "x"}}), Path::new("/base"))
            .unwrap_err();
        assert_eq!(
            err.0,
            "packs.extraDirs must be an array of directory paths."
        );
    }

    #[test]
    fn exclude_shape_errors_match_js_text() {
        let err = config_to_request(
            &json!({"roots": ["."], "exclude": "legacy/"}),
            Path::new("/base"),
        )
        .unwrap_err();
        assert_eq!(
            err.0,
            "exclude must be an array of path substrings or globs."
        );
        let err = config_to_request(
            &json!({"roots": ["."], "exclude": [123]}),
            Path::new("/base"),
        )
        .unwrap_err();
        assert_eq!(err.0, "exclude entries must be strings.");
    }

    #[test]
    fn rules_exclude_shape_errors_match_js_text() {
        let err = config_to_request(
            &json!({"rules": {"toctou": {"exclude": "legacy/"}}}),
            Path::new("/base"),
        )
        .unwrap_err();
        assert_eq!(
            err.0,
            "rules.toctou.exclude must be an array of path substrings or globs."
        );
    }

    // --- mountedAt / mounts / hosts gates -------------------------------------------------------

    #[test]
    fn mounted_at_gate_error_texts() {
        let run = |v: serde_json::Value| {
            config_to_request(
                &json!({"trees": [{"root": ".", "mountedAt": v}]}),
                Path::new("/base"),
            )
            .unwrap_err()
            .0
        };
        assert_eq!(run(json!(5)), "trees[0].mountedAt must be a string.");
        assert_eq!(
            run(json!("///")),
            "trees[0].mountedAt must be a non-empty path after trimming slashes."
        );
        assert_eq!(
            run(json!("api")),
            "trees[0].mountedAt must start with \"/\"."
        );
        // The leading-"/" check runs BEFORE the scheme check (same order as the JS source), so a bare
        // "https://api" (which does not start with "/") trips the "/" message, not the scheme one —
        // the scheme check only fires for a value that already starts with "/".
        assert_eq!(
            run(json!("/gateway://oops")),
            "trees[0].mountedAt must not contain a scheme (\"://\") — it is a path prefix, not a full URL."
        );
        assert_eq!(
            run(json!("/api/{}")),
            "trees[0].mountedAt must not contain a path-param placeholder (\"{}\")."
        );
        assert_eq!(
            run(json!("/a b")),
            "trees[0].mountedAt must not contain whitespace."
        );
    }

    #[test]
    fn mounts_gate_error_texts() {
        let run = |v: serde_json::Value| {
            config_to_request(
                &json!({"trees": [{"root": ".", "mounts": v}]}),
                Path::new("/base"),
            )
            .unwrap_err()
            .0
        };
        assert_eq!(
            run(json!("x")),
            "trees[0].mounts must be an array of { dir, at } objects."
        );
        assert_eq!(
            run(json!(["x"])),
            "trees[0].mounts[0] must be an object with \"dir\" and \"at\" strings."
        );
        assert_eq!(
            run(json!([{"dir": "/abs", "at": "/api"}])),
            "trees[0].mounts[0].dir must be tree-relative and must not start with \"/\"."
        );
        assert_eq!(
            run(json!([{"dir": "a\\b", "at": "/api"}])),
            "trees[0].mounts[0].dir must use forward slashes, not backslashes."
        );
    }

    #[test]
    fn hosts_gate_error_texts() {
        let run = |v: serde_json::Value| {
            config_to_request(
                &json!({"trees": [{"root": ".", "hosts": v}]}),
                Path::new("/base"),
            )
            .unwrap_err()
            .0
        };
        assert_eq!(
            run(json!("x")),
            "trees[0].hosts must be an array of host strings."
        );
        assert_eq!(
            run(json!([""])),
            "trees[0].hosts[0] must be a non-empty string."
        );
        assert_eq!(
            run(json!(["https://x"])),
            "trees[0].hosts[0] must be a bare host, not a full URL (\"://\" is not allowed)."
        );
        assert_eq!(
            run(json!(["x/y"])),
            "trees[0].hosts[0] must be a bare host, not a path (\"/\" is not allowed)."
        );
        assert_eq!(
            run(json!(["x y"])),
            "trees[0].hosts[0] must not contain whitespace."
        );
    }

    #[test]
    fn well_formed_mounted_at_mounts_hosts_flow_into_the_tree_request() {
        let mapped = config_to_request(
            &json!({"trees": [{
                "root": ".",
                "mountedAt": "/gateway",
                "mounts": [{"dir": "apps/api", "at": "/api"}],
                "hosts": ["internal.example.com"]
            }]}),
            Path::new("/base"),
        )
        .unwrap();
        let tree = &mapped.request["trees"][0];
        assert_eq!(tree["mountedAt"], "/gateway");
        assert_eq!(tree["mounts"][0]["dir"], "apps/api");
        assert_eq!(tree["hosts"][0], "internal.example.com");
    }

    #[test]
    fn empty_mounts_and_hosts_arrays_are_omitted_from_the_request() {
        let mapped = config_to_request(
            &json!({"trees": [{"root": ".", "mounts": [], "hosts": []}]}),
            Path::new("/base"),
        )
        .unwrap();
        let tree = &mapped.request["trees"][0];
        assert!(tree.get("mounts").is_none());
        assert!(tree.get("hosts").is_none());
    }

    #[test]
    fn roots_shorthand_never_reads_mounted_at_mounts_hosts() {
        // These keys have no meaning off the `trees[]` shape; a `roots` config simply has nowhere to
        // put them, and the resulting tree request carries none of them.
        let mapped = config_to_request(&json!({"roots": ["."]}), Path::new("/base")).unwrap();
        let req = analyze_request(&mapped.request);
        assert!(req.get("mountedAt").is_none());
        assert!(req.get("mounts").is_none());
        assert!(req.get("hosts").is_none());
    }

    // --- overlays ---------------------------------------------------------------------------------

    #[test]
    fn overlay_happy_path_attaches_parsed_json_to_adapter_overlays() {
        let dir = TempDir::new("zzop-config-overlay-happy");
        dir.write(
            "overlay.json",
            r#"{"format": "zzop-normalized-ast", "version": 1}"#,
        );
        let mapped = config_to_request(
            &json!({"roots": ["."], "overlays": ["overlay.json"]}),
            dir.path(),
        )
        .unwrap();
        let req = analyze_request(&mapped.request);
        let overlays = req["adapterOverlays"].as_array().unwrap();
        assert_eq!(overlays.len(), 1);
        assert_eq!(overlays[0]["format"], "zzop-normalized-ast");
        assert_eq!(overlays[0]["version"], 1);
        assert!(mapped.warnings.iter().all(|w| !w.contains("overlay")));
    }

    #[test]
    fn missing_overlay_file_produces_a_warning_and_is_skipped() {
        let dir = TempDir::new("zzop-config-overlay-missing");
        let mapped = config_to_request(
            &json!({"roots": ["."], "overlays": ["does-not-exist.json"]}),
            dir.path(),
        )
        .unwrap();
        let req = analyze_request(&mapped.request);
        assert!(req.get("adapterOverlays").is_none());
        assert!(mapped.warnings.iter().any(|w| {
            w.contains("overlay \"does-not-exist.json\"")
                && w.contains("could not be read")
                && w.contains("This overlay is skipped.")
        }));
    }

    #[test]
    fn unparseable_overlay_file_produces_a_warning_and_is_skipped() {
        let dir = TempDir::new("zzop-config-overlay-bad-json");
        dir.write("bad.json", "{not json");
        let mapped = config_to_request(
            &json!({"roots": ["."], "overlays": ["bad.json"]}),
            dir.path(),
        )
        .unwrap();
        let req = analyze_request(&mapped.request);
        assert!(req.get("adapterOverlays").is_none());
        assert!(mapped.warnings.iter().any(|w| {
            w.contains("overlay \"bad.json\"")
                && w.contains("is not valid JSON")
                && w.contains("skipped")
        }));
    }

    #[test]
    fn overlay_paths_resolve_against_the_tree_root_not_base_dir() {
        let dir = TempDir::new("zzop-config-overlay-tree-relative");
        dir.write("sub/overlay.json", r#"{"marker": "sub"}"#);
        dir.write("overlay.json", r#"{"marker": "top"}"#);
        let mapped = config_to_request(
            &json!({"trees": [{"root": "sub", "overlays": ["overlay.json"]}]}),
            dir.path(),
        )
        .unwrap();
        let overlays = mapped.request["trees"][0]["adapterOverlays"]
            .as_array()
            .unwrap();
        assert_eq!(overlays[0]["marker"], "sub");
    }

    #[test]
    fn shared_and_tree_overlays_both_apply_in_order() {
        let dir = TempDir::new("zzop-config-overlay-shared-and-tree");
        dir.write("shared.json", r#"{"marker": "shared"}"#);
        dir.write("tree.json", r#"{"marker": "tree"}"#);
        let mapped = config_to_request(
            &json!({"trees": [{"root": ".", "overlays": ["tree.json"]}], "overlays": ["shared.json"]}),
            dir.path(),
        )
        .unwrap();
        let overlays = mapped.request["trees"][0]["adapterOverlays"]
            .as_array()
            .unwrap();
        assert_eq!(overlays.len(), 2);
        assert_eq!(overlays[0]["marker"], "shared");
        assert_eq!(overlays[1]["marker"], "tree");
    }

    #[test]
    fn overlays_shape_errors_match_js_text() {
        let err = config_to_request(
            &json!({"roots": ["."], "overlays": "valid.json"}),
            Path::new("/base"),
        )
        .unwrap_err();
        assert_eq!(err.0, "overlays must be an array of file paths.");
        let err = config_to_request(
            &json!({"roots": ["."], "overlays": [123]}),
            Path::new("/base"),
        )
        .unwrap_err();
        assert_eq!(
            err.0,
            "overlays entries must be non-empty strings (paths to overlay JSON files)."
        );
    }

    // --- unknown-key warnings at 3+ scopes -----------------------------------------------------

    #[test]
    fn unknown_key_warnings_fire_at_top_packs_and_tree_scopes() {
        let mapped = config_to_request(
            &json!({
                "roots": ["."],
                "bogusTopLevel": true,
                "packs": {"bogusPacksKey": 1},
            }),
            Path::new("/base"),
        )
        .unwrap();
        assert!(mapped
            .warnings
            .iter()
            .any(|w| w.contains("unknown config key \"bogusTopLevel\"")
                && w.contains("at the top level")));
        assert!(mapped
            .warnings
            .iter()
            .any(|w| w.contains("unknown config key \"packs.bogusPacksKey\"")
                && w.contains("under \"packs\"")));

        let mapped2 = config_to_request(
            &json!({"trees": [{"root": ".", "bogusTreeKey": 1}]}),
            Path::new("/base"),
        )
        .unwrap();
        assert!(mapped2.warnings.iter().any(|w| w
            .contains("unknown config key \"trees[0].bogusTreeKey\"")
            && w.contains("under \"trees[0]\"")));
    }

    #[test]
    fn unknown_key_warning_fires_inside_a_mounts_entry() {
        let mapped = config_to_request(
            &json!({"trees": [{"root": ".", "mounts": [{"dir": "a", "at": "/a", "bogus": 1}]}]}),
            Path::new("/base"),
        )
        .unwrap();
        assert!(mapped
            .warnings
            .iter()
            .any(|w| w.contains("unknown config key \"trees[0].mounts[0].bogus\"")));
    }

    #[test]
    fn unknown_key_warning_fires_inside_a_rule_object() {
        let mapped = config_to_request(
            &json!({"rules": {"toctou": {"severity": "off", "bogus": 1}}}),
            Path::new("/base"),
        )
        .unwrap();
        assert!(mapped
            .warnings
            .iter()
            .any(|w| w.contains("unknown config key \"rules.toctou.bogus\"")));
    }

    #[test]
    fn known_keys_never_warn() {
        let mapped = config_to_request(
            &json!({
                "roots": ["."],
                "packs": {"extraDirs": [], "disabled": []},
                "git": {"since": "2024-01-01"},
                "rules": {"toctou": {"severity": "warn", "exclude": ["a"]}},
            }),
            Path::new("/base"),
        )
        .unwrap();
        assert!(mapped
            .warnings
            .iter()
            .all(|w| !w.contains("unknown config key")));
    }

    // --- CLI-presentation keys are known but never forwarded -----------------------------------

    #[test]
    fn fail_on_format_report_are_known_but_not_forwarded() {
        let mapped = config_to_request(
            &json!({"roots": ["."], "failOn": "critical", "format": "json", "report": {"dir": "out"}}),
            Path::new("/base"),
        )
        .unwrap();
        assert!(mapped
            .warnings
            .iter()
            .all(|w| !w.contains("unknown config key")));
        let req = analyze_request(&mapped.request);
        assert!(req.get("failOn").is_none());
        assert!(req.get("format").is_none());
        assert!(req.get("report").is_none());
    }

    // --- packs.extraDirs resolution ------------------------------------------------------------

    #[test]
    fn packs_extra_dirs_resolve_against_base_dir_and_are_omitted_when_empty() {
        let mapped = config_to_request(
            &json!({"roots": ["."], "packs": {"extraDirs": ["./zzop-packs"]}}),
            Path::new("/base"),
        )
        .unwrap();
        let req = analyze_request(&mapped.request);
        let dirs = req["packsDir"].as_array().unwrap();
        assert_eq!(
            dirs[0],
            path_to_string(&resolve_path(Path::new("/base"), "./zzop-packs"))
        );

        let mapped_empty = config_to_request(
            &json!({"roots": ["."], "packs": {"extraDirs": []}}),
            Path::new("/base"),
        )
        .unwrap();
        assert!(analyze_request(&mapped_empty.request)
            .get("packsDir")
            .is_none());

        let mapped_none = config_to_request(&json!({"roots": ["."]}), Path::new("/base")).unwrap();
        assert!(analyze_request(&mapped_none.request)
            .get("packsDir")
            .is_none());
    }

    // --- git / cacheDir / sizeCap passthrough + withDefaults ------------------------------------

    #[test]
    fn git_defaults_to_empty_object_when_absent() {
        let mapped = config_to_request(&json!({"roots": ["."]}), Path::new("/base")).unwrap();
        let req = analyze_request(&mapped.request);
        assert_eq!(req["git"], json!({}));
    }

    #[test]
    fn git_passthrough_is_not_overwritten_by_the_default() {
        let mapped = config_to_request(
            &json!({"roots": ["."], "git": {"since": "2024-01-01"}}),
            Path::new("/base"),
        )
        .unwrap();
        let req = analyze_request(&mapped.request);
        assert_eq!(req["git"]["since"], "2024-01-01");
    }

    #[test]
    fn cache_dir_resolves_against_base_dir() {
        let mapped = config_to_request(
            &json!({"roots": ["."], "cacheDir": "./.zzop-cache"}),
            Path::new("/base"),
        )
        .unwrap();
        let req = analyze_request(&mapped.request);
        assert_eq!(
            req["cacheDir"],
            path_to_string(&resolve_path(Path::new("/base"), "./.zzop-cache"))
        );
    }

    #[test]
    fn size_cap_passes_through_unchanged() {
        let mapped =
            config_to_request(&json!({"roots": ["."], "sizeCap": 999}), Path::new("/base"))
                .unwrap();
        let req = analyze_request(&mapped.request);
        assert_eq!(req["sizeCap"], 999);
    }

    // --- packDefs ---------------------------------------------------------------------------------

    #[test]
    fn pack_defs_carries_every_bundled_pack_with_no_parse_warnings() {
        let mapped = config_to_request(&json!({"roots": ["."]}), Path::new("/base")).unwrap();
        let req = analyze_request(&mapped.request);
        let pack_defs = req["packDefs"].as_array().unwrap();
        assert_eq!(
            pack_defs.len(),
            14,
            "expected exactly the 14 bundled DSL packs"
        );
        assert!(mapped.warnings.iter().all(|w| !w.contains("bundled pack")));
    }

    #[test]
    fn every_tree_in_an_analyze_trees_request_gets_its_own_pack_defs() {
        let mapped =
            config_to_request(&json!({"roots": ["./a", "./b"]}), Path::new("/base")).unwrap();
        let trees = mapped.request["trees"].as_array().unwrap();
        for tree in trees {
            assert_eq!(tree["packDefs"].as_array().unwrap().len(), 14);
        }
    }

    #[test]
    fn a_bad_bundled_pack_source_becomes_a_warning_and_is_skipped() {
        let mut warnings = Vec::new();
        let defs = parse_pack_defs(
            &[("good.json", "{\"id\":\"g\"}"), ("bad.json", "not json")],
            &mut warnings,
        );
        assert_eq!(defs.len(), 1);
        assert!(warnings
            .iter()
            .any(|w| w.contains("\"bad.json\"") && w.contains("skipped")));
    }

    // --- representative config -> full request JSON deep-equal fixture -------------------------

    #[test]
    fn representative_config_maps_to_the_expected_request_shape() {
        let dir = TempDir::new("zzop-config-fixture");
        dir.write("overlay.json", r#"{"marker": "shared-overlay"}"#);

        let config = json!({
            "roots": ["."],
            "packs": {"extraDirs": ["./extra-packs"], "disabled": ["conventions"]},
            "rules": {
                "toctou": "off",
                "n-plus-one": {"severity": "warn", "exclude": ["legacy/", "**/*.gen.ts"]}
            },
            "exclude": ["vendor/"],
            "overlays": ["overlay.json"],
            "git": {"since": "2024-01-01", "recentDays": 14},
            "cacheDir": "./.cache",
            "sizeCap": 500000
        });

        let mapped = config_to_request(&config, dir.path()).unwrap();
        assert_eq!(mapped.method, Method::Analyze);

        let mut actual = mapped.request.clone();
        let pack_defs_len = actual["packDefs"].as_array().unwrap().len();
        actual.as_object_mut().unwrap().remove("packDefs");

        let expected = json!({
            "root": path_to_string(dir.path()),
            "packsDir": [path_to_string(&resolve_path(dir.path(), "./extra-packs"))],
            "disabledRules": ["conventions", "toctou"],
            "severityOverrides": {"n-plus-one": "warning"},
            "suppressions": [
                {"rule": "n-plus-one", "path": "legacy/"},
                {"rule": "n-plus-one", "glob": "**/*.gen.ts"}
            ],
            "globalExcludes": [{"path": "vendor/"}],
            "adapterOverlays": [{"marker": "shared-overlay"}],
            "git": {"since": "2024-01-01", "recentDays": 14},
            "cacheDir": path_to_string(&resolve_path(dir.path(), "./.cache")),
            "sizeCap": 500000
        });

        assert_eq!(actual, expected);
        assert_eq!(pack_defs_len, 14);
        assert!(mapped
            .warnings
            .iter()
            .all(|w| !w.contains("overlay") && !w.contains("unknown config key")));
    }

    // --- lexical path resolution sanity ---------------------------------------------------------

    #[test]
    fn resolve_path_normalizes_dot_and_dot_dot_segments() {
        let base = Path::new("/base/dir");
        assert_eq!(resolve_path(base, "."), base);
        assert_eq!(resolve_path(base, "./x"), base.join("x"));
        assert_eq!(resolve_path(base, "../sibling"), Path::new("/base/sibling"));
        assert_eq!(resolve_path(base, "a/../b"), base.join("b"));
    }
}
