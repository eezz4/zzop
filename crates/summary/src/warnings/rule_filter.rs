//! The findings-view `rule` filter's own honesty check — the one helper in this module family that
//! PRODUCES a warning instead of merging one, split out of `warnings.rs` on 2026-08-12 for the repo's
//! per-file line cap. The seam is the one the parent's own header names: gathering a channel and
//! deciding there is something to say are different jobs, and only this one needs the reply's
//! `packsLoaded` to decide.

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
/// ## Why this cannot simply consult the full id universe
///
/// The exact answer is "every native id, plus every rule of every pack this run loaded". The first
/// half is available ([`zzop_facade::native_analysis_ids`]); the second is not — the reply's
/// `packsLoaded` carries each pack's id, rule COUNT and source, never its rule ids, and this crate is
/// layered above the facade and must not reach past it into the engine's loaded config.
///
/// So the check is deliberately ONE-SIDED — it fires only where it can be certain, and stays silent
/// where it cannot:
/// - a BARE id (no `/`) is judged against the native ids in full. A DSL finding's `rule_id` is always
///   `"<pack>/<rule>"`, so a bare id that is not native can never match a finding — including a bare
///   PACK id, which is legal in `packs.disabled` and meaningless here.
/// - a QUALIFIED id is judged on its PACK PREFIX only: if no loaded pack carries that id, nothing in
///   this run could have produced the finding. If the pack IS loaded, this returns `None` even for a
///   nonexistent rule inside it (`sql/definitely-not-a-rule`) — stated rather than hidden, because the
///   alternative is a claim this layer has no evidence for.
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
    let loaded: Vec<&str> = output_view["packsLoaded"]
        .as_array()
        .map(|packs| {
            packs
                .iter()
                .filter_map(|p| p["id"].as_str())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    match rule.split_once('/') {
        None => {
            if zzop_facade::native_analysis_ids()
                .iter()
                .any(|id| id == rule)
            {
                return None;
            }
            Some(format!(
                "the `rule` filter names `{rule}`, which is not a native analysis id — and a DSL rule's \
                 id is always `<pack>/<rule>`, so no finding can ever match it. This reply's `shown: 0` \
                 is the filter, not a clean result. The `rule-catalog` contract document lists every id \
                 this build ships."
            ))
        }
        Some((pack, _)) => {
            if loaded.contains(&pack) {
                return None;
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

    /// The ONE-SIDED half, pinned so a later widening has to come here and say so: a nonexistent rule
    /// inside a pack that IS loaded stays silent, because this layer cannot enumerate a loaded pack's
    /// rule ids and a claim without evidence is the defect this whole channel exists to avoid.
    #[test]
    fn a_nonexistent_rule_inside_a_loaded_pack_is_deliberately_not_reported() {
        let view = serde_json::json!({ "packsLoaded": [{ "id": "sql" }] });
        assert_eq!(
            unknown_rule_filter_warning(&view, "sql/definitely-not-a-rule"),
            None
        );
    }

    /// A real, loaded rule id must never warn — the non-vacuity leg. Without it the two tests above
    /// pass just as well on a function that reports everything.
    #[test]
    fn a_loaded_packs_rule_id_is_silent() {
        let view = serde_json::json!({ "packsLoaded": [{ "id": "sql" }] });
        assert_eq!(unknown_rule_filter_warning(&view, "sql/nplus1"), None);
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
}
