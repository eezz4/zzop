//! The TWO honesty-channel merge helpers every query lane stamps on its reply — `configWarnings`
//! ([`facade_config_warnings`]) and the engine's own `warnings` ([`engine_warnings`]) — with one
//! definition of each merge contract (and of its absent-field degradation guarantee) instead of a copy
//! per lane.
//!
//! Both helpers read the MULTI-tree `analyzeTrees` document, and both read it PER TREE. That is the
//! shape of the document, not a preference: `MultiAnalyzeOutputView` carries `warnings` at its top level
//! for the join's own self-reports and carries no `configWarnings` there at all, so a lane that reads
//! either channel off the root alone contributes nothing on every run and publishes `[]` — which in this
//! repo is the honest "nothing to report" signal, making the silence indistinguishable from a clean run.
//! Three lanes shipped exactly that defect (`check_file`, `coverage`, `check_endpoint`); the helpers live
//! here so a fourth cannot write its own fourth copy of the loop.

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

/// The engine's own `warnings` for a run: the run-level join self-reports first, then EVERY tree's, in
/// tree order. Always an array (an absent/malformed field degrades to `[]`, never `null`), and uncapped
/// like the replies that carry it.
///
/// # Why a query lane carries them at all, and why every tree's rather than the matched one's
///
/// A query lane is not a narrower analysis than `analyze`/`cross` — it runs the identical
/// `analyze_trees_json` call over the identical trees and then asks one question of the result. So there
/// is no class of warning it "cannot produce": the whole-tree self-reports (a server framework imported
/// with zero routes recognized, `exclude` removing paths, a tree that walked no files) are already
/// computed and were simply being dropped on the floor. `zzop analyze` printed them, these lanes did
/// not, and a caller debugging through one had one eye fewer with nothing saying so.
///
/// Carrying only the tree named by the reply was the other candidate and is worse in the case that
/// matters most: a `not-found` verdict has no matched tree, and that verdict is exactly the one a
/// tree-level warning explains ("`exclude` removed these paths", "analyzed as an empty tree"). A
/// per-verdict shape would go silent precisely when the question is "why is this not here". The trees in
/// a query are the caller's own list — the `path`/`paths`/config THEY named — so there is no third party
/// whose noise this could be forwarding.
pub(crate) fn engine_warnings(analysis: &serde_json::Value) -> Vec<serde_json::Value> {
    let empty = Vec::new();
    let mut out: Vec<serde_json::Value> =
        analysis["warnings"].as_array().cloned().unwrap_or_default();
    for tree in analysis["trees"].as_array().unwrap_or(&empty) {
        if let Some(tree_warnings) = tree["output"]["warnings"].as_array() {
            out.extend(tree_warnings.iter().cloned());
        }
    }
    out
}

/// Every tree's facade-level `configWarnings`, in tree order — the per-tree loop each lane used to write
/// for itself. Pair it with the config loader's own warnings (those come first) to build the one
/// `configWarnings` channel a reply publishes.
pub(crate) fn tree_config_warnings(analysis: &serde_json::Value) -> Vec<serde_json::Value> {
    let empty = Vec::new();
    analysis["trees"]
        .as_array()
        .unwrap_or(&empty)
        .iter()
        .flat_map(|t| facade_config_warnings(&t["output"]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{engine_warnings, facade_config_warnings, tree_config_warnings};

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

    /// The defect this module exists to make unrepeatable, stated as a test: the multi-tree document
    /// carries NO `configWarnings` at its top level, so a lane reading it there gets `[]` — a value that
    /// reads as "nothing to report". Reading per tree gets the real entries.
    #[test]
    fn tree_config_warnings_reads_per_tree_because_the_root_has_no_such_field() {
        let doc = serde_json::json!({
            "trees": [
                { "output": { "configWarnings": ["unknown rule id `typo` in disabledRules"] } },
                { "output": { "configWarnings": [] } },
                { "output": { "configWarnings": ["unknown rule id `also-typo` in disabledRules"] } }
            ]
        });
        assert!(
            facade_config_warnings(&doc).is_empty(),
            "the multi-tree root must have no configWarnings — if it grows one, this module's per-tree \
             loop is no longer the whole answer"
        );
        assert_eq!(
            tree_config_warnings(&doc),
            vec![
                serde_json::json!("unknown rule id `typo` in disabledRules"),
                serde_json::json!("unknown rule id `also-typo` in disabledRules"),
            ]
        );
    }

    /// The engine channel's two halves and their order: the join's own run-level self-reports first,
    /// then every tree's, in tree order. Absence anywhere degrades to nothing, never a `null` entry.
    #[test]
    fn engine_warnings_merges_the_run_level_reports_then_every_trees_in_order() {
        let doc = serde_json::json!({
            "warnings": ["join ran with a parallel implementation mismatch"],
            "trees": [
                { "output": { "warnings": ["tree one walked no files"] } },
                { "output": {} },
                { "output": { "warnings": ["tree three: exclude removed 12 paths"] } }
            ]
        });
        assert_eq!(
            engine_warnings(&doc),
            vec![
                serde_json::json!("join ran with a parallel implementation mismatch"),
                serde_json::json!("tree one walked no files"),
                serde_json::json!("tree three: exclude removed 12 paths"),
            ]
        );
        assert!(engine_warnings(&serde_json::json!({})).is_empty());
    }
}
