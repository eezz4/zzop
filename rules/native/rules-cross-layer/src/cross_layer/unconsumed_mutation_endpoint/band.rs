//! The consume-side band for `unconsumed-mutation-endpoint`, and the sentence that explains it.
//!
//! Its own module because the two are ONE decision that must never drift apart: the band says how much
//! to trust an "unconsumed" verdict, and the sentence says which witness moved it. Three witnesses feed
//! it and they are not interchangeable — a ratio witness saw call sites it could not resolve, a
//! zero-contribution witness saw nothing at all, an untraced-client witness saw an import of a client
//! this build cannot read. Merging them would make the band right and the sentence false, which is the
//! shape review ledger V63 and V82 both caught.

use std::collections::BTreeSet;

use zzop_core::Severity;

/// Returns the run-level band and the sentence naming the witnesses that decided it.
pub(super) fn decide(
    blind_sources: &BTreeSet<String>,
    silent_blind_sources: &BTreeSet<String>,
    untraced_blind_sources: &BTreeSet<String>,
) -> (Severity, String) {
    // Run-level, not per-provide: "is this run's consume side blind at all" is the question, since a blind
    // source ANYWHERE in the run is a plausible unseen caller of ANY write route regardless of which tree
    // provides it (see this rule's module doc's "Confidence downgrade" section).
    let run_severity = if blind_sources.is_empty()
        && silent_blind_sources.is_empty()
        && untraced_blind_sources.is_empty()
    {
        Severity::Warning
    } else {
        Severity::Info
    };
    // Both branches speak. The empty branch used to be silent, which left warning severity reading as a
    // proof of completeness ("no blindness detected => the caller set was resolved") — the exact
    // class-extrapolation the non-empty branch exists to avoid. The check that did not fire is narrow, so
    // its silence is named rather than inferred (`output-philosophy.md` §0).
    let confidence_note = if blind_sources.is_empty()
        && silent_blind_sources.is_empty()
        && untraced_blind_sources.is_empty()
    {
        " Severity is warning because NONE of the three consume-side blindness checks fired on this \
         run: no source has majority-unresolved `http` consumes, none contributed zero joinable io \
         while its own files went mostly unread, and none imports a client this build cannot read. \
         Three checks that did not fire mean no blindness was WITNESSED, not that the caller set was \
         proven complete — a caller can still sit in a repo this run was not given."
            .to_string()
    } else {
        // Both mechanisms name sources, and they name them for OPPOSITE reasons — one saw call sites it
        // could not resolve, the other saw nothing at all. The sentence says which, per source, because a
        // reader who is told the wrong one goes looking for unresolved URLs that do not exist.
        let describe = |set: &BTreeSet<String>| -> String {
            let names: Vec<String> = set.iter().take(3).map(|s| format!("`{s}`")).collect();
            let more = set.len() - names.len();
            let more_note = if more > 0 {
                format!(", and {more} more")
            } else {
                String::new()
            };
            format!("{}{more_note}", names.join(", "))
        };
        let mut clauses: Vec<String> = Vec::new();
        if !blind_sources.is_empty() {
            clauses.push(format!(
                "source(s) {} have majority-unresolved `http` consumes (see \
                 `cross-layer/unresolved-consume-ratio`)",
                describe(blind_sources)
            ));
        }
        if !silent_blind_sources.is_empty() {
            clauses.push(format!(
                "source(s) {} contributed NO joinable io at all while most of their own files went \
                 unread by any structural parser — the join never saw their call sites to resolve",
                describe(silent_blind_sources)
            ));
        }
        if !untraced_blind_sources.is_empty() {
            clauses.push(format!(
                "source(s) {} route their calls through a client/SDK package this build cannot \
                 read, so the join never saw the call sites at all (see `cross-layer/\
                 untraced-client-import-no-visible-consume`)",
                describe(untraced_blind_sources)
            ));
        }
        format!(
            " This run's consume side is partly blind — {} — so severity here is reduced to info: \
             \"unconsumed\" cannot be trusted as a confident zero, and this write endpoint may well be \
             called from a caller this run could not see. Confirm before treating it as attack surface.",
            clauses.join("; and ")
        )
    };
    (run_severity, confidence_note)
}
