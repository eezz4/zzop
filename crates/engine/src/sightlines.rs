//! The engine aggregator half of the rule-sightline mechanism (`zzop_core::sightline`'s module doc
//! holds the contract): each owning rules crate declares, next to the rule, where that rule's trigger
//! evidence can be witnessed, and this module composes the crates' declarations into the one list a
//! consumer reads — the exact shape [`crate::register_all_native`] gives registration, and for the
//! same reason: the engine may enumerate, never own, per-rule data.
//!
//! The list is CAPABILITY-kind ("this build can/cannot see X"), independent of any run — its consumer
//! is `zzop_facade`'s coverage query, which crosses it with a tree's measured extension mix to derive
//! per-tree blind-spot cells instead of restating `docs/rules/catalog.md`'s sightline prose by hand.
//!
//! DECISIONS — two language-limited rules are deliberately UNDECLARED here (2026-07-31, refuted
//! candidates, not omissions):
//! - `mutating-route-no-auth`: its trigger IS witnessed in every call-graph-covered extension
//!   (`.go` routes included) — the gap is call-EDGE evidence, route-conditional, and owned by the
//!   S8 framework-silence warning (`framework_silence::call_graph_language_gap_warning`), which the
//!   coverage reply now forwards per tree. A sightline entry would mis-type a per-run conditional
//!   disclosure as a build capability.
//! - `route-shadowing`: its language gate is an EXEMPTION on routing semantics (first-match vs
//!   most-specific), not an evidence channel — semantic, not evidential, so an extension cross
//!   cannot express it.
//!
//! Both rows also deliberately carry NO sightline paragraph in `docs/rules/catalog.md`, and the
//! declared↔catalog set equality is pinned both directions by `rule_contracts`'
//! `catalog_sightline_rows_and_declared_rule_sightlines_are_the_same_set` — so a prose claim without
//! a declaration (or the reverse) can no longer fail silently.

use zzop_core::RuleSightline;

/// Every declared rule sightline in this build, in the crate order [`crate::register_all_native`]
/// composes registration (http, cross-layer, schema — of the five registering crates only these three
/// declare any today), so output derived from it is deterministic.
pub fn rule_sightlines() -> Vec<RuleSightline> {
    let mut out = zzop_rules_http::rule_sightlines();
    out.extend(zzop_rules_cross_layer::rule_sightlines());
    out.extend(zzop_rules_schema::rule_sightlines());
    out
}

#[cfg(test)]
mod tests {
    use super::rule_sightlines;
    use crate::dead_exports::is_ts_source_ext;

    /// A sightline is metadata ABOUT a registered rule — an id here that the registry does not know
    /// (a typo, or a declaration outliving a renamed rule) would publish a blind-spot cell for a rule
    /// that does not exist. Read off the real registry, never a hand list, like every id cross-check.
    #[test]
    fn every_declared_sightline_names_a_registered_rule_id_exactly_once() {
        let mut registry = zzop_core::RuleRegistry::new();
        crate::register_all_native(&mut registry);
        let mut seen = std::collections::BTreeSet::new();
        for s in rule_sightlines() {
            assert!(
                registry.ids().contains(&s.rule_id.to_string()),
                "sightline declares unregistered rule id {:?}",
                s.rule_id
            );
            assert!(
                seen.insert(s.rule_id),
                "duplicate sightline for {:?}",
                s.rule_id
            );
        }
        assert!(!seen.is_empty(), "sanity: no sightlines declared at all");
    }

    /// T2 policy-value pin, the `call_graph_covered_extensions_pin` arrangement (see
    /// `crate::dead_exports`): `QUERY_CALL_SITE_EXTENSIONS` and `RETRY_WITNESS_EXTENSIONS` are
    /// hand-kept duplicates of the dispatch table's TypeScript arm — their producers
    /// (`extract_query_call_sites`, the egress retry recognizer) run on exactly the
    /// TypeScript-dispatched files — and the owning crates depend on `zzop_core` only, so they cannot
    /// read [`is_ts_source_ext`] themselves. Both directions, so a dispatch-arm change or a rule-side
    /// edit each fail loudly with the side to re-justify.
    #[test]
    fn ts_witness_extension_lists_match_the_dispatch_table() {
        for (name, list) in [
            (
                "QUERY_CALL_SITE_EXTENSIONS",
                zzop_rules_schema::message::QUERY_CALL_SITE_EXTENSIONS,
            ),
            (
                "RETRY_WITNESS_EXTENSIONS",
                zzop_rules_cross_layer::RETRY_WITNESS_EXTENSIONS,
            ),
        ] {
            for ext in list {
                assert!(
                    is_ts_source_ext(&format!("x.{ext}")),
                    "{name} lists {ext:?}, but the dispatch table does not route it to TypeScript — \
                     the declared sightline names files its producer never sees"
                );
            }
            // Enumerated from the dispatch arm itself, same as the sibling pin's reverse direction.
            for ext in ["ts", "tsx", "js", "jsx", "mjs", "cjs", "mts", "cts"] {
                assert!(
                    list.contains(&ext),
                    "the dispatch table routes {ext:?} to TypeScript (so its producer DOES run \
                     there), but {name} omits it — the declared sightline UNDER-claims: {list:?}"
                );
            }
        }
    }
}
