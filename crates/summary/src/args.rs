//! Shared `tools/call`-shaped argument-extraction helpers, used both internally (`crate::output::
//! FindingFilters`) and by every host's own dispatch layer (e.g. `zzop-mcp`'s `tools` module calls
//! these directly to extract its `tools/call` arguments before calling into this crate). A live-fire
//! round found every one of these arguments accepted the WRONG JSON type silently: a non-string
//! `path`/`configPath`/`pattern`/`rule`/`envelopeJson`/`packJson`, or a non-array `paths`, degraded to
//! "argument not provided" (a `serde_json::Value::as_*` call simply returns `None` on a type mismatch,
//! indistinguishable from an absent key) instead of a named error. The declared `inputSchema` `type`
//! constraints are advisory only — nothing in a host's transport enforces them against a caller that
//! ignores the schema — so THIS layer is the one place that actually validates JSON type before a value
//! reaches a handler. Only an ABSENT key or an explicit JSON `null` (its natural stand-in) means "not
//! provided"; every other wrong-type value is a caller mistake, named in the error, never a silent
//! fallback.

use serde_json::Value;

/// Extracts a required string argument. Absent/`null` -> "missing" (the caller omitted it); present with
/// a non-string value -> a named type error (the caller sent the wrong JSON shape).
pub fn required_string<'a>(args: Option<&'a Value>, name: &str) -> Result<&'a str, String> {
    match args.and_then(|a| a.get(name)) {
        None | Some(Value::Null) => Err(format!("missing `{name}` argument")),
        Some(v) => v
            .as_str()
            .ok_or_else(|| format!("`{name}` must be a string (got {v})")),
    }
}

/// Extracts an optional string argument: absent/`null` -> `Ok(None)`; present with a non-string value ->
/// a named type error (never a silent `None`, which would read identically to "not provided").
pub fn optional_string<'a>(args: Option<&'a Value>, name: &str) -> Result<Option<&'a str>, String> {
    match args.and_then(|a| a.get(name)) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_str()
            .map(Some)
            .ok_or_else(|| format!("`{name}` must be a string (got {v})")),
    }
}

/// Extracts an optional array-of-strings argument (`paths`): absent/`null` -> `Ok(vec![])`; present but
/// not an array, or an array holding a non-string element, is a named type error — an offending element
/// used to be silently dropped (`filter_map`), shrinking the list with no error at all.
pub fn optional_string_array(args: Option<&Value>, name: &str) -> Result<Vec<String>, String> {
    match args.and_then(|a| a.get(name)) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(items)) => items
            .iter()
            .map(|v| {
                v.as_str()
                    .map(String::from)
                    .ok_or_else(|| format!("`{name}` entries must be strings (got {v})"))
            })
            .collect(),
        Some(v) => Err(format!("`{name}` must be an array of strings (got {v})")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_string_distinguishes_missing_from_wrong_type() {
        assert_eq!(
            required_string(Some(&serde_json::json!({})), "path").unwrap_err(),
            "missing `path` argument"
        );
        assert_eq!(
            required_string(Some(&serde_json::json!({ "path": null })), "path").unwrap_err(),
            "missing `path` argument"
        );
        assert_eq!(
            required_string(Some(&serde_json::json!({ "path": 5 })), "path").unwrap_err(),
            "`path` must be a string (got 5)"
        );
        assert_eq!(
            required_string(Some(&serde_json::json!({ "path": "x" })), "path").unwrap(),
            "x"
        );
    }

    #[test]
    fn optional_string_ok_none_when_absent_error_when_wrong_type() {
        assert_eq!(
            optional_string(Some(&serde_json::json!({})), "configPath").unwrap(),
            None
        );
        assert_eq!(
            optional_string(
                Some(&serde_json::json!({ "configPath": null })),
                "configPath"
            )
            .unwrap(),
            None
        );
        assert_eq!(
            optional_string(
                Some(&serde_json::json!({ "configPath": true })),
                "configPath"
            )
            .unwrap_err(),
            "`configPath` must be a string (got true)"
        );
    }

    #[test]
    fn optional_string_array_rejects_non_array_and_non_string_elements() {
        assert_eq!(
            optional_string_array(Some(&serde_json::json!({})), "paths").unwrap(),
            Vec::<String>::new()
        );
        assert_eq!(
            optional_string_array(Some(&serde_json::json!({ "paths": "a" })), "paths").unwrap_err(),
            "`paths` must be an array of strings (got \"a\")"
        );
        assert_eq!(
            optional_string_array(Some(&serde_json::json!({ "paths": ["a", 5] })), "paths")
                .unwrap_err(),
            "`paths` entries must be strings (got 5)"
        );
        assert_eq!(
            optional_string_array(Some(&serde_json::json!({ "paths": ["a", "b"] })), "paths")
                .unwrap(),
            vec!["a".to_string(), "b".to_string()]
        );
    }
}
