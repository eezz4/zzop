//! The provide-side tripwire trio: S1 (controller decorators), S2 (server-framework imports) and S16
//! (partial per-file silence). One question, asked three ways: **does this tree serve http routes that
//! zzop cannot see?**
//!
//! They sit together because of what they SHARE, not because they are adjacent in `warnings.rs`: S1 and
//! S2 are the only tripwires in the family that re-read `candidate_rels` from disk, and all three feed
//! the single `provide_side_alarm` bit that S15 rides at the end of the phase. Extracted 2026-09-06 when
//! S2 grew a fourth argument and pushed `warnings.rs` past the 300-line cap — the seam was chosen so the
//! split is a boundary rather than a line count.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Runs S1, S2 and S16 in that order, pushing each warning that fired onto `warnings`.
///
/// Returns whether ANY of them fired — the `provide_side_alarm` bit. S15 reads it rather than gating
/// itself, because "the provide channel is empty" and "an empty provide channel here is a surprise" are
/// different judgments and these three own the second one.
///
/// `visible_out` receives the lexically-visible route-registration count, but ONLY when that scan ran —
/// otherwise it is left untouched, and `None` keeps meaning "never measured" to both of its readers. It
/// is an out-param rather than a second return value because the other reader is nowhere near here: the
/// run-wide provide-blind severity gate in `cross_layer_findings` judges the same population and needs
/// the same number, and this is the only scope holding `root` and the candidate list. Same shape and the
/// same reason as `callgraph::run_callgraph_rules`' `decorator_guarded_out`.
pub(super) fn provide_side_warnings(
    root: &Path,
    candidate_rels: &[String],
    package_import_files: &BTreeMap<String, BTreeSet<String>>,
    io_provides: &[zzop_core::IoProvide],
    http_count: usize,
    warnings: &mut Vec<String>,
    visible_out: &mut Option<usize>,
) -> bool {
    let mut provide_side_alarm = false;
    if let Some(w) =
        crate::framework_silence::controller_silence_warning(root, candidate_rels, http_count)
    {
        provide_side_alarm = true;
        warnings.push(w);
    }

    // S2 — server-framework import tripwire (provide side): a server-framework package import present
    // while extracted `http` provides stay near-zero (closes the method-call registration idiom S1's
    // decorator regex cannot see). Additive to S1 above; both may fire. A map lookup on the ordinary
    // path; below the floor it re-reads `candidate_rels` to MEASURE the gap instead of asserting one —
    // its doc has why the floor alone was wrong, and why that measurement may only silence, never speak.
    // Scanned only under S2's own precondition — below the floor, with a server framework imported —
    // so an ordinary tree still pays nothing and the one scan serves both readers. A tree that fails the
    // precondition leaves `visible_out` at `None` and passes 0 here, which S2's `visible > 0` guard
    // treats as "no measurement": an unmeasured tree must never read as a cleared one.
    let visible = if crate::framework_silence::needs_visible_scan(package_import_files, http_count)
    {
        let n = crate::framework_silence::visible_route_registrations(root, candidate_rels);
        *visible_out = Some(n);
        n
    } else {
        0
    };
    if let Some(w) = crate::framework_silence::server_framework_import_warning(
        package_import_files,
        http_count,
        visible,
    ) {
        provide_side_alarm = true;
        warnings.push(w);
    }

    // S16 — PARTIAL-silence tripwire (provide side). Every sibling above asks a tree-wide question and
    // can therefore only see a tree with almost NO routes; this one asks per FILE, which is the shape a
    // partial gap takes — and partial is the common one. It stays quiet when the tree extracted nothing
    // (S1/S2 own that case and name the framework), so the two cannot double-report one silence.
    // Judged on the NATIVE count for the same reason S1/S2 are: an overlay answers "does this tree have
    // routes", not "can zzop see this framework".
    let provide_files: std::collections::BTreeSet<String> = io_provides
        .iter()
        .filter(|p| p.kind == "http")
        .map(|p| p.file.clone())
        .collect();
    if let Some(w) = crate::framework_silence::partial_route_silence_warning(
        package_import_files,
        &provide_files,
        http_count,
    ) {
        provide_side_alarm = true;
        warnings.push(w);
    }

    provide_side_alarm
}
