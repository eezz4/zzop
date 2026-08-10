//! The THIRD way a Mode B merge loses a fact — the one that is neither a displacement nor an
//! overrule, and the only one whose damage lands on facts NOBODY offered twice.
//!
//! `Tombstone` and `DroppedOverlayBinding` (see `super`) both record a collision the merge RESOLVED:
//! one side's value was kept, the other's discarded, and the loss is exactly the discarded value.
//! The router-composition fragment channels lose differently. Their merge is a bare `extend` with no
//! collision policy at all, because a fragment has no key the merge could compare on — and the loss
//! surfaces two phases later, inside a composer that can no longer see who produced what.
//!
//! ## Why an appended fragment can SUBTRACT
//! `analyze::compose_router_mount_provides` and `compose_trpc_provides` fold every file's fragments
//! into ONE by-name graph and walk it from its roots. Two of that graph's rules are per-FILE, and a
//! second producer breaks both:
//! - **Sole-fragment resolution.** A mount naming a child by its import alias (`from .api import
//!   router as api_router`) finds no fragment of that name in the target file, so it falls back to that
//!   file's SOLE fragment. A file described twice no longer has one — the mount resolves to nothing and
//!   the entire subtree beneath it goes silent.
//! - **Root exclusion by name.** A fragment is a root only when no mount ANYWHERE names it; the name
//!   set is global, so one producer's mounts exclude the other's fragments from ever being walked.
//!
//! Both rules are individually correct — refusing to guess between two same-named fragments is the same
//! conservatism as refusing to emit a mounted child at a truncated prefix — and neither can be relaxed
//! without inventing an answer. Nor can root selection simply be scoped per producer: the cross-producer
//! mount (a natively parsed parent mounting an overlay-supplied child) is precisely the composition Mode
//! B advertises, and scoping would forbid it. See `docs/recipes/write-an-adapter.md` for the measured
//! numbers (`examples/fastapi_overlay_adapter`: provides 19 -> 0 and 25 -> 2 on two FastAPI trees the
//! native Python parser already reads).
//!
//! So the engine keeps composing conservatively and SAYS SO. That is the same stance the third fragment
//! channel already takes on its own duplicate-description case: `analyze::compose::shape_merge` poisons a
//! class name declared with conflicting shapes and discloses it rather than picking one.
//!
//! ## Why this is recorded here and not in the composer
//! Provenance dies at the `extend`. After it, `artifact.router_mount_fragments` is one undifferentiated
//! pool and "which producer said this" is unrecoverable — the composer could see that a file has two
//! fragments but not that two PRODUCERS described it, and a file legitimately holding two routers is
//! ordinary. This seam is the last place both facts are still true at once.

use zzop_core::FileProjection;

/// One file whose router-composition fragments now come from two producers at once: something already
/// in the artifact (the native pass, or an earlier overlay in the same run) plus this projection.
///
/// Both sides' fragment NAMES are carried, not just a count, for the same reason
/// [`super::Tombstone`] carries both specifiers — a reader has to be able to re-derive the judgment and
/// disagree with it. Equal name lists mean the two producers describe the same routers; differing ones
/// still collide, because the damage above is per-FILE, not per-name.
pub(in crate::envelope) struct FragmentCollision {
    pub(in crate::envelope) path: String,
    /// The channel's wire-field name, minus the `_fragments` suffix (`router-mount` /
    /// `procedure-router`) — what the adapter author sees in their own envelope.
    pub(in crate::envelope) channel: &'static str,
    pub(in crate::envelope) ours: Vec<String>,
    pub(in crate::envelope) theirs: Vec<String>,
}

/// Every loss a Mode B merge can incur, collected per overlay and reported together by
/// `super::super::overlay::reports`. One struct rather than three `&mut Vec` parameters so the apply
/// loop cannot collect a direction and forget to disclose it — the defect each of these exists to
/// abolish.
#[derive(Default)]
pub(in crate::envelope) struct MergeLosses {
    pub(in crate::envelope) tombstones: Vec<super::Tombstone>,
    pub(in crate::envelope) dropped: Vec<super::DroppedOverlayBinding>,
    pub(in crate::envelope) collisions: Vec<FragmentCollision>,
}

/// Records a [`FragmentCollision`] for each name-composed fragment channel both sides carry. MUST be
/// called BEFORE the channel's `extend` — see the module doc.
///
/// `class_shape_fragments` is deliberately not covered: it is the third fragment channel but not a
/// by-name GRAPH — `analyze::compose::shape_merge` folds it into a `name -> shape` map that already has
/// its own conflict policy (identical redeclaration resolves, conflicting redeclaration poisons AND
/// discloses), so a duplicate there is either inert or already reported by its owner.
pub(in crate::envelope) fn record_fragment_collisions(
    artifact: &crate::pipeline::FileArtifact,
    projection: &FileProjection,
    losses: &mut MergeLosses,
) {
    let channels = [
        (
            "router-mount",
            names(&artifact.router_mount_fragments, |f| &f.name),
            names(&projection.router_mount_fragments, |f| &f.name),
        ),
        (
            "procedure-router",
            names(&artifact.procedure_router_fragments, |f| &f.name),
            names(&projection.procedure_router_fragments, |f| &f.name),
        ),
    ];
    for (channel, ours, theirs) in channels {
        if ours.is_empty() || theirs.is_empty() {
            continue;
        }
        losses.collisions.push(FragmentCollision {
            path: projection.path.clone(),
            channel,
            ours,
            theirs,
        });
    }
}

/// Sorted, deduplicated fragment names — the disclosure is prose a human reads, so it must not inherit
/// per-file source order, and a router described under one name twice is one name.
fn names<T>(frags: &[T], name: impl Fn(&T) -> &str) -> Vec<String> {
    let mut out: Vec<String> = frags.iter().map(|f| name(f).to_string()).collect();
    out.sort();
    out.dedup();
    out
}
