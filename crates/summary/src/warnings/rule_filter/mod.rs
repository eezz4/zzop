//! The findings-view `rule` filter's own honesty check — the one helper in this module family that
//! PRODUCES a warning instead of merging one, split out of `warnings.rs` on 2026-08-12 for the repo's
//! per-file line cap. The seam is the one the parent's own header names: gathering a channel and
//! deciding there is something to say are different jobs, and only this one needs the reply's
//! `packsLoaded` to decide.

mod gated_id;
mod not_evaluated;

/// The findings-view `rule` filter's own honesty check: a warning when the id it was given can be
/// PROVEN to name no rule this run could report, `None` otherwise.
///
/// ## The defect (measured 2026-08-11, still live 2026-08-12)
///
/// `--rule <id>` with an id that does not exist returns exit 0, `shown: 0`, and NOT ONE warning. That
/// is indistinguishable from "this rule exists and found nothing", which is the answer a reader
/// usually assumes — so a typo, or an id whose pack was EXPORTED out of the bundle, reads as a clean
/// bill of health. Measured on a one-file tree: `--rule typescript/no-explicit-any` (a real id whose
/// pack moved to `examples/packs/` on 2026-08-11) and `--rule sql/definitely-not-a-rule` produced
/// byte-identical replies apart from nothing at all.
///
/// The sibling channels already do this for the CONFIG dialect (`unknown_disabled_rule_ids` and
/// friends), which is what makes the view filter's silence an asymmetry rather than a policy.
///
/// ## The id universe, and the half that used to be missing
///
/// The exact answer is "every native id, plus every rule of every pack this run loaded". The first
/// half has always been available ([`zzop_facade::native_analysis_ids`]). The second was not — the
/// reply's `packsLoaded` carried each pack's id, rule COUNT and source, never its rule ids — and this
/// crate is layered above the facade and must not reach past it into the engine's loaded config. Since
/// 2026-08-20 the run publishes the list (`packsLoaded[].ruleIds`; `zzop_engine::PackLoaded::rule_ids`
/// owns the measurement that forced it and the size it costs), so both halves are readable from the
/// reply with no layering violation.
///
/// The check still fires only where it can be certain, and stays silent where it cannot:
/// - a BARE id (no `/`) is judged against the native ids in full. A DSL finding's `rule_id` is always
///   `"<pack>/<rule>"`, so a bare id that is not native can never match a finding — including a bare
///   PACK id, which is legal in `packs.disabled` and meaningless here.
/// - a REGISTERED id that still keys no finding is refused too, and this arm runs FIRST because the
///   one above clears it. Registration is the id space the CONFIG gates, which is strictly wider than
///   the ids a finding carries: `zzop_metrics`' score gates report into the score surfaces, and the
///   schema family gates report under `schema/<label>`. Reading only the wider set is what let
///   `--fail-on`-free `--rule schema-structural` come back `shown: 0` beside a 92-finding census with
///   an empty stderr — measured 2026-09-05 on `cases/trees/api-be`, all seven ids, no warning on any
///   of them. `zzop_facade::ids_that_carry_no_finding` is the narrower set, and its own doc holds why
///   the registry could not answer this.
/// - a QUALIFIED id whose PACK is absent from `packsLoaded` could not have matched, whatever the tree
///   holds. That reading carries its own prescription (below) because an exported pack is not a typo.
/// - a QUALIFIED id whose pack IS loaded is judged against that pack's `ruleIds`. A missing `ruleIds`
///   key (an older/edge reply shape) means NO DATA and returns `None` — never a refusal, which would
///   be the false-positive direction and is exactly what validating against a compiled-in catalog
///   would have done to a user pack loaded from `<tree>/zzop/rules/`.
///
/// NOT applied to the cross-tree lane's `crossLayerFindings` (`crate::cross`), deliberately: every
/// finding on that channel carries a NATIVE id (`cross-layer/*`, `schema/*`), so its id universe is a
/// different set from "the packs this run loaded" and the prefix test above would be answering a
/// question nobody asked there. That lane needs its own check, not this one widened.
///
/// The prefix case is the one the defect was reported for, and it is the one that carries a
/// PRESCRIPTION: an exported pack is not a typo, and telling the reader to check their spelling is
/// the exact wrong conclusion. The config-side channel
/// (`crates/metrics/src/diagnostics/config_reports.rs`) said exactly that for the same input until
/// 2026-08-13, when this clause was ported there — all four of its reports now hand over the same two
/// readings, so the two channels no longer disagree about what an unmatched id means.
///
/// ## Where the prescription POINTS, and why it moved (2026-08-13)
///
/// It used to say "a pack that moved to `examples/packs/` still loads — drop the file in
/// `<tree>/zzop/rules/`". `examples/packs/` is a REPO path, and the reader of this warning is by
/// construction someone whose build does not carry the pack: an npm or `.mcpb` install has no
/// checkout to drop a file out of. The retrieval path that actually exists for them is the embedded
/// `example-pack-*` contract document, named here in BOTH host dialects because this channel reaches
/// CLI and MCP readers alike (`host_vocabulary` contracts 15/16 fail this file if either twin is
/// dropped). The pack STEM is the file stem, not the pack id — `typescript` lives in
/// `example-pack-typescript-lint` — which is why the sentence points at the contract index rather
/// than composing a name out of the id it just reported.
pub(crate) fn unknown_rule_filter_warning(
    output_view: &serde_json::Value,
    rule: &str,
) -> Option<String> {
    let packs: &[serde_json::Value] = output_view["packsLoaded"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();

    // Registered is not reportable, and the gap is seven ids wide. Ahead of the membership test
    // below because that test would clear every one of them: they ARE registered — that is what
    // makes them gates — and a finding can still never be keyed by one.
    if let Some(w) = gated_id::gated_id_refusal(rule) {
        return Some(w);
    }

    // Registered, spelled right, and STILL unable to appear — because this run never evaluated it.
    // Ahead of the membership test below for the same reason as the arm above: that test clears every
    // one of these (they are native ids) and returns `None`, which is the silence this whole module
    // exists to remove. The reply already partitions them by WHO switched them off; this reads it.
    if let Some(w) = not_evaluated::not_evaluated_refusal(output_view, rule) {
        return Some(w);
    }

    // Checked BEFORE the shape split, because the native registry answers for BOTH shapes and the
    // qualified arm below cannot: a native analysis is namespaced exactly like a pack-qualified rule
    // (`schema/god-model`, `cross-layer/route-near-miss`) and is compiled in rather than loaded, so it
    // never appears in `packsLoaded`. Testing the prefix as a pack id alone reported "no pack `schema`
    // was loaded" on a run whose `shown` held the `schema/god-model` finding.
    if zzop_facade::native_analysis_ids()
        .iter()
        .any(|id| id == rule)
    {
        return None;
    }

    match rule.split_once('/') {
        None => {
            // Before denying that the id exists, consult the SECOND id space the bare form could
            // belong to. A native analysis is namespaced (`schema/god-model`), so its tail is a
            // spelling a caller reasonably types — and saying "that is not a native analysis id" of
            // `god-model` is simply false. Resolved only when EXACTLY one registered id ends in it,
            // the same terms the CLI's own pre-check and `explain` use.
            //
            // 🔴 The CLI had this repair since its `args.rs` gained the lookup, and it exits 2 BEFORE
            // this shared sentence is ever built — so the fix lived on one host while the other host
            // shipped the falsehood. That file's own comment named the shape: "One binary, two
            // verdicts on whether the thing the caller typed exists." This is the second binary.
            //
            // Host-neutral by contract: name the id, never a flag or an argument. Each product's own
            // usage text owns the spelling of HOW to pass it.
            let native = zzop_facade::native_analysis_ids();
            let tails: Vec<&String> = native
                .iter()
                .filter(|id| id.rsplit_once('/').is_some_and(|(_, tail)| tail == rule))
                .collect();
            if let [full] = tails.as_slice() {
                return Some(format!(
                    "the `rule` filter names `{rule}`, which is the bare form of the native analysis \
                     id `{full}` — and a finding carries the FULL id, so nothing can match this \
                     filter. This reply's `shown: 0` is the filter, not a clean result. Name \
                     `{full}` instead."
                ));
            }
            Some(format!(
                "the `rule` filter names `{rule}`, which is not a native analysis id — and a DSL rule's \
                 id is always `<pack>/<rule>`, so no finding can ever match it. This reply's `shown: 0` \
                 is the filter, not a clean result. The `rule-catalog` contract document lists every id \
                 this build ships."
            ))
        }
        Some((pack, name)) => {
            if let Some(entry) = packs.iter().find(|p| p["id"].as_str() == Some(pack)) {
                // The pack loaded, so the only remaining question is whether it carries this rule —
                // answerable from the ids it published, and from nothing else. An absent `ruleIds`
                // key is NO DATA (older/edge shape) and must stay silent: a warning there would be
                // the false-positive direction this channel refuses.
                let ids = entry["ruleIds"].as_array()?;
                if ids.iter().any(|id| id.as_str() == Some(name)) {
                    return None;
                }
                return Some(format!(
                    "the `rule` filter names `{rule}`, and pack `{pack}` DID load in this run but \
                     carries no rule `{name}` — so no finding could match it and this reply's \
                     `shown: 0` is the filter rather than a clean result. This is a spelling \
                     mistake, not an unloaded pack: that pack's own `ruleIds` in `packsLoaded` \
                     lists every id it could have reported here, and the `rule-catalog` contract \
                     document lists every id this build ships."
                ));
            }
            Some(format!(
                "the `rule` filter names `{rule}`, but no pack `{pack}` was loaded in this run, so no \
                 finding could match it and this reply's `shown: 0` is the filter rather than a clean \
                 result. Two readings, and they need different fixes: the id may be misspelled, or its \
                 pack may be one this build SHIPS BUT DOES NOT LOAD — an exported pack is a real id, \
                 not a spelling mistake. Every exported pack is retrievable from this build and starts \
                 matching once its file is in the tree: MCP resource \
                 `zzop://contract/example-pack-<stem>` on MCP hosts (`zzop contract \
                 example-pack-<stem>` with the CLI binary; the contract index lists one entry per \
                 exported pack), saved under `<tree>/zzop/rules/` or in a directory named by \
                 `packs.extraDirs` — that key REPLACES the `zzop/rules/` default rather than adding \
                 to it, so a tree already using it must name this directory too. `packsLoaded` in \
                 this reply lists the pack ids that DID load."
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::unknown_rule_filter_warning;

    /// The reported defect, in the shape it was reported: a real rule id whose pack left the bundle.
    /// The message must NOT read as a spelling correction — that is the wrong conclusion for this
    /// input, and the runtime's config-side sibling made exactly that mistake until 2026-08-13.
    #[test]
    fn a_rule_filter_naming_an_unloaded_packs_rule_is_named_and_not_called_a_typo() {
        let view = serde_json::json!({ "packsLoaded": [{ "id": "sql" }, { "id": "security" }] });
        let w = unknown_rule_filter_warning(&view, "typescript/no-explicit-any")
            .expect("an unloaded pack's rule must be reported");
        assert!(w.contains("typescript/no-explicit-any") && w.contains("no pack `typescript`"));
        // The prescription must be REACHABLE by whoever receives this warning — someone whose build
        // does not carry the pack, and who therefore has no repo checkout to copy a file out of. Both
        // host dialects of the retrieval resource, plus the config key that widens the search path.
        assert!(
            w.contains("zzop://contract/example-pack-")
                && w.contains("`zzop contract example-pack-")
                && w.contains("packs.extraDirs"),
            "the prescription must name the retrieval resource in BOTH host dialects: {w}"
        );
        assert!(
            !w.contains("examples/packs/"),
            "a repo path is not a retrieval route for a reader who installed a binary: {w}"
        );
        assert!(
            !w.contains("typo"),
            "an exported pack is not a misspelling; the word must not appear: {w}"
        );
    }

    /// The half that used to be silent: a typo inside a pack that IS loaded. Silent until the run
    /// published `ruleIds`, and silent is the worst answer available here — `shown: 0` under a
    /// misspelled filter reads exactly like a clean rule. The message must place the blame correctly:
    /// this one IS a spelling mistake, unlike the unloaded-pack case above.
    #[test]
    fn a_nonexistent_rule_inside_a_loaded_pack_is_reported_once_the_pack_publishes_its_ids() {
        let view = serde_json::json!({
            "packsLoaded": [{ "id": "sql", "ruleIds": ["nplus1", "count-in-loop"] }]
        });
        let w = unknown_rule_filter_warning(&view, "sql/definitely-not-a-rule")
            .expect("a pack that published its ids and does not carry this one must be reported");
        assert!(
            w.contains("sql/definitely-not-a-rule") && w.contains("DID load"),
            "the message must say the pack loaded, or the reader chases the wrong fix: {w}"
        );
        assert!(
            w.contains("ruleIds"),
            "the message must name the field holding the answer: {w}"
        );
    }

    /// A real, loaded rule id must never warn — the non-vacuity leg. Without it the tests above pass
    /// just as well on a function that reports everything.
    #[test]
    fn a_loaded_packs_rule_id_is_silent() {
        let view = serde_json::json!({
            "packsLoaded": [{ "id": "sql", "ruleIds": ["nplus1", "count-in-loop"] }]
        });
        assert_eq!(unknown_rule_filter_warning(&view, "sql/nplus1"), None);
    }

    /// The one-sidedness that MUST survive the widening: a pack entry carrying no `ruleIds` is NO
    /// DATA, and a claim without evidence is the defect this whole channel exists to avoid. This is
    /// also the shape a user pack loaded out of `<tree>/zzop/rules/` would take on any reply older
    /// than the field — refusing it would be the false-positive direction, which is the bug that was
    /// fixed for native ids the same day and must not be reintroduced in a new spelling.
    #[test]
    fn a_pack_that_publishes_no_ids_is_never_turned_into_a_refusal() {
        let view = serde_json::json!({ "packsLoaded": [{ "id": "sql" }] });
        assert_eq!(
            unknown_rule_filter_warning(&view, "sql/definitely-not-a-rule"),
            None,
            "no `ruleIds` means the reply cannot answer, not that the rule is absent"
        );
    }

    /// A bare id is judged in full against the real registry, both directions. `dead-candidates` is a
    /// native analysis; `dead-candidatez` is not, and neither is a bare PACK id — legal in
    /// `packs.disabled`, meaningless as a finding filter.
    #[test]
    fn a_bare_id_is_judged_against_the_native_registry_in_both_directions() {
        let view = serde_json::json!({ "packsLoaded": [{ "id": "sql" }] });
        assert_eq!(
            unknown_rule_filter_warning(&view, "dead-candidates"),
            None,
            "a real native id must not warn"
        );
        let w = unknown_rule_filter_warning(&view, "dead-candidatez")
            .expect("a misspelled native id must be reported");
        assert!(w.contains("dead-candidatez") && w.contains("native analysis id"));
        assert!(
            unknown_rule_filter_warning(&view, "sql").is_some(),
            "a bare PACK id can never equal a finding's rule_id, so it must be reported"
        );
    }

    /// Degradation: an output with no `packsLoaded` at all must not turn every qualified filter into a
    /// warning. It reports (no pack is loaded, so nothing can match) — the point is that it does not
    /// PANIC on the missing field, which `["packsLoaded"]` indexing would have made easy to get wrong.
    #[test]
    fn a_missing_packs_loaded_field_degrades_instead_of_panicking() {
        let view = serde_json::json!({});
        assert!(unknown_rule_filter_warning(&view, "sql/nplus1").is_some());
        assert_eq!(unknown_rule_filter_warning(&view, "dead-candidates"), None);
    }

    /// The other direction, and the one that breaks a working command line: an id findings really do
    /// carry must still pass. `schema/god-model` is the trap — it is native, it is namespaced exactly
    /// like a pack-qualified rule, and its family gate is refused two lines up.
    #[test]
    fn a_schema_issue_id_that_findings_do_carry_is_not_refused() {
        let view = serde_json::json!({ "packsLoaded": [] });
        for id in ["schema/god-model", "schema/unreferenced-field-name"] {
            assert_eq!(
                unknown_rule_filter_warning(&view, id),
                None,
                "`--rule {id}` matches real findings and must not be refused"
            );
        }
    }
    /// The ROUTING, which is all this level owns: a gate id must reach the refusal instead of being
    /// cleared by the registry-membership arm below it. What that refusal then SAYS is pinned once,
    /// in `gated_id`'s own test — asserting the sentence here too would put one claim under two
    /// owners, and this file has a whole module doc about what that costs.
    #[test]
    fn a_registered_id_that_no_finding_can_carry_reaches_the_refusal() {
        let view = serde_json::json!({ "packsLoaded": [] });
        for id in &zzop_facade::ids_that_carry_no_finding() {
            assert!(
                unknown_rule_filter_warning(&view, id).is_some(),
                "`--rule {id}` returned no warning: it is registered, so the membership arm clears \
                 it, and `shown: 0` then reads as a clean result"
            );
        }
    }
}
