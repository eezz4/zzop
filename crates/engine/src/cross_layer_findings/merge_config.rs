//! The two union `RuleConfig`s and the one union `VocabularyConfig` the cross-layer run derives from its
//! trees' per-tree configs; the contract prose lives on `compute_cross_layer_findings`'s own doc.

use std::collections::BTreeMap;
use std::path::PathBuf;

use zzop_core::{GlobalExclude, RuleConfig, Severity};

use crate::{EngineConfig, VocabularyConfig};

/// `(gate, merge_config)`:
///
/// - `gate` — disabledRules union (exclude-only): a cross-layer rule is disabled if its id appears
///   in ANY tree's `disabled_rules`. This is a joint-analysis output no single tree fully owns, so
///   any one tree opting out opts the whole run out of that rule.
/// - `merge_config` — severity-overrides union for the final `merge_findings` call. Cross-layer
///   findings are run-level while `severity_overrides` is a per-tree knob, so an override on ANY
///   tree takes effect; on a conflict (two trees override the SAME cross-layer rule id to
///   different severities) the FIRST-declaring tree wins, mirroring `analyze_trees`'
///   `host_owners` first-declarer precedent: trees may legitimately disagree, and first-declared
///   keeps the choice deterministic and input-order-stable. The override must be carried INTO the
///   merge (it runs before the sort there) — applying it after would leave a remapped finding in
///   its pre-override position (opus review, 2026-07-17 batch; sealed by
///   `merge_findings_sorts_by_the_overridden_severity_not_the_original` in zzop-core). Per-file
///   `suppressions` are a per-tree lever with no run-level meaning here, so they stay empty; the
///   top-level `exclude` (`global_excludes`) is NOT — it unions, for the reasons below.
///
/// ## Why `global_excludes` unions while `suppressions` stays empty
///
/// The two look alike (both are per-tree, path-filtered, finding-level levers) and the honest
/// counter-argument is that `exclude` is "a per-tree lever with no run-level meaning" for the same
/// reason `suppressions` is. It is not, and the difference is what each one is a statement ABOUT:
///
/// - `suppressions` is keyed by RULE ID — it is a statement about a VERDICT ("this rule's judgement
///   about this path is a known-acceptable one"). Carrying a tree's verdict-level acceptance into a
///   joint-analysis output would assert the tree accepted a claim it never saw: a `cross-layer/*` rule
///   never fires inside that tree's own run. And a tree that does want to opt out of a cross-layer
///   verdict already has a run-level lever built for exactly that — `disabled_rules`, unioned above.
/// - `global_excludes` is rule-agnostic by construction (`GlobalExclude` has no `rule` field) — it is a
///   statement about the OUTPUT SURFACE: "never name this path to me". `zzop_core`'s own contract prose
///   says a matching file has "findings from EVERY rule dropped, not just one", and that it is a REPORT
///   filter, never a scan-skip. A cross-layer finding names paths; whether the reasoning that produced
///   it was per-tree or run-level is irrelevant to an instruction about the NAME. There is no other way
///   to express it at run level — unlike `suppressions`, `exclude` has no `disabled_rules`-shaped
///   alternative — so leaving it empty here is not deferral, it is the lever being unreachable.
///
/// **WHAT THE FILTER COVERS.** `merge_findings` reads two things now: the finding's ANCHOR
/// (`is_suppressed(config, &f.rule_id, Some(f.file))` — an excluded anchor drops the finding whole) and
/// its [`evidence_paths`](zzop_core::Finding::evidence_paths), every OTHER path it prints — an excluded
/// one is replaced by `zzop_core::REDACTED` in the message and throughout `data`. So the enforced
/// contract is the promised one: never NAME this path.
///
/// It used to be only the anchor, and the gap was a family rather than a one-off: every two-sided
/// cross-layer rule prints a second path (`method-mismatch`'s `data.exampleProvide.file`, plus
/// `path-near-miss`, `route-near-miss`, `external-shadow-internal`, `body-field-drift`,
/// `retrying-write-no-idempotency`, `cross-tree-route-shadowing`, and the N-source `sites` arrays), so a
/// finding anchored on the CONSUME side survived a PROVIDE-side `exclude` and printed the excluded path
/// in its body anyway. Measured: `exclude` drove per-tree `findingCount` 36 -> 10 while
/// `crossLayerFindings` stayed at 33, still shipping an excluded path 33 times — the same
/// self-contradiction that got `global_excludes` wired into `zzop_metrics::build_recommendations`.
///
/// The inverse bit too, and is closed separately: `shared-db-table` used to anchor at the FIRST source's
/// file, so one tree excluding its own path deleted a finding whose entire point was that N OTHER trees
/// write that table. That family now emits one copy per participating source, each anchored in its own
/// tree (2026-07-29) — so the drop takes only the excluding tree's copy.
///
/// The widening did NOT become a per-rule table of "which `data` keys hold paths" — that is the
/// hand-maintained shape the sibling `union_vocabulary` doc refuses on staleness grounds, and it would
/// fail the same way here (a thirteenth rule leaves it silently short, and the failure looks exactly like
/// "that path was not excluded"). The knowledge lives in a typed field the rule fills instead; see
/// `zzop_core::Finding::evidence_paths` and `zzop_core::registry::redact`.
///
/// The union is EXCLUDE-ONLY, exactly like `disabled_rules` above: any tree's `exclude` filters the
/// whole run's cross-layer output, and no tree's config can ever ADD a finding. That direction is what
/// makes the union safe to take without per-tree attribution — and per-tree attribution is not
/// expressible here. `merge_findings` filters on `Finding::file`, which is a TREE-RELATIVE path
/// (`internal/handler.go`), so two trees sharing a relative path share an exclude; recovering the owning
/// tree would mean reading the finding's `data`, and there is no uniform key to read — the answer to
/// "does a cross-layer finding always carry its owning tree in `data.source`?" is NO, in three ways.
/// Single-tree rules write `data.source` (`unconsumed-endpoint`, `unconsumed-procedure`,
/// `unknown-verb-route`, `unresolved-consume-ratio`, `untraced-client-import-no-visible-consume`,
/// `external-host-fanout`); two-tree rules write `data.consumeSource`/`data.provideSource`
/// (`prefix-drift`, `method-mismatch`, `path-near-miss`, `route-near-miss`, `version-skew`,
/// `body-field-drift`, `retrying-write-no-idempotency`, `external-shadow-internal`, and
/// `external-secret-in-url`/`external-ip-literal`/`unprovided-mutation-call`/`ambiguous-consume` with a
/// consume side only); `route-shadowing` writes a third spelling again, `data.patternSource`; the N-tree
/// rules (`db-table-name-in-multiple-sources`, `external-host-in-multiple-sources`) write only a
/// `sources` ARRAY with no single owner; and `external-base-url-drift` writes no owner key at ALL — its
/// only source strings sit inside a truncated `exampleSites` sample (`take(4)`), so the anchor tree can
/// be absent from its own finding's data. A hand-listed key lookup over that set is precisely the staleness
/// shape `union_vocabulary` below refuses — it would silently stop attributing every rule added after it
/// was written. Over-suppression from a relative-path collision is a strictly smaller harm than
/// shipping a path the user said not to name, and it is visible: the same `exclude` also empties that
/// tree's own findings, so the user sees the breadth. Sealed by `analyze_cross_layer_exclude.rs`,
/// including the cross-tree direction, so a future move to per-tree semantics has to come here and say so.
pub(super) fn union_configs(trees: &[(PathBuf, EngineConfig)]) -> (RuleConfig, RuleConfig) {
    let mut disabled_union: Vec<String> = Vec::new();
    for (_, config) in trees {
        disabled_union.extend(config.rule_config.disabled_rules.iter().cloned());
    }
    let gate = RuleConfig {
        disabled_rules: disabled_union,
        ..RuleConfig::default()
    };
    let mut severity_union: BTreeMap<String, Severity> = BTreeMap::new();
    for (_, config) in trees {
        for (rule_id, severity) in &config.rule_config.severity_overrides {
            severity_union.entry(rule_id.clone()).or_insert(*severity);
        }
    }
    // Exclude-only, like `disabled_union` above: concatenated, not deduped (a repeated entry only costs a
    // redundant path match) and order-independent, since `is_suppressed` ORs them.
    let mut exclude_union: Vec<GlobalExclude> = Vec::new();
    for (_, config) in trees {
        exclude_union.extend(config.rule_config.global_excludes.iter().cloned());
    }
    let merge_config = RuleConfig {
        severity_overrides: severity_union,
        global_excludes: exclude_union,
        ..RuleConfig::default()
    };
    (gate, merge_config)
}

