//! The SINGLE-TREE join disclosure — see [`disclose`].
//!
//! # One tree, two lanes, two answers
//!
//! `zzop analyze <tree>` does not run the cross-layer join at all: the join is the 2+-tree question,
//! and a single-tree config is refused by `zzop cross` for that exact reason. But `zzop file` and
//! `zzop endpoint` reach the engine through `analyze_trees` even for one path — deliberately, because
//! the reply names the TREE a file was found in and a cross-layer finding anchored in the target file
//! is part of "everything about this file" (`zzop_summary::file`'s own module doc argues it).
//!
//! So the join DOES run over one tree there, and its consumer side is whatever that one tree happens
//! to contain. On a backend-only tree that is nothing, and every route the tree serves reads as
//! consumed by nobody.
//!
//! 📏 Measured (external review round 20, ledger V224) on `corpus/frameworks/express`:
//! `zzop file examples/route-separation/index.js` reports 22 findings, of which **11** are
//! `cross-layer/unconsumed-endpoint` (6) and `cross-layer/unconsumed-mutation-endpoint` (5) — that
//! file registers 8 routes, so essentially every route it serves is reported, and half of everything
//! the reply says about the file is this one artifact. `zzop analyze` on the SAME tree reports zero of
//! those rules. Two answers about the same handlers, at the same moment, with nothing saying why.
//!
//! # Why this discloses rather than suppresses
//!
//! Each such finding already carries the caveat in its own prose ("it may be consumed by a caller this
//! analysis cannot see — a repo not included in this `analyzeTrees` run"). What no channel said is the
//! STRUCTURAL fact that makes the whole group unreliable at once: this run had one tree, so there was
//! no second layer for the join to find consumers in.
//!
//! Suppressing the rules on a single-tree run was the alternative and is not this module's call to
//! make: `zzop_summary::file`'s doc argues the opposite position on the record, and overturning a
//! written decision days before a freeze — to delete findings rather than explain them — is the more
//! expensive direction if that argument is right. A reader who now knows the condition can act either
//! way; a reader with the findings silently gone cannot.
//!
//! # Why it rides on the tree's own warnings
//!
//! Same channel and same reasoning as the three siblings in this module tree (the test-io join filter,
//! the topology-host tripwire, the wildcard partition): a per-tree `AnalyzeOutput::warnings` entry has
//! a proven carrier all the way to every host surface, and a new run-level channel would need one.

use crate::AnalyzeOutput;

/// Pushes ONE warning onto the single tree's own channel when the join ran over exactly one tree AND
/// produced at least one unconsumed provide.
///
/// Both conditions matter. Without the tree count it would fire on a real 2+-tree run, where an
/// unconsumed provide is a genuine finding and this sentence would be false. Without the second it
/// would fire on every single-tree run of `coverage`/`facts`/`file` over a tree with no routes at all,
/// which is a warning about nothing — and this repo's channels are read as "something happened here".
pub(super) fn disclose(
    outputs: &mut [(std::path::PathBuf, String, AnalyzeOutput)],
    unconsumed_provides: usize,
) {
    if !applies(outputs.len(), unconsumed_provides) {
        return;
    }
    let Some((_, _, output)) = outputs.first_mut() else {
        return;
    };
    output.warnings.push(message(unconsumed_provides));
}

/// The CONDITION, split out so it is testable without building an `AnalyzeOutput` (which has no
/// `Default` — a fixture for it would be a second, hand-maintained copy of the engine's output shape).
fn applies(tree_count: usize, unconsumed_provides: usize) -> bool {
    tree_count == 1 && unconsumed_provides > 0
}

/// The pure half, so the wording is testable without an `AnalyzeOutput`.
fn message(unconsumed_provides: usize) -> String {
    format!(
        "the cross-layer join ran over ONE tree, so its consumer side is only what this same tree \
         contains — {unconsumed_provides} provide(s) matched no consume and are reported as \
         unconsumed. On a backend-only tree that is every route it serves, and it is a fact about the \
         RUN rather than about the code: the callers live in a repo this run did not analyze. \
         A single-tree ANALYSIS of this same tree reports none of these rules at all, because one \
         tree is not the cross-layer question — so the two answers disagreeing here is this \
         condition, not a contradiction. To get a verdict worth acting on, analyze the consumer tree \
         and this one in the SAME run (declare both under `trees` in the config, or hand both paths \
         to the cross-layer lane); to keep the single-tree reply quiet instead, turn the rules off \
         (`rules: {{ \"cross-layer/unconsumed-endpoint\": \"off\", \
         \"cross-layer/unconsumed-mutation-endpoint\": \"off\" }}`)."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_message_names_the_count_and_both_ways_out() {
        let m = message(11);
        assert!(m.contains("11 provide(s)"), "{m}");
        // 🔴 Not `zzop cross`. This string reaches an MCP client too, which has no argv — a shared
        // crate's user-facing message must say what to DO, not what to type, and
        // `rule_contracts::host_vocabulary` fails the build over it. It caught this very message.
        assert!(m.contains("SAME run"), "{m}");
        assert!(!m.contains("`zzop "), "names a CLI-only spelling: {m}");
        assert!(m.contains("cross-layer/unconsumed-endpoint"), "{m}");
    }

    /// BOTH halves of the gate, including the two that must stay SILENT. A multi-tree run makes this
    /// sentence false (there, an unconsumed provide is a real finding), and a single-tree run with
    /// nothing unconsumed would be a warning about nothing.
    #[test]
    fn the_gate_fires_only_on_a_single_tree_that_actually_has_unconsumed_provides() {
        assert!(applies(1, 11), "the measured express case must fire");
        assert!(!applies(2, 40), "a real cross-tree run must stay silent");
        assert!(!applies(1, 0), "nothing unconsumed is nothing to say");
        assert!(!applies(0, 5), "no tree at all is not this module's case");
    }
}
