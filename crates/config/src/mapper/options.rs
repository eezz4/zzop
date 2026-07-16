// ---------------------------------------------------------------------------------------------------
// Shared per-config options — the rule/pack/git/cache knobs that are global to the config (not
// per-tree), merged into every tree request. Only fields actually set are returned, so an omitted
// config key falls through to the facade/engine default.
// ---------------------------------------------------------------------------------------------------

use std::path::Path;

use crate::ConfigError;

use super::paths::{path_to_string, resolve_path};
use super::severity::{apply_severity, exclude_entry, suppression_entry};
use super::validation::is_json_falsy;

/// `JSON.stringify(value)` equivalent for a severity error's offending value — matches byte-for-byte
/// for every JSON primitive (strings, numbers, booleans, null), which covers every value a config
/// author could plausibly write for a severity field.
pub(super) fn json_stringify(value: &serde_json::Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}

pub(super) fn build_shared_options(
    config: &serde_json::Value,
    base_dir: &Path,
) -> Result<serde_json::Map<String, serde_json::Value>, ConfigError> {
    use serde_json::{Map, Value};

    let mut shared = Map::new();

    // --- packs shape gate. `config.packs || {}` in JS: an absent OR falsy `packs` defaults to empty
    // (no error); a present, truthy, non-object `packs` (e.g. an array) is a shape error — failing
    // loudly like `rules` below instead of silently mapping to nothing. ---
    let packs_obj = match config.get("packs") {
        None => None,
        Some(v) if is_json_falsy(v) => None,
        Some(Value::Object(m)) => Some(m),
        Some(_) => {
            return Err(ConfigError(
                "packs must be an object ({ \"extraDirs\": [...], \"disabled\": [...] })."
                    .to_string(),
            ))
        }
    };

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
