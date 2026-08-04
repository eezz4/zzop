//! The `rules.<id>` half of the rule-selection surface: one config map fanned out into the three
//! engine-facing lists it actually feeds — `disabledRules`, `severityOverrides` and `suppressions`.
//!
//! Split out of `super` when that file crossed the 300-line cap, along the seam the surface already
//! has: `packs.*` selects WHOLE PACKS (the coarse, domain-level axis a reader reaches for first) and
//! `rules.*` addresses ONE ID (the fine axis). They meet in exactly one place — a rule set to a
//! disabling severity here lands in the same `disabled` vec `packs.disabled` filled — which is why
//! that vec is threaded in as a parameter rather than owned here: there is one disabled list, and
//! both halves of the surface write to it.

use serde_json::{Map, Value};

use crate::ConfigError;

use super::super::severity::{apply_severity, suppression_entry};
use super::super::validation::is_json_falsy;

/// Reads `config.rules` and writes whatever it implies into `shared` (`disabledRules`,
/// `severityOverrides`, `suppressions`), appending to the `disabled` list `packs.disabled` already
/// seeded. Each of the three keys is emitted ONLY when non-empty — an absent key and an empty one
/// would otherwise be indistinguishable downstream, and "the caller declared an empty accept-list" is
/// a claim no absent config makes.
///
/// `config.rules || {}` in the JS mapper this ports: an absent OR falsy `rules` is empty (no error),
/// while a present-but-not-an-object `rules` (an array, a string) is a shape error rather than a
/// silent skip — the same "reject a shape we cannot honour, never guess it" line every other key in
/// this mapper draws.
pub(super) fn fold_rules_map(
    config: &Value,
    shared: &mut Map<String, Value>,
    mut disabled: Vec<Value>,
) -> Result<(), ConfigError> {
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
                                ConfigError(format!(
                                    "rules.{rule_id}.exclude entries must be strings."
                                ))
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
    Ok(())
}
