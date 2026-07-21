// ---------------------------------------------------------------------------------------------------
// Adapter overlays — see the crate-level doc's `withDefaults`/overlay summary and `mapper.js`'s own
// "Adapter overlays" section for the full rationale. Read/parse failures are NEVER fatal: they are
// pushed onto the shared warnings `Vec` and the offending overlay is skipped.
// ---------------------------------------------------------------------------------------------------

use std::path::Path;

use crate::ConfigError;

use super::paths::resolve_path;

/// Validates an `overlays` array's SHAPE (must be an array of non-empty strings) and extracts it as
/// owned `String`s. A shape violation is a config-authoring mistake (like any other mistyped array
/// field in this module) and fails fast with a `ConfigError` — unlike a per-file read/parse failure,
/// handled separately by `resolve_overlays_for_root`, which never throws.
pub(super) fn validate_overlays_array(
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
pub(super) fn resolve_overlays_for_root(
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
pub(super) fn validate_mount_at(
    value: &serde_json::Value,
    label: &str,
) -> Result<String, ConfigError> {
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
pub(super) fn validate_mounts_array(
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
pub(super) fn validate_hosts_array(
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

/// Validates a `routes` array's shape (`[{ key, role? }, ...]`): each entry is an object with a
/// non-empty `key` string and an optional `role` of exactly `"provide"` or `"consume"`. The deeper
/// `"METHOD PATH"` shape of `key` is NOT gated here — the facade's `routes_overlay` normalizes it and
/// soft-skips a malformed pair with a warning (an injected route that can never join is surfaced, not a
/// hard load error), the same "validate shape here, the join layer backstops semantics" split the mount
/// gates use. Returns the array unchanged (original `Value`s, extra keys surfaced separately via
/// `collect_config_warnings`'s `route` scope).
pub(super) fn validate_routes_array(
    value: &serde_json::Value,
    label: &str,
) -> Result<Vec<serde_json::Value>, ConfigError> {
    let arr = value.as_array().ok_or_else(|| {
        ConfigError(format!(
            "{label} must be an array of {{ key, role? }} objects."
        ))
    })?;
    for (i, entry) in arr.iter().enumerate() {
        if !entry.is_object() {
            return Err(ConfigError(format!(
                "{label}[{i}] must be an object with a \"key\" string."
            )));
        }
        entry
            .get("key")
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| {
                ConfigError(format!(
                    "{label}[{i}].key must be a non-empty \"METHOD PATH\" string (e.g. \"GET /api/users\")."
                ))
            })?;
        if let Some(role) = entry.get("role") {
            let ok = role
                .as_str()
                .is_some_and(|r| r == "provide" || r == "consume");
            if !ok {
                return Err(ConfigError(format!(
                    "{label}[{i}].role must be \"provide\" or \"consume\"."
                )));
            }
        }
    }
    Ok(arr.clone())
}

/// A JSON value is "falsy" exactly like JS's `||` operator would treat it: `null`, `false`, `0`
/// (any numeric zero), or `""`. Used ONLY where the JS source itself relies on `x || defaultValue`
/// (today: `config.rules`) — every other field in this module is presence-gated with `!== undefined`,
/// which `serde_json::Value::get` (`Option::is_some`) already matches directly.
pub(super) fn is_json_falsy(v: &serde_json::Value) -> bool {
    match v {
        serde_json::Value::Null => true,
        serde_json::Value::Bool(b) => !*b,
        serde_json::Value::Number(n) => n.as_f64() == Some(0.0),
        serde_json::Value::String(s) => s.is_empty(),
        _ => false,
    }
}
