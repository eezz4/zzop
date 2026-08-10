//! The merge's loss self-reports, as pure string builders.
//!
//! Split out of `overlay.rs` when that file crossed the 300-line ceiling `check-max-file-lines`
//! enforces. These were the natural cut: everything else in the apply loop MUTATES artifacts, while
//! these only read a collected list and produce prose. Keeping them together also keeps the set legible
//! as a set — every one of them says "this merge dropped something", and a reader who finds one should
//! not have to discover the others. [`loss_warnings`] is the single entry point for exactly that reason:
//! a new loss direction cannot be collected and then left unreported.

use super::super::merge::{DroppedOverlayBinding, FragmentCollision, MergeLosses, Tombstone};

/// Pushes every loss report this merge produced, in a fixed order. The apply loop calls only this — see
/// the module doc.
pub(super) fn loss_warnings(
    source: &str,
    parser: &str,
    losses: &MergeLosses,
    warnings: &mut Vec<String>,
) {
    warnings.extend(displacement_warning(source, parser, &losses.tombstones));
    warnings.extend(overruled_warning(source, parser, &losses.dropped));
    warnings.extend(fragment_collision_warning(
        source,
        parser,
        &losses.collisions,
    ));
}

/// G9 — every native fact this overlay DISPLACED via a declared `overrides` entry.
///
/// An override is the only overlay operation that REMOVES a fact the engine extracted itself, so it is
/// the only one whose damage is invisible in the output: the native binding is simply gone. Each
/// displacement is named with BOTH sides, so the judgment can be re-derived and disagreed with — the
/// same stance `zzop_core::registry::redact` takes when it marks an excluded evidence path instead of
/// deleting it. A bare count would not do: a number nobody can trace back is not a disclosure by this
/// repo's "an agent must not have to notice" rule.
///
/// NOT CAPPED, deliberately. The list length is bounded by what the adapter itself declared — every
/// entry required an explicit `overrides` name that passed validation — so there is no runaway to guard
/// against, and capping would re-import the truncation-disclosure machinery `output-philosophy` §13
/// removed for exactly this shape of list.
pub(super) fn displacement_warning(
    source: &str,
    parser: &str,
    tombstones: &[Tombstone],
) -> Option<String> {
    if tombstones.is_empty() {
        return None;
    }
    let detail = tombstones
        .iter()
        .map(|t| {
            format!(
                "{} '{}': ours \"{}\" -> theirs \"{}\"",
                t.path, t.local_name, t.native_specifier, t.overlay_specifier
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    Some(format!(
        "adapter overlay \"{source}\" (parser {parser}) DISPLACED {} natively-parsed import \
         binding(s) it declared in `overrides`: {detail}. Those native facts are no longer in this \
         run's output — this line is the only record that they were extracted at all. Nothing \
         verifies that the adapter is the correct side; if these replacements look wrong, the adapter \
         is asserting something the engine could not check.",
        tombstones.len()
    ))
}

/// G10 — the mirror of [`displacement_warning`]: bindings this overlay OFFERED that the native side
/// outranked, because it had already bound the same local name to a different specifier and the overlay
/// declared no override for it.
///
/// Native-first is the correct default, but dropping the adapter's side in silence is the same defect
/// as dropping ours, pointing the other way: an author who misspells or forgets a declaration otherwise
/// gets a run indistinguishable from success. Restated-identical bindings are agreement, not loss, and
/// never reach here (see `DroppedOverlayBinding`).
pub(super) fn overruled_warning(
    source: &str,
    parser: &str,
    dropped: &[DroppedOverlayBinding],
) -> Option<String> {
    if dropped.is_empty() {
        return None;
    }
    let detail = dropped
        .iter()
        .map(|d| {
            format!(
                "{} '{}': ours \"{}\" kept, theirs \"{}\" dropped",
                d.path, d.local_name, d.native_specifier, d.overlay_specifier
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    Some(format!(
        "adapter overlay \"{source}\" (parser {parser}): {} import binding(s) it offered were DROPPED \
         because the native parser had already bound the same local name to a different specifier and \
         the overlay declared no override for it: {detail}. Parsed facts win by default. If the \
         adapter is the correct side, name those local names in the projection's `overrides.imports` \
         (which requires the envelope to declare version {min_version}) — that displaces the native \
         binding and reports what it displaced.",
        dropped.len(),
        min_version = zzop_core::MIN_VERSION_FOR_OVERRIDES,
    ))
}

/// G11 — every file whose router-composition fragments this overlay described on top of a producer that
/// had already described them (see [`FragmentCollision`] and its module for the mechanism).
///
/// This is the only merge loss whose victims are facts NEITHER side offered twice: two correct
/// descriptions of one file make that file's routers ambiguous to the by-name composers, which then walk
/// less of the graph than either description alone would have. The output is a route count that went
/// DOWN when an adapter was added, with nothing else changed — so without this line an author cannot
/// tell a route from a deleted one.
///
/// The wording says what to DO, because there is no engine-side remedy to offer: `overrides` covers
/// `imports` only, so an adapter has no way to declare its fragments authoritative, and the engine will
/// not pick a side it cannot verify. The fix is the adapter's — describe the channel, or the file, that
/// the other producer does not.
///
/// NOT CAPPED, on the same reasoning as [`displacement_warning`]: the list is bounded by the overlay's
/// own declared `files[]`, and a count nobody can trace back to a path is not a disclosure.
fn fragment_collision_warning(
    source: &str,
    parser: &str,
    collisions: &[FragmentCollision],
) -> Option<String> {
    if collisions.is_empty() {
        return None;
    }
    let detail = collisions
        .iter()
        .map(|c| {
            format!(
                "{} ({}): ours [{}] + theirs [{}]",
                c.path,
                c.channel,
                c.ours.join(", "),
                c.theirs.join(", ")
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    Some(format!(
        "adapter overlay \"{source}\" (parser {parser}): {} file(s) already carried \
         router-composition fragments that this overlay ALSO describes: {detail}. Both descriptions \
         enter ONE by-name composition graph, where a fragment is a root only if no mount anywhere \
         names it and an alias mount resolves to the target file's SOLE fragment — a file described \
         twice satisfies neither, so mounts below it resolve to nothing and their whole subtree emits \
         no route. Fewer routes can come out than either producer alone would have produced; compare \
         `census.ioProvides` with and without this overlay. The engine cannot choose a side (`overrides` \
         covers `imports` only) and will not guess: drop these files from the overlay's `files[]`, or \
         emit only the channels the other producer left empty.",
        collisions.len()
    ))
}
