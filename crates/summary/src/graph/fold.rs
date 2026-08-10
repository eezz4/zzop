//! `--fold <n>` — the granularity axis shared by the RELATION domains (`dep`, `cochange`).
//!
//! # Why one flag instead of more domains
//! "What connects two nodes" and "what IS a node" are independent questions, and spelling their product
//! as domain names (`folderdep`, `layerdep`, `foldercochange`, ...) multiplies the vocabulary without
//! adding a fact. `--domain` keeps answering the first; this answers the second. A domain whose node
//! kind is fixed by a JUDGMENT rather than by a path (`risk`'s hubs and seams, `posture`'s routes)
//! cannot cross with it at all, so it REFUSES the flag at the CLI edge rather than accepting a knob that
//! does nothing — see `GraphDomain::accepts_fold`.
//!
//! # What folding is, and what it deliberately is not
//! A fold is a DISPLAY collapse over paths, applied after `--scope` and before `--top`. That order is
//! the contract: `--scope` keeps meaning "which FILES are in this picture" (folding first would make
//! `--scope crates/engine/src` match nothing at depth 2), and `--top` keeps meaning "how many boxes are
//! drawn" (capping first would cap files and then collapse them, which is a picture of an arbitrary
//! sample rather than of the tree).
//!
//! It re-reads nothing. The edges are the same edges the file-level picture draws — a folded run and an
//! unfolded run of the same analysis cannot disagree about what is connected to what, because one is
//! literally the other with its endpoints renamed.
//!
//! # The three ways a fold can mislead, each disclosed rather than avoided
//! Folding is not free, and none of these can be fixed by choosing a better depth — they are properties
//! of collapsing, so the picture states them:
//!
//! 1. **The tail disappears into a number.** 155 file edges become one box-to-box edge. So a folded edge
//!    is ALWAYS labelled with how many file-level edges it collapsed, and the label means that and only
//!    that — never a commit count, never a strength.
//! 2. **The depth is a convention, not a fact about the tree.** `crates/engine` and
//!    `parser/parser-typescript` are both two segments here and both happen to be the unit a reader
//!    means; a tree that puts everything under `src/` collapses to ONE box at the same depth. The census
//!    reports the box count so a degenerate fold is visible as a number rather than felt as a boring
//!    picture.
//! 3. **A path shorter than the depth cannot fold.** A root-level file stands for itself and sits beside
//!    boxes that stand for hundreds. Counted and named, never silently promoted.
use std::fmt::Write as _;

/// The `--fold` setting. `None` is the file-level picture — the absence of the flag, not a depth of 0.
#[derive(Clone, Copy, Default)]
pub(super) struct Fold(Option<usize>);

impl Fold {
    /// `None` -> the unfolded (file-level) picture. A `Some(0)` never reaches here: the CLI rejects it as
    /// an argument-shape error, because "fold to zero segments" would name one box for the whole tree and
    /// is far more likely a typo than a request.
    pub(super) fn of(depth: Option<usize>) -> Self {
        Fold(depth.filter(|d| *d > 0))
    }

    pub(super) fn is_on(self) -> bool {
        self.0.is_some()
    }

    pub(super) fn depth(self) -> Option<usize> {
        self.0
    }

    /// The path this node is drawn as. Returns a BORROW of the input when folding: the fold is always a
    /// leading prefix, so no allocation is needed to name the box.
    ///
    /// A path with fewer than `depth` separators is returned whole — see the module doc's third
    /// caveat. It is the honest answer (there is nothing coarser to call it), not a fallback.
    pub(super) fn rel(self, rel: &str) -> &str {
        let Some(depth) = self.0 else { return rel };
        let mut seen = 0usize;
        for (i, ch) in rel.char_indices() {
            if ch == '/' {
                seen += 1;
                if seen == depth {
                    return &rel[..i];
                }
            }
        }
        rel
    }

    /// True when `rel` has fewer segments than the fold depth, so it stands for itself rather than for a
    /// group. Callers count these for the census — the picture must not let a lone file look like a
    /// module.
    pub(super) fn is_unfoldable(self, rel: &str) -> bool {
        match self.0 {
            None => false,
            Some(depth) => rel.matches('/').count() < depth,
        }
    }
}

/// The `%%` census lines every folded picture carries, in one place so `dep` and `cochange` cannot
/// describe the same collapse differently. Returns the empty string when the fold is off — an unfolded
/// picture has nothing to disclose here, and printing "fold: none" would be noise on the common path.
///
/// `files`/`boxes` are the node counts either side of the collapse; `file_edges`/`box_edges` the same for
/// edges (both BEFORE `--top`, so the fold's own loss is separable from the cap's — two different
/// reasons a reader is not seeing everything, and one number cannot carry both).
pub(super) fn census(
    fold: Fold,
    files: usize,
    boxes: usize,
    unfoldable: usize,
    file_edges: usize,
    box_edges: usize,
) -> String {
    let Some(depth) = fold.depth() else {
        return String::new();
    };
    let mut out = String::new();
    let _ = writeln!(
        out,
        "%% FOLDED to --fold {depth} path segment(s): {files} file(s) drawn as {boxes} box(es), \
         {file_edges} file-level edge(s) as {box_edges}. Each edge label is the NUMBER OF FILE-LEVEL \
         EDGES it collapsed — nothing else."
    );
    let _ = writeln!(
        out,
        "%% A fold is a display collapse over PATHS, so the depth is a convention rather than a fact \
         about this tree: a tree that keeps everything under one top directory collapses to one box at \
         the same depth. Compare the two counts above before reading the shape."
    );
    if unfoldable > 0 {
        let _ = writeln!(
            out,
            "%% {unfoldable} of those box(es) are a SINGLE FILE with fewer than {depth} path segment(s) \
             — nothing coarser exists to name them, so they stand beside boxes that stand for many."
        );
    }
    out
}

#[cfg(test)]
mod tests;
