//! The two override-related self-reports, as pure string builders.
//!
//! Split out of `overlay.rs` when that file crossed the 300-line ceiling `check-max-file-lines`
//! enforces. These two were the natural cut: everything else in the apply loop MUTATES artifacts,
//! while these only read a collected list and produce prose. Keeping them together also keeps the
//! pair legible as a pair — they are the same disclosure pointed in opposite directions, and a reader
//! who finds one should not have to discover the other.

use super::super::merge::{DroppedOverlayBinding, Tombstone};

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
