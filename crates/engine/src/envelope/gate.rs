//! The admission gate one overlay passes before anything merges — and everything this run says about
//! it on the way through. Split from `overlay.rs` for the file-size guard; the seam is real, since the
//! apply loop past this point only mutates artifacts and never re-judges the overlay.

use zzop_core::NormalizedEnvelope;

/// Judges one overlay and reports. Returns `true` when it should be applied.
///
/// BOTH axes come off ONE deserialize, which is what `validate_envelope_verdict` exists for (see its
/// doc): a rejection skips the overlay, and an acceptance still carries the advisory hints into
/// `warnings`. The overlay-lane narrative for why the advice half must reach this run at all lives in
/// [`super::overlay::apply_adapter_overlays`]'s doc.
///
/// Hints ride ONLY for an accepted overlay, unlike the authoring surface where they ride beside a
/// rejection too. A hint names the consequence its shape produces once applied; nothing was applied
/// here, so that sentence would describe something that did not happen — and `zzop_core`'s `hints`
/// module says why that is worse than silence (an overstated consequence teaches a producer to
/// distrust the whole pass).
pub(super) fn admit(overlay: &NormalizedEnvelope, warnings: &mut Vec<String>) -> bool {
    let json = match serde_json::to_string(overlay) {
        Ok(j) => j,
        Err(e) => {
            warnings.push(format!(
                "adapter overlay '{}' skipped: failed to serialize for validation: {e}",
                overlay.parser
            ));
            return false;
        }
    };
    let verdict = zzop_core::validate_envelope_verdict(&json);
    if let Err(issues) = verdict.result {
        let detail = issues
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join("; ");
        warnings.push(format!(
            "adapter overlay '{}' skipped: {detail}",
            overlay.parser
        ));
        return false;
    }
    for hint in verdict.hints {
        warnings.push(format!(
            "adapter overlay '{}' was applied, but: {hint}",
            overlay.parser
        ));
    }
    true
}
