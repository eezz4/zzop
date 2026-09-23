//! THE BY-ID LANE — the one of the three message mechanisms that points OUT of the reply.
//!
//! The other two deduplicate: [`super::fold_exact`] and [`super::template`] move a text into a sibling
//! table and leave an address behind, so every byte is still in the reader's hands. This one removes
//! the text entirely, because the reader can rebuild it from a field they already hold — the finding's
//! own `ruleId` — through `zzop explain <id>` or the MCP `zzop://rule/{id}` resource.
//!
//! # Why that difference is the whole design
//! Resolving a `messageBy` pointer costs a SECOND REQUEST. `ruleMessages` promises the opposite in so
//! many words ("every byte is reachable without a second request"), and a reader holding one reply
//! cannot tell two square-bracket markers apart by looking. So this lane carries its own legend, states
//! the difference in it, and — the part an external review had to teach us — publishes a FIELD rather
//! than asking anyone to read the sentence.
//!
//! Split out of `mod.rs` when that file crossed the repo's 400-line source cap. The seam is the same
//! one `template/` uses: one module per message mechanism, with `mod.rs` owning only what composes them.

/// The per-finding field that says a `message` is a POINTER resolved by the finding's own `ruleId`.
/// Its VALUE names the resolver, and its presence is the signal — the same contract [`super::MESSAGE_REF`]
/// and [`super::TEMPLATE_PARTS`] keep, and no finding ever carries more than one of the three.
///
/// # Why this field exists at all, when the sentence already begins `[by-id]`
/// Because this repo already decided that a lane discriminated only by WORDING is built on the half
/// of the contract that is free to move. `scripts/measure/resolve-folded-message.mjs` says it in so
/// many words — *"Resolve through the FIELD, never by pattern-matching `message`. VERSIONING.md puts
/// exact message wording explicitly OUTSIDE the compatibility surface, so a consumer that scraped the
/// pointer sentence would be built on the half that is free to move."* The first version of this lane
/// shipped with no field, so the prefix was the only signal a consumer could key on; an external
/// review (round 18) named it as the one IRREVERSIBLE defect in the change, because 1.0 would have
/// frozen the sentence by making consumers depend on it — and the absence guarantee ("`message` is
/// prose unless a sibling field says otherwise") could not have been restored before 2.0.
pub(super) const MESSAGE_BY: &str = "messageBy";

/// The one value [`MESSAGE_BY`] takes today: resolve by the finding's own `ruleId`. A value rather
/// than a bare `true` so a second resolver can be added without a second field — and so the field
/// says what to DO, not merely that something happened.
pub(super) const MESSAGE_BY_RULE_ID: &str = "ruleId";

/// The sentence that replaces prose a reader can rebuild from the finding's own `ruleId`
/// (`output-philosophy.md` §3.5). Terse on purpose: [`MESSAGE_BY`] beside it carries the signal and
/// the resolver, and [`MESSAGE_BY_ID_MEANING`] carries the explanation once per reply, so restating
/// either here would be the per-finding repetition this whole lane exists to remove. It still says
/// something true about itself and names where its own explanation is, which is the rule
/// `docs/modules/facade.md` states for a pointer message.
pub(super) const BY_ID_MESSAGE: &str = "[by-id] resolve by ruleId; see messageByIdMeaning";