/// The run-level convention vocabulary, merged PER KEY across the trees' own `EngineConfig::vocabulary`.
///
/// Cross-layer findings are a run-level output while `vocabulary` is a per-tree knob, so a run-level answer
/// has to be derived — exactly the situation `severity_overrides` above is already in, and it is resolved
/// the same way: for each KEY, the FIRST tree (in input order) that DECLARES it wins, mirroring
/// `analyze_trees`' `host_owners` first-declarer precedent. Trees may legitimately disagree about what
/// their project calls a secret parameter, and first-declared keeps the choice deterministic and
/// input-order-stable. A key NO tree declares stays absent from the merged struct and falls back to its
/// built-in at `VocabularyConfig::resolve` time — never "the last tree's empty value wins", which would
/// silently blank a vocabulary another tree declared. Per key, not per tree: two trees each declaring a
/// DIFFERENT key both take effect, since neither conflicts with the other.
///
/// The merge runs over the serialized `serde_json::Map` rather than field by field ON PURPOSE: the next
/// vocabulary key is added to the struct, not here, and a hand-listed field-by-field merge would
/// silently stop merging every key added after it was written — the class of staleness this repo has
/// already measured twice. This sentence used to open with a COUNT of the keys ("~27"), which is the
/// same staleness in miniature and had already drifted (34 by the v0.25.0 audit) — the argument never
/// needed the number, and the number is exactly what a reader must not have to maintain. Going through `VocabularyConfig`'s own `Serialize`/`Deserialize`
/// (`#[serde(rename_all = "camelCase", default)]`, every field serialized unconditionally — see the
/// `vocabulary` module doc's cache-key section) makes a new key merge itself.
pub(super) fn union_vocabulary(trees: &[(PathBuf, EngineConfig)]) -> VocabularyConfig {
    let mut merged = serde_json::Map::new();
    for (_, config) in trees {
        let Ok(serde_json::Value::Object(map)) = serde_json::to_value(&config.vocabulary) else {
            continue;
        };
        for (key, value) in map {
            if is_declared(&value) {
                merged.entry(key).or_insert(value);
            }
        }
    }
    // Round-trips through the same shape it was serialized from, so the only way this fails is a future
    // field whose `Serialize` and `Deserialize` disagree; falling back to "nothing declared" then keeps the
    // run on the built-in vocabulary rather than taking it down.
    serde_json::from_value(serde_json::Value::Object(merged)).unwrap_or_default()
}

