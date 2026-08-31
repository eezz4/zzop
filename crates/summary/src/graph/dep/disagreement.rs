//! The one number both dep-graph formats need in order to be honest about a fact neither of them
//! computes: that the edge list they draw and the cycle verdict they print beside it were derived from
//! two different edge sets.
//!
//! Split out of the parent for the per-file line cap; the seam is real either way. The parent turns an
//! `analyzeTrees` value into a graph, and this asks ONE question about the finished graph — it opens no
//! JSON, knows nothing about trees, source ids or findings, and could not be re-used by the parent's
//! collection loop even if it wanted to be.
//!
//! The SENTENCE that explains the number has a different owner again
//! (`zzop_core::CYCLE_GRAPH_EXCLUDES_ERASED_IMPORTS`, beside the code that does the subtracting), which
//! is deliberate: a count computed here and a claim written there cannot drift into disagreeing, because
//! neither one restates the other.

use std::collections::BTreeSet;

/// Unordered pairs `{a, b}` where BOTH `a -> b` and `b -> a` are edges of this graph and at least one
/// end is in no reported cycle.
///
/// # Why this counter exists
/// `ir.dep` — what these lanes draw — holds every resolved in-tree import. `circular` is computed after
/// `zzop_core::noncycle` SUBTRACTS the edges whose every contributing binding is erased at compile time
/// (`import type { X }`, a dynamic `import()`, or a plain `import { X }` whose target declares `X` as an
/// `export type`/`export interface`): those carry no runtime module load, so they cannot close a runtime
/// cycle. Both halves are deliberate; until 2026-08-31 no surface said the pair existed. The absence was
/// loud — `corpus/oss/fe-axios` emits BOTH directions of
/// `src/components/App/App.slice.ts <-> src/types/user.ts` in the links table with `inCycle: false` on
/// both nodes and `0 circular finding(s)` in the census, so a reader who trusts the rows sees a cycle
/// the tool denies and has no third line to reconcile them with.
///
/// # Why a mutual-pair count and not cycle detection
/// A second Tarjan here would be a second answer to a question the engine already answered (the parent
/// module's doc forbids exactly that). A 2-cycle needs no traversal: it is a membership test over the
/// edges already collected, exact for the smallest and most legible case, and it can only UNDERCOUNT the
/// disagreement — the safe direction for a disclosure. Requiring only that ONE end be outside the cycle
/// set (rather than both) is conservative for the same reason: two files that import each other and both
/// belong to some reported cycle are already explained by that finding.
pub(super) fn mutual_pairs_outside_cycles(
    edges: &BTreeSet<(String, String)>,
    cycle_files: &BTreeSet<String>,
) -> usize {
    edges
        .iter()
        // Each unordered pair is counted once, at the lexicographically smaller end. A self-edge
        // (`a -> a`) is excluded by the same comparison — the engine does not emit one, and calling it
        // a mutual pair would be this counter inventing a fact.
        .filter(|(a, b)| a < b && edges.contains(&((*b).clone(), (*a).clone())))
        .filter(|(a, b)| !(cycle_files.contains(a) && cycle_files.contains(b)))
        .count()
}
