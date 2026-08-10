//! Mermaid emission for [`super`]: the drawn graph plus its census -> flowchart text.
//!
//! Split from the projection so the two halves can be read for different things — what the picture
//! CONTAINS is decided next door, what a reader is TOLD about it is decided here. Every disclosure this
//! repo owes a graph consumer lands in this file, which is what makes "did we say so?" a one-file
//! question.

use std::collections::BTreeMap;

use super::super::fold::Fold;
use super::{DepCensus, DepGraph};

/// Mermaid id for a node — mermaid ids cannot carry `/`, `.` or `:`, so a stable sanitized form plus an
/// index keeps them unique even when two different paths sanitize identically.
fn mermaid_id(index: usize) -> String {
    format!("f{index}")
}

pub(super) fn render(
    g: &DepGraph,
    c: &DepCensus,
    scope: Option<&str>,
    top: usize,
    fold_note: &str,
    fold: Fold,
) -> String {
    let mut out = String::new();
    out.push_str("%% zzop graph --domain dep — file import graph\n");
    out.push_str(&format!(
        "%% nodes: drawn {} / in-scope {} / total {} | edges: drawn {} / total {} | cycles reported: {}\n",
        c.drawn_nodes, c.in_scope_nodes, c.total_nodes, c.drawn_edges, c.total_edges, c.cycles
    ));
    out.push_str(&format!(
        "%% node cap --top {top}{}\n",
        scope.map(|s| format!(" | --scope {s}")).unwrap_or_default()
    ));
    out.push_str(fold_note);
    out.push_str("flowchart LR\n");

    let index: BTreeMap<&String, usize> = g.nodes.keys().enumerate().map(|(i, k)| (k, i)).collect();
    for (id, n) in &g.nodes {
        let i = index[id];
        let label = n.rel.replace('"', "'");
        if g.in_cycle.contains(id) {
            // A distinct SHAPE, not only a class: a reader looking at raw mermaid text (or a renderer
            // with no CSS) still sees which files are in a cycle.
            out.push_str(&format!("  {}{{{{\"{label}\"}}}}\n", mermaid_id(i)));
        } else {
            out.push_str(&format!("  {}[\"{label}\"]\n", mermaid_id(i)));
        }
    }
    for ((a, b), weight) in &g.edges {
        let (Some(ia), Some(ib)) = (index.get(a), index.get(b)) else {
            continue;
        };
        let arrow = if g.in_cycle.contains(a) && g.in_cycle.contains(b) {
            "==>"
        } else {
            "-->"
        };
        // The weight is emitted only under `--fold`, where it is the collapsed file-edge count. Without
        // a fold every weight is 1 and the label would be pure noise.
        let label = if fold.is_on() {
            format!("|{weight}| ")
        } else {
            String::new()
        };
        out.push_str(&format!(
            "  {} {arrow} {label}{}\n",
            mermaid_id(*ia),
            mermaid_id(*ib)
        ));
    }

    // The disclosure has to survive into the PICTURE, not only the source text — same rule the join map
    // follows, and the reason a `%%` header alone is not enough.
    let dropped_nodes = c.in_scope_nodes.saturating_sub(c.drawn_nodes);
    let dropped_edges = c.total_edges.saturating_sub(c.drawn_edges);
    // A folded node is a BOX standing for many files, so calling it a file here would contradict the
    // fold census three lines above it — and this note is the half that survives into the rendered
    // picture, which makes it the half a reader actually believes.
    let unit = if fold.is_on() { "boxes" } else { "files" };
    let note = if dropped_nodes == 0 && dropped_edges == 0 && scope.is_none() {
        format!(
            "complete: all {} {unit} and {} import edges drawn",
            c.total_nodes, c.total_edges
        )
    } else {
        format!(
            "PARTIAL VIEW: {} of {} {unit} drawn ({} dropped by --top {}), {} of {} edges. An edge whose \
             other end was dropped is not drawn. Use zzop facts for the uncapped graph.",
            c.drawn_nodes, c.in_scope_nodes, dropped_nodes, top, c.drawn_edges, c.total_edges
        )
    };
    out.push_str(&format!("  zzopNote[\"{note}\"]\n"));
    if c.cycles > 0 {
        let hexagon = if fold.is_on() {
            "a hexagon means the box CONTAINS a file in some cycle, not that the boxes form one — a \
             fold cannot tell those apart and does not pretend to"
        } else {
            "files in a cycle are drawn as hexagons with thick arrows"
        };
        out.push_str(&format!(
            "  zzopCycles[\"{} circular finding(s) — {hexagon}\"]\n",
            c.cycles
        ));
    }
    out
}