/// "Declared" for [`union_vocabulary`]: anything but `null`, `""` and `[]` — the same three shapes
/// `VocabularyConfig::resolve` already treats as absent, kept in agreement with it so a value that wins the
/// merge is never one `resolve` would then discard (which would let a declaring tree lose to nobody).
fn is_declared(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Null => false,
        serde_json::Value::String(s) => !s.is_empty(),
        serde_json::Value::Array(a) => !a.is_empty(),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(vocabulary: VocabularyConfig) -> (PathBuf, EngineConfig) {
        (
            PathBuf::from("/tree"),
            EngineConfig {
                vocabulary,
                ..EngineConfig::default()
            },
        )
    }

    /// Seals the per-key first-declarer merge in both directions: a key only ONE tree declares still wins
    /// (an undeclared neighbour never blanks it, whichever side it sits on), and when both declare the
    /// SAME key the FIRST tree in input order wins.
    #[test]
    fn the_first_tree_that_declares_a_vocabulary_key_wins() {
        let declaring = VocabularyConfig {
            secret_param_names: vec!["sessionid".to_string()],
            ..VocabularyConfig::default()
        };
        let silent = VocabularyConfig::default();

        for trees in [
            vec![tree(declaring.clone()), tree(silent.clone())],
            vec![tree(silent), tree(declaring.clone())],
        ] {
            assert_eq!(
                union_vocabulary(&trees).secret_param_names,
                vec!["sessionid".to_string()],
                "an undeclared tree must never blank a key its neighbour declared"
            );
        }

        let second = VocabularyConfig {
            secret_param_names: vec!["xsrf".to_string()],
            ..VocabularyConfig::default()
        };
        assert_eq!(
            union_vocabulary(&[tree(declaring), tree(second)]).secret_param_names,
            vec!["sessionid".to_string()],
            "on a conflict the FIRST declaring tree wins, like severity_overrides above"
        );
    }

    /// Seals the other half of the contract: a key nobody declares stays ABSENT from the merged struct,
    /// and therefore reaches the rule as "not judged" — the union must not invent a value that no tree
    /// stated. Before 2026-07-27 `resolve()` turned that absence into the built-in list; now it stays
    /// empty, and this test is the place that difference is visible.
    #[test]
    fn a_key_no_tree_declares_stays_undeclared() {
        let merged = union_vocabulary(&[
            tree(VocabularyConfig::default()),
            tree(VocabularyConfig::default()),
        ]);
        assert_eq!(merged, VocabularyConfig::default());
        assert!(
            merged.resolve().secret_param_names.is_empty(),
            "no tree named a secret-parameter vocabulary, so no parameter name is a secret"
        );
    }
}
