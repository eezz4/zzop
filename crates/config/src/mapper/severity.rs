// ---------------------------------------------------------------------------------------------------
// Severity normalization — the SINGLE source of truth for turning friendly config severities into the
// engine's `Severity` serde values (see `crates/core/src/finding.rs`'s `#[serde(rename_all =
// "lowercase")]` and `crates/facade/src/lib.rs`'s `AnalyzeRequest::severity_overrides`).
// ---------------------------------------------------------------------------------------------------

use crate::ConfigError;

use super::options::json_stringify;
use super::paths::is_glob_pattern;

/// A normalized severity: either the `"off"` sentinel (routes to `disabledRules`) or one of the three
/// engine severity strings (routes to `severityOverrides`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SeverityValue {
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
pub(super) fn normalize_severity(
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

/// Shared `rules.<id>` / `rules.<id>.severity` handling: normalizes `sev_value`, then either records
/// `rule_id` as disabled (`"off"`) or as a `severityOverrides` entry — the exact same routing whether
/// the config wrote a bare severity string or `{severity: ...}`.
pub(super) fn apply_severity(
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

pub(super) fn suppression_entry(rule_id: &str, path_or_glob: &str) -> serde_json::Value {
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

pub(super) fn exclude_entry(path_or_glob: &str) -> serde_json::Value {
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
