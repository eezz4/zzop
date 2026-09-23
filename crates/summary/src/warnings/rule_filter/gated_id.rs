//! The refusal for an id this build REGISTERS and no finding can carry — carved out of the parent on
//! 2026-09-05 when adding it put that file over the 300-line cap.
//!
//! The seam is the parent's own: it decides WHICH reading applies, and each arm owns the sentence that
//! reading needs. This arm is the one whose subject is not the reply at all — it needs no `packsLoaded`,
//! no pack ids, nothing from the run — only the id the caller typed and the build's own registration.
//! That independence is what makes it the cheapest arm to lift, and it is also why its test can drive
//! off the published set instead of a fabricated view.

/// `Some(warning)` when `rule` names an id that gates a pass rather than reporting one, `None`
/// otherwise. See [`zzop_facade::ids_that_carry_no_finding`] for why the registry cannot answer this
/// and what the two classes are.
pub(super) fn gated_id_refusal(rule: &str) -> Option<String> {
    if !zzop_facade::ids_that_carry_no_finding()
        .iter()
        .any(|id| id == rule)
    {
        return None;
    }
    Some(format!(
        "the `rule` filter names `{rule}`, which is a real id this build registers — but it GATES \
         a pass rather than reporting one, so no finding is ever keyed by it and this reply's \
         `shown: 0` is the filter rather than a clean result. Two shapes ride this class and they \
         send you to different places: an id gating a SCORE computation emits no finding at all \
         (its output is the score surfaces, not `findings`), while a family gate's findings carry \
         the finer per-issue id built at finding time — `schema/<label>` for the schema families, \
         which is what to filter on instead. `findings.byRule` in an unfiltered run of this same \
         tree lists every id that CAN appear here, and the `rule-catalog` contract document lists \
         every id this build ships. The id is still valid where it is meant to be used: config \
         `rules: {{ \"{rule}\": \"off\" }}` speaks this space — the same spelling every finding's own \
         disable hint carries — and disabling it switches its whole pass off."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// This function's own claim, tested against the published set rather than a spelled list: an id
    /// added to either class (a new score gate, a new family) is covered the day it lands instead of
    /// the day someone remembers this test. What the whole filter then DOES with the refusal is the
    /// parent's test — this one owns only the sentence.
    #[test]
    fn every_published_gate_id_is_refused_and_told_where_findings_are() {
        let gates = zzop_facade::ids_that_carry_no_finding();
        assert!(
            !gates.is_empty(),
            "nothing to test — the facade stopped publishing the class this arm exists for"
        );
        for id in &gates {
            let w = gated_id_refusal(id).unwrap_or_else(|| {
                panic!("`{id}` is published as carrying no finding, yet is not refused")
            });
            assert!(
                w.contains(id.as_str()),
                "the message must name the id the reader typed: {w}"
            );
            assert!(
                w.contains("shown: 0") && w.contains("rather than a clean result"),
                "the message must say what the empty list actually means: {w}"
            );
            assert!(
                w.contains("schema/<label>") && w.contains("byRule"),
                "the message must point at the id space that DOES carry findings, and at the field \
                 listing it: {w}"
            );
            // The sentence sends the reader to a knob, so the knob has to be spellable. Until
            // 2026-09-05 this arm said `rules.disabled`, which is not a config key in any dialect
            // — `packs.disabled` exists and `rules` takes a per-id severity token, and the two got
            // crossed. A refusal that hands out an unspellable remedy is worse than a bare refusal:
            // the reader edits their config, nothing changes, and the reply already told them the
            // empty list was the filter's doing.
            assert!(
                w.contains(&format!("`rules: {{ \"{id}\": \"off\" }}`")),
                "the remedy must be the one dialect config actually accepts, spelled for THIS id: {w}"
            );
        }
    }

    /// The other half of the predicate, and the direction that would break a working command line: an
    /// id findings really do carry must fall through untouched.
    #[test]
    fn a_reportable_id_is_not_refused_here() {
        assert_eq!(gated_id_refusal("schema/god-model"), None);
        assert_eq!(gated_id_refusal("sql/nplus1"), None);
    }
}
