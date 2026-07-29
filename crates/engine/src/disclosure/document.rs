//! The registry's two DERIVED views: its per-status tallies, and its full text as one Markdown
//! document. Split out of `disclosure.rs` (which keeps the registry DATA) when the two together pushed
//! that file past the 300-line cap — and the split line is the honest one: above is what zzop is blind
//! to, here is how that is delivered.
//!
//! Both views read `BLINDNESS_REGISTRY` and nothing else. That is the load-bearing property of the
//! 2026-07-29 fold, not a nicety: a run reply carries the TALLIES (so "gaps exist, and there are this
//! many" is still unmissable without asking) while the PROSE ships once, through the contract lane. If
//! the two could come from different places, a registry that grew while the counts sat still would turn
//! the fold into a lie.

use super::{DisclosureStatus, BLINDNESS_REGISTRY};

/// How many classes carry each status, in wire order — `(total, asserted, partial, notYetDetected)`.
/// The registry's SHAPE: what an analyze reply carries instead of the paragraphs.
pub fn disclosure_counts() -> (usize, usize, usize, usize) {
    let with = |status: DisclosureStatus| {
        BLINDNESS_REGISTRY
            .iter()
            .filter(|c| c.status == status)
            .count()
    };
    (
        BLINDNESS_REGISTRY.len(),
        with(DisclosureStatus::Asserted),
        with(DisclosureStatus::Partial),
        with(DisclosureStatus::NotYetDetected),
    )
}

/// The WHOLE registry as one Markdown document — the full text a run reply stopped shipping on
/// 2026-07-29 (it was ~10.6KB of byte-identical prose on every call: a fixed per-tool-call tax on the
/// AI agent this output is written for, and ~65% of a small tree's whole reply). The reply now carries
/// [`disclosure_counts`] plus a pointer to this document, which `zzop contract disclosure-classes` and
/// MCP `resources/read zzop://contract/disclosure-classes` both serve.
///
/// Rendered from `BLINDNESS_REGISTRY` on every call, never a committed copy of the same prose —
/// `document_carries_every_class_verbatim` below is the seal, and the fold's own end-to-end pin lives
/// in `crates/summary/tests/disclosure_fold.rs`.
///
/// What is deliberately NOT here: the RUN-VARYING disclosure channels — `coverage.joinContributionZero`,
/// the per-tree `coverage` census, the `warnings` self-reports. They stay in the reply, and they are why
/// folding this static registry does not remove a given run's own disclosure.
pub fn disclosure_contract_text() -> String {
    let (classes, asserted, partial, not_yet) = disclosure_counts();
    let mut out = String::new();
    out.push_str("# zzop coverage disclosure — the silent-failure-class registry\n\n");
    out.push_str(
        "Every way zzop's own output can be silently MISREAD, and how completely zzop detects each one \
         today. This is zzop's meta-honesty channel: it never pretends to be silently complete, so even \
         an unknown-unknown leaves the holes in zzop's own disclosure visible.\n\n",
    );
    out.push_str(&format!(
        "{classes} classes: {asserted} asserted, {partial} partial, {not_yet} notYetDetected.\n\n",
    ));
    out.push_str(
        "Every analyze-shaped reply carries these numbers (`disclosure.classes` and the per-status \
         counts) plus the pointer that brought you here — not these paragraphs, which are identical on \
         every run. What does VARY per run stays in the reply and is not repeated here: \
         `coverage.joinContributionZero`, the per-tree `coverage` census, and the `warnings` \
         self-reports.\n\n",
    );
    out.push_str("## Status vocabulary\n\n");
    out.push_str(
        "An honest snapshot of SHIPPED detection, never of the design's aspiration — a class the design \
         intends to assert but has not implemented yet reads `notYetDetected` here.\n\n",
    );
    out.push_str(
        "- `asserted` — asserted from a structural fact on every run; cannot be silently missed.\n",
    );
    out.push_str(
        "- `partial` — detected in the common cases, but a member of the class can still slip past (a \
         heuristic, or a signal folded into a coarser count).\n",
    );
    out.push_str(
        "- `notYetDetected` — a real failure class zzop does NOT yet detect, declared precisely so you \
         do not assume coverage that does not exist.\n\n",
    );
    let mut group = "";
    for class in BLINDNESS_REGISTRY {
        if class.group != group {
            group = class.group;
            out.push_str(&format!("## Group: {group}\n\n"));
        }
        out.push_str(&format!(
            "### {} — `{}`\n\n{}\n\n",
            class.id,
            class.status.as_str(),
            class.summary
        ));
    }
    out.push_str(
        "Every id above is also answerable one at a time, with the CLI binary: `zzop explain <class-id>` \
         names the class, its group and its status (and `zzop explain <group>` lists a group's members). \
         `zzop-mcp` has no `explain` — this document, served as MCP resource \
         `zzop://contract/disclosure-classes`, already carries every id's class, group and status, which \
         is what `explain` would print.\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// THE SEAL. Add a class to the registry and this document grows with it — id, group, status token
    /// and the full summary paragraph, verbatim. Asserted here, in the crate that owns the registry,
    /// because this is the half that makes folding legitimate: the run reply is allowed to ship only the
    /// counts precisely because the text they summarize is complete and one lookup away.
    #[test]
    fn document_carries_every_class_verbatim() {
        let text = disclosure_contract_text();
        for class in BLINDNESS_REGISTRY {
            assert!(text.contains(class.id), "document omits id {:?}", class.id);
            assert!(
                text.contains(class.group),
                "document omits {:?}'s group",
                class.id
            );
            assert!(
                text.contains(class.status.as_str()),
                "document omits {:?}'s status token",
                class.id
            );
            assert!(
                text.contains(class.summary),
                "document omits {:?}'s summary — a shortened restatement is exactly the second, \
                 driftable copy the fold is not allowed to have",
                class.id
            );
        }
    }

    /// The tallies the document STATES must be the tallies it was built from — the number a reader who
    /// followed the reply's pointer checks the reply's own counts against.
    #[test]
    fn document_states_the_same_tallies_the_reply_carries() {
        let (classes, asserted, partial, not_yet) = disclosure_counts();
        let text = disclosure_contract_text();
        assert!(text.contains(&format!(
            "{classes} classes: {asserted} asserted, {partial} partial, {not_yet} notYetDetected."
        )));
        assert_eq!(
            asserted + partial + not_yet,
            classes,
            "a status outside {{asserted, partial, notYetDetected}} would be uncounted by every folded \
             reply while `classes` kept growing"
        );
        assert_eq!(classes, BLINDNESS_REGISTRY.len());
    }

    /// Every group gets exactly one heading, in registry order — the document is grouped, not one flat
    /// list, and a regrouped class must not silently split its group's section in two.
    #[test]
    fn every_group_heads_exactly_one_section() {
        let text = disclosure_contract_text();
        let groups: std::collections::BTreeSet<&str> =
            BLINDNESS_REGISTRY.iter().map(|c| c.group).collect();
        for group in groups {
            let heading = format!("## Group: {group}\n");
            assert_eq!(
                text.matches(&heading).count(),
                1,
                "group {group:?} must head exactly one section — the registry's own order groups its \
                 classes, so two sections means a class was declared out of its group's run"
            );
        }
    }
}
