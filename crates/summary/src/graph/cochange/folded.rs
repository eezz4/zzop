//! The `--fold` collapse for [`super`]'s co-change pairs: same ties, coarser endpoints.
//!
//! Split out of `cochange.rs` for the same reason [`super::super::dep::folded`] is, and shaped
//! deliberately like it — two domains that fold by the same rule should be readable side by side, so a
//! reviewer comparing them is comparing behaviour rather than layout.
//!
//! The one place they genuinely differ is the label, and it is the difference that matters: `dep`'s
//! file-level edge has no weight to lose, while this domain's DOES (commits touching both files). See
//! [`fold_edges`] for why that weight cannot survive the collapse.

use std::collections::{BTreeMap, BTreeSet};

use super::super::fold::Fold;
use super::Edge;

/// Collapses file pairs to `fold`'s depth. Returns the box edges plus the counts the census needs.
///
/// # Why the label stops being a commit count
/// At file granularity an edge's label is "commits that touched both files", which is a fact about
/// history. Summing those across the pairs inside a box pair would count one commit once per pair it
/// touched, producing a number larger than the repository has commits — a plausible-looking lie. So a
/// folded label means what `dep`'s folded label means: HOW MANY FILE-LEVEL EDGES collapsed here. One
/// rule for both domains, stated by `super::fold::census` in the picture itself.
pub(super) fn fold_edges(fold: Fold, edges: Vec<Edge>) -> (Vec<Edge>, FoldCounts) {
    let file_pairs = edges.len();
    let mut file_nodes: BTreeSet<&str> = BTreeSet::new();
    let mut boxes: BTreeMap<(String, String, String), u64> = BTreeMap::new();
    let mut unfoldable: BTreeMap<String, bool> = BTreeMap::new();
    for e in &edges {
        file_nodes.insert(e.a.as_str());
        file_nodes.insert(e.b.as_str());
        let (fa, fb) = (fold.rel(&e.a), fold.rel(&e.b));
        for (raw, folded) in [(&e.a, fa), (&e.b, fb)] {
            let slot = unfoldable.entry(folded.to_string()).or_insert(true);
            *slot &= fold.is_unfoldable(raw);
        }
        if fa == fb {
            // Both files live in the same box. The tie is real but it is INTERNAL cohesion, not a
            // relation between boxes — drawing a self-loop would read as a boundary where there is none.
            continue;
        }
        // Re-normalize after folding: `a < b` held for the file paths, and folding can invert it.
        let key = if fa < fb {
            (e.source.clone(), fa.to_string(), fb.to_string())
        } else {
            (e.source.clone(), fb.to_string(), fa.to_string())
        };
        *boxes.entry(key).or_insert(0) += 1;
    }
    let box_count = {
        let mut names = BTreeSet::new();
        for (_, a, b) in boxes.keys() {
            names.insert(a);
            names.insert(b);
        }
        names.len()
    };
    let mut out: Vec<Edge> = boxes
        .into_iter()
        .map(|((source, a, b), count)| Edge {
            a,
            b,
            count,
            source,
        })
        .collect();
    // Same ordering contract the engine gives the unfolded list: heaviest first, then a total order.
    out.sort_by(|x, y| {
        y.count
            .cmp(&x.count)
            .then_with(|| x.source.cmp(&y.source))
            .then_with(|| x.a.cmp(&y.a))
            .then_with(|| x.b.cmp(&y.b))
    });
    (
        out,
        FoldCounts {
            file_nodes: file_nodes.len(),
            file_pairs,
            boxes: box_count,
            unfoldable: unfoldable.values().filter(|only| **only).count(),
        },
    )
}

/// What the collapse cost, for [`super::fold::census`]. Named fields rather than a 4-tuple because three
/// of the four are counts of different things and a positional swap would be silent.
pub(super) struct FoldCounts {
    /// Distinct files appearing in at least one in-scope pair — this domain's node population, which is
    /// NOT the tree's file count (a file with no measured tie is not in this picture at all).
    pub(super) file_nodes: usize,
    pub(super) file_pairs: usize,
    pub(super) boxes: usize,
    pub(super) unfoldable: usize,
}
