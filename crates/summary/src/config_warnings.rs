//! Facade-level `configWarnings` merge helper, shared by `analyze_summary` and `cross_summary` — one
//! definition of the merge contract (and its absent-field degradation guarantee) instead of two
//! independently-drifting copies.

/// Facade-level `configWarnings` entries riding a tree output's JSON — config-front-end diagnostics
/// the engine reports OUTSIDE its `warnings` channel (e.g. unknown-rule-id override diagnostics).
/// They belong in the reply's `configWarnings` array (after the config-loader's own warnings) — the
/// two sources feed ONE channel because they are the same kind of honesty (config handling), unlike
/// the engine's `warnings`, which stays separate. `.get()` is deliberate: the field may be absent on
/// older/edge outputs, and absence degrades to "nothing to merge" — never a panic or a `null` entry.
pub(crate) fn facade_config_warnings(output: &serde_json::Value) -> Vec<serde_json::Value> {
    output
        .get("configWarnings")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::facade_config_warnings;

    #[test]
    fn facade_config_warnings_merge_entries_and_degrade_to_empty_when_absent() {
        // The facade-level `configWarnings` field (engine-side config diagnostics, e.g. unknown-rule-id
        // override diagnostics) merges into the reply's configWarnings after the loader's own; an ABSENT
        // field (an older/edge output) must degrade to "nothing to merge", never a panic or a JSON null
        // entry.
        let with = serde_json::json!({ "configWarnings": ["unknown rule id in overrides"] });
        assert_eq!(
            facade_config_warnings(&with),
            vec![serde_json::json!("unknown rule id in overrides")]
        );
        let without = serde_json::json!({ "warnings": [] });
        assert!(facade_config_warnings(&without).is_empty());
        let non_array = serde_json::json!({ "configWarnings": null });
        assert!(facade_config_warnings(&non_array).is_empty());
    }
}
