//! The `--fold` collapse for [`super`]'s file graph: same edges, coarser endpoints.
//!
//! Split out of `dep.rs` rather than inlined because it is a self-contained rewrite of one graph into
//! another — the caller hands it the in-scope file graph and gets back the box graph plus the numbers
//! the census needs, with no knowledge of mermaid on either side.
//!
//! # Cycle marking survives the fold, and says less than it did
//! A box is marked as participating in a cycle when ANY file inside it does. That is the only defensible
//! reading — the alternative (mark only when the cycle is wholly inside the box) would hide precisely the
//! cycles that cross a module boundary, which are the ones a reader folding the picture is looking for —
//! but it is weaker than the file-level mark, so the shape means "contains a file in some cycle" rather
//! than "is in a cycle". `dep`'s note node says so whenever the fold is on.

use std::collections::{BTreeMap, BTreeSet};

use super::super::fold::Fold;
use super::node::DepNode;

/// The folded graph plus what it took to build it.
pub(in crate::graph) struct Folded {
    pub(in crate::graph) nodes: BTreeMap<String, DepNode>,
    /// `(from box, to box) -> how many file-level import edges collapsed into it`.
    pub(in crate::graph) edges: BTreeMap<(String, String), usize>,
    pub(in crate::graph) in_cycle: BTreeSet<String>,
    /// Boxes that are a single file with fewer segments than the depth — see [`Fold::is_unfoldable`].
    pub(in crate::graph) unfoldable: usize,
    /// File-level edges BETWEEN in-scope files, before the collapse. The denominator the census reports
    /// the fold's loss against; deliberately not the whole-tree edge count, which is the CAP's
    /// denominator and a different number.
    pub(in crate::graph) file_edges: usize,
}

/// Collapses the in-scope file graph to `fold`'s depth.
///
/// `in_scope` is the file nodes that survived `--scope`; `all_edges` the whole universe's edges, of
/// which only those with BOTH ends in scope participate (an edge to a file the reader filtered out is
/// not evidence about a box they can see). Self-edges after folding are dropped — a box importing
/// itself is what a module IS, not a relation between modules.
pub(in crate::graph) fn collapse(
    fold: Fold,
    in_scope: &BTreeMap<&String, &DepNode>,
    all_edges: &BTreeSet<(String, String)>,
    cycle_files: &BTreeSet<String>,
) -> Folded {
    // file id -> box id, built once so the edge pass is a lookup rather than a re-fold.
    let mut box_of: BTreeMap<&str, String> = BTreeMap::new();
    let mut nodes: BTreeMap<String, DepNode> = BTreeMap::new();
    let mut in_cycle: BTreeSet<String> = BTreeSet::new();
    let mut unfoldable_boxes: BTreeMap<String, bool> = BTreeMap::new();

    for (id, n) in in_scope {
        let folded_rel = fold.rel(&n.rel);
        // A multi-tree run prefixes ids with the source; keep that, since two trees' `src/` are still
        // two different boxes. `id != rel` is exactly the condition `dep::collect` used to add it.
        let box_id = if id.as_str() == n.rel {
            folded_rel.to_string()
        } else {
            format!("{}::{}", n.source, folded_rel)
        };
        box_of.insert(id.as_str(), box_id.clone());
        // A box has no single loc and no single history, so the measured axes are deliberately dropped
        // rather than summed: `loc` would be defensible, `lastModified` and `authorCount` would not, and
        // a node whose fields mean different things per field is worse than one that measures nothing.
        // The mermaid lane draws topology only, so nothing here reads them.
        nodes.entry(box_id.clone()).or_insert_with(|| DepNode {
            source: n.source.clone(),
            rel: folded_rel.to_string(),
            loc: None,
            git: None,
        });
        // Unfoldable only if EVERY file in the box is — a box that also holds deeper paths is a real
        // group, and the caveat would be false about it.
        let entry = unfoldable_boxes.entry(box_id.clone()).or_insert(true);
        *entry &= fold.is_unfoldable(&n.rel);
        if cycle_files.contains(*id) {
            in_cycle.insert(box_id);
        }
    }

    let mut edges: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut file_edges = 0usize;
    for (a, b) in all_edges {
        let (Some(ba), Some(bb)) = (box_of.get(a.as_str()), box_of.get(b.as_str())) else {
            continue;
        };
        file_edges += 1;
        if ba == bb {
            continue;
        }
        *edges.entry((ba.clone(), bb.clone())).or_insert(0) += 1;
    }

    Folded {
        nodes,
        edges,
        in_cycle,
        unfoldable: unfoldable_boxes.values().filter(|only| **only).count(),
        file_edges,
    }
}

#[cfg(test)]
mod tests;