/// The legend for the pointer a finding carries when its rule declared that text VERBATIM and the
/// facade left it to `zzop explain` (`zzop_facade::BY_ID_MESSAGE`, `output-philosophy.md` §3.5).
///
/// # Why this one is INLINE while four repo-invariant legends are folded to a pointer
/// The other four say what a field MEANS. This one says that bytes a reader used to receive are no
/// longer in their hands, and where to go for them. Putting the disclosure of a lossy change behind a
/// second fetch is the failure it exists to prevent, so it pays its bytes every reply that fires it.
///
/// # Why it states the difference from the fold rather than assuming a reader knows
/// `ruleMessages` promises every byte is reachable WITHOUT a second request. That promise is still
/// true of the fold and false of this, and a reader holding one reply cannot tell two square-bracket
/// markers apart by looking. So the difference is spelled here, next to the marker it applies to.
pub(super) const MESSAGE_BY_ID_MEANING: &str =
    "A finding carrying `messageBy` is holding a POINTER in `message`, not prose. Detect it by that \
     FIELD, never by the sentence: the sentence is wording and may be reworded, the field is the \
     contract. Its value names the resolver, and the only value today is `ruleId`: the rule declared \
     that text word for word, so every byte of it is rebuildable from the finding's own `ruleId` and \
     none of it is repeated here. Run `zzop explain <ruleId>`, or read the MCP resource \
     `zzop://rule/<ruleId>`; both return the same bytes, and both carry the message, the suppress \
     marker and the disable knob that used to ride on the finding. This differs from `ruleMessages` \
     and `ruleMessageTemplates` in the one way that matters: a folded message is still inside this \
     reply, and a `messageBy` one is NOT, so resolving it costs a second request. No finding ever \
     carries more than one of `messageBy`, `messageRef` and `templateParts`. What is never affected \
     is which findings you were shown or what they counted toward — `truncated` remains the only key \
     that means rows were left out. Messages that are not rebuildable keep every byte inline and \
     never carry this field: native analyses, whose prose interpolates keys, sources and counts; any \
     finding whose text was rewritten to name a suppression comment the rule does not honour, since \
     that token was read out of the scanned source and no rule id can rebuild it; and any rule this \
     binary does not carry, because the two resolvers above answer only for rules compiled into it — \
     a pack loaded from `zzop/rules/` or `packs.extraDirs` keeps its prose for exactly that reason. \
     A rule whose whole message is cheaper than the pointer and its field also stays inline: the \
     gate is net bytes, the same one `ruleMessages` publishes, so a message left whole is a size \
     decision and never a sign that anything is missing.";

/// Replaces prose a reader can rebuild from the finding's own `ruleId` with [`BY_ID_MESSAGE`] plus
/// [`MESSAGE_BY`], and reports whether it fired. Runs BEFORE both folds, whose byte gates then see
/// pointers rather than prose and decline them.
///
/// # Why the population is BUNDLED rules and nothing else
/// The pointer tells a reader to resolve by `ruleId` through `zzop explain <id>` or the MCP
/// `zzop://rule/{id}` resource. Both answer out of the packs compiled into the binary and nothing
/// else, so a finding from a pack in `zzop/rules/` or `packs.extraDirs` would be pointed at a door
/// that answers "unknown rule id" — measured, round 18. `zzop_facade::bundled_verbatim_message` is
/// that exact corpus, which is why the question it answers is "can the door open for this id" rather
/// than the proxy "where did this pack come from" (`PackSource` cannot answer the latter — see that
/// function's doc).
///
/// # Why HERE and not at the facade, which is where it first shipped
/// `zzop_facade::analyze_trees_json` is a DATA producer with nine consumers in this crate — `endpoint`,
/// `file`, `facts`, `graph`, `manifest`, `coverage`, `cross`, `warnings` and this lane. Applying a
/// presentation transform inside it reached all nine: `endpoint`'s `relatedFindings` began text-matching
/// the pointer sentence (20 of 20 listed findings were pointers), and the `file` lane shipped the marker
/// with no legend beside it because it does not run this module. Both were round-18 findings. A reply
/// transform belongs in the reply builder, and this is it — the same module that owns the two fold
/// markers and, in [`Folded::publish`], the pairing of every marker with its legend.
///
/// The three entry points `VERSIONING.md` freezes as ONE row (`analyze`, `analyzeTrees`,
/// `analyzeEnvelope`) all funnel through this shaper, so they agree by construction. At the facade they
/// did not: envelope runs never got the pointer at all.
pub(super) fn point_at_rule_id(shown: &mut [serde_json::Value]) -> bool {
    let mut fired = false;
    for finding in shown.iter_mut() {
        let Some(rule_id) = finding.get("ruleId").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(verbatim) = zzop_facade::bundled_verbatim_message(rule_id) else {
            continue; // native, or a pack the door cannot open
        };
        if finding.get("message").and_then(|v| v.as_str()) != Some(verbatim) {
            continue; // a near-miss rewrite spliced a token from the reader's own source
        }
        // The same net-byte discipline the folds publish: the pointer plus its field costs bytes, so a
        // message cheaper than that pair is left whole.
        if verbatim.len() <= BY_ID_MESSAGE.len() + MESSAGE_BY.len() + MESSAGE_BY_RULE_ID.len() {
            continue;
        }
        finding["message"] = BY_ID_MESSAGE.into();
        finding[MESSAGE_BY] = MESSAGE_BY_RULE_ID.into();
        fired = true;
    }
    fired
}
