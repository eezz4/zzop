//! Which facts in this run zzop EXTRACTED, and which a caller DECLARED.
//!
//! Merging an adapter overlay is lossy in exactly one direction: an overlay's `IoProvide` lands in an
//! artifact and is thereafter indistinguishable from one a parser read out of source. That is the right
//! design for everything downstream — a rule, a join, an `endpoint` answer should not care where a true
//! fact came from — and it is wrong for exactly one reader: the person deciding how much of this report
//! is zzop's own sight.
//!
//! Two measured consequences, both from the same successful overlay run. First, the adapter's `parser`
//! id appeared NOWHERE in a successful output — only in failures — so `coverage` reported `provides: 6`
//! with no hint that all six were handed to it. Second, and worse: the honest warning that sent the
//! author to write the adapter in the first place ("a server framework is imported and zero routes were
//! extracted") went SILENT afterwards, because the tripwire counts http provides without asking where
//! they came from. A reader handed only the after-run would conclude zzop parses this framework
//! natively. It does not, and the next unadapted tree in that stack will be read as clean.
//!
//! The fix has two halves and they belong together, which is why one module holds both: the tripwires
//! judge on the NATIVE count ([`native_http_provides`] / [`native_http_consumes`]), and the run states
//! the declared half out loud ([`overlay_provenance_warning`]).

use std::collections::BTreeMap;

use crate::envelope::OverlayIoCounts;

/// The http provides this run EXTRACTED, out of `total` merged — the count every provide-side
/// framework-silence tripwire must judge on. Saturating, because the merged total is the ground truth
/// and a dedup (an overlay re-emitting a fact the native pass already found) can make the overlay's own
/// tally exceed what it actually added; under-reporting the native half would fire a tripwire falsely,
/// while this direction at worst leaves it as quiet as before.
pub(crate) fn native_http_provides(
    total: usize,
    overlay: &BTreeMap<String, OverlayIoCounts>,
) -> usize {
    total.saturating_sub(overlay.values().map(|c| c.http_provides).sum::<usize>())
}

/// The consume-side twin of [`native_http_provides`], for the same reason.
pub(crate) fn native_http_consumes(
    total: usize,
    overlay: &BTreeMap<String, OverlayIoCounts>,
) -> usize {
    total.saturating_sub(overlay.values().map(|c| c.http_consumes).sum::<usize>())
}

/// Names each accepted overlay and what it contributed. `None` when no overlay contributed anything —
/// a run with no adapters says nothing, and an overlay that carried only `attributes`/`is_entry` has no
/// io provenance to disclose (its effects are visible as the findings they clear).
pub(crate) fn overlay_provenance_warning(
    overlay: &BTreeMap<String, OverlayIoCounts>,
    total_http_provides: usize,
) -> Option<String> {
    let contributors: Vec<(&String, &OverlayIoCounts)> =
        overlay.iter().filter(|(_, c)| c.total() > 0).collect();
    if contributors.is_empty() {
        return None;
    }
    let per_parser = contributors
        .iter()
        .map(|(parser, c)| format!("`{parser}` ({})", contribution(c)))
        .collect::<Vec<_>>()
        .join(", ");
    let declared_http: usize = contributors.iter().map(|(_, c)| c.http_provides).sum();
    // The sentence that carries the weight: a share, because "6 provides came from an adapter" reads
    // very differently at 6 of 6 than at 6 of 200. The 6-of-6 case is the one where a reader would
    // otherwise conclude the framework is natively supported.
    let share = if declared_http > 0 {
        format!(
            " Of this tree's {total_http_provides} http route(s), {declared_http} came from an \
             overlay rather than from source zzop parsed{}.",
            if declared_http >= total_http_provides {
                " — every one of them, so this tree's route visibility is entirely the adapter's, and \
                 zzop's own extractors read no routes here at all"
            } else {
                ""
            }
        )
    } else {
        String::new()
    };
    Some(format!(
        "adapter overlay facts are part of this analysis: {per_parser}.{share} These are facts the \
         CALLER declared, not facts zzop extracted, and once merged nothing downstream can tell them \
         apart — a finding, a join edge or an `endpoint` answer resting on them is exactly as good as \
         the adapter that supplied them. This entry is the only place the distinction survives, so a \
         reader deciding how much of this report is zzop's own sight should read it here rather than \
         inferring it from the counts."
    ))
}

/// "3 http route(s), 1 http call(s)" — only the non-zero parts, so a provide-only adapter reads as one
/// clause instead of four with three zeros in them.
fn contribution(c: &OverlayIoCounts) -> String {
    let mut parts = Vec::new();
    if c.http_provides > 0 {
        parts.push(format!("{} http route(s)", c.http_provides));
    }
    if c.http_consumes > 0 {
        parts.push(format!("{} http call(s)", c.http_consumes));
    }
    if c.other_provides > 0 {
        parts.push(format!("{} other provide(s)", c.other_provides));
    }
    if c.other_consumes > 0 {
        parts.push(format!("{} other consume(s)", c.other_consumes));
    }
    parts.join(", ")
}

#[cfg(test)]
mod tests;
