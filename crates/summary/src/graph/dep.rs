//! The DEPENDENCY-graph domain of `zzop graph` — the second domain to ship, and the first one whose
//! nodes are files rather than io keys.
//!
//! # Why this domain second, and why the format question stayed closed
//! The visualization decision (the visualization section of `rule-candidates.md`) left per-domain DIMENSION and LIBRARY open,
//! noting that the join map's mermaid verdict was "for that case" and did not close 3D candidates. This
//! domain answers that for itself and the answer is the same: **2D, mermaid, no new format**. A file
//! import graph is a DAG-with-cycles over named nodes — exactly what a flowchart is for — and every
//! argument the join map made (renders inline in the main consumer's surface, no toolchain, zero
//! rendering code here) applies unchanged. A 3D force layout would need a viewer, a library, and a
//! coordinate system this repo would then own, which is the opposite of "our viz code is zero".
//!
//! So the open question is now narrower rather than answered wholesale: 3D stays a live candidate only
//! for a domain whose data is genuinely not a small labelled graph — a whole-repo risk surface with
//! thousands of nodes and continuous dimensions. Neither shipped domain is that.
//!
//! # What a node IS
//! One FILE, labelled by its tree-relative path, prefixed by `sourceId` when the run has more than one
//! tree (two trees can both have `src/index.ts`). An edge is "importer -> imported", the direction
//! `ir.dep` already stores. What ELSE a node carries — the measured axes a viewer styles by, and the
//! rule that an unmeasured one is an omitted column rather than a zero — is [`node`]'s business.
//!
//! # Cycles are the point, so they are drawn differently
//! `circular` is the highest-severity structural finding this engine emits and the hardest to read as
//! text: a 6-file cycle is a list of 6 paths that a reader has to mentally close into a loop. Files that
//! participate in a cycle are marked, and their edges use a distinct arrow, so the loop is visible
//! rather than reconstructed. The cycle membership comes from the engine's own `circular` findings —
//! this module does NOT re-run Tarjan, because a second cycle implementation is a second answer.
//!
//! # Scoping, and why the default is tighter than the join map's
//! An import graph is far denser than a join: a mid-size repo has thousands of edges where the join has
//! tens. Drawing all of them produces a black square, which is worse than drawing nothing because it
//! looks like information. So `--top` here caps NODES (not edges) by fan-in + fan-out — the files a
//! reader is most likely to be asking about — and every dropped node and edge is disclosed in the same
//! two places the join map uses: a `%%` header and a VISIBLE note node, because a comment does not
//! survive into a rendered picture.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

/// Default node cap for this domain. Lower than the join map's relation cap on purpose — see the module
/// doc's density note; a flowchart stops being readable well before it stops being renderable.
pub const DEFAULT_DEP_TOP: usize = 40;

mod node;

pub(super) use node::DepNode;

/// One file node, keyed by its display id so two trees' identical relative paths cannot collide.
#[derive(Default)]
struct DepGraph {
    /// display id -> node
    nodes: BTreeMap<String, DepNode>,
    edges: BTreeSet<(String, String)>,
    in_cycle: BTreeSet<String>,
}

/// Census of what was dropped, so the disclosure is computed rather than asserted.
struct DepCensus {
    total_nodes: usize,
    total_edges: usize,
    drawn_nodes: usize,
    drawn_edges: usize,
    in_scope_nodes: usize,
    cycles: usize,
}

/// Everything the run produced, before any `--scope` filter or `--top` cap. Pass 1 of `project`, lifted
/// out because the cosmograph lane needs exactly this and nothing after it: capping exists to keep a
/// DRAWN picture readable, and a viewer with zoom does that job itself. So the two formats share their
/// whole understanding of the graph and differ only in what they do with it — which is the property that
/// keeps them from drifting into two different answers about the same repo.
pub(super) struct DepUniverse {
    /// display id -> node
    pub(super) nodes: BTreeMap<String, DepNode>,
    pub(super) edges: BTreeSet<(String, String)>,
    pub(super) cycle_files: BTreeSet<String>,
    pub(super) cycles: usize,
}

/// Does one node survive `--scope`? One rule, shared by both formats for the same reason `in_scope` is
/// shared across the join map's buckets: a filter that means two things is the ambiguity it exists to
/// remove.
pub(super) fn node_in_scope(scope: Option<&str>, id: &str, source: &str, rel: &str) -> bool {
    match scope {
        None => true,
        Some(p) => id.starts_with(p) || source.starts_with(p) || rel.starts_with(p),
    }
}

/// `analyzeTrees` output -> the uncapped file graph. Pure; no filesystem, no cap, no format.
pub(super) fn collect(v: &Value) -> DepUniverse {
    let empty = Vec::new();
    let trees = v["trees"].as_array().unwrap_or(&empty);
    let multi = trees.len() > 1;

    let mut all_nodes: BTreeMap<String, DepNode> = BTreeMap::new();
    let mut all_edges: BTreeSet<(String, String)> = BTreeSet::new();
    let mut cycle_files: BTreeSet<String> = BTreeSet::new();
    let mut cycles = 0usize;
    for t in trees {
        let source = t["sourceId"].as_str().unwrap_or("");
        let id = |rel: &str| -> String {
            if multi {
                format!("{source}::{rel}")
            } else {
                rel.to_string()
            }
        };
        // The measured axes (`ir.loc`, and the git history when this tree collected any) — read once
        // per tree, then asked per file. See `node`'s module doc for why an unmeasured one stays `None`.
        let axes = node::TreeAxes::of(t);
        if let Some(dep) = t["output"]["ir"]["dep"].as_object() {
            for (from, tos) in dep {
                all_nodes.insert(id(from), axes.node(source, from));
                for to in tos.as_array().unwrap_or(&empty) {
                    let Some(to) = to.as_str() else { continue };
                    all_nodes.insert(id(to), axes.node(source, to));
                    all_edges.insert((id(from), id(to)));
                }
            }
        }
        // Cycle membership from the engine's own verdict — never a second Tarjan here.
        for f in t["output"]["findings"].as_array().unwrap_or(&empty) {
            if f["ruleId"].as_str() != Some("circular") {
                continue;
            }
            cycles += 1;
            if let Some(file) = f["file"].as_str() {
                cycle_files.insert(id(file));
            }
            for m in f["data"]["members"].as_array().unwrap_or(&empty) {
                if let Some(m) = m.as_str() {
                    cycle_files.insert(id(m));
                }
            }
        }
    }

    DepUniverse {
        nodes: all_nodes,
        edges: all_edges,
        cycle_files,
        cycles,
    }
}

/// `analyzeTrees` output -> mermaid text for the dependency domain. Pure, like `super::project`.
pub(super) fn project(v: &Value, scope: Option<&str>, top: usize) -> String {
    let DepUniverse {
        nodes: all_nodes,
        edges: all_edges,
        cycle_files,
        cycles,
    } = collect(v);

    // Pass 2 — scope, then rank by degree and cap NODES. Ranking is (degree desc, id asc): a total
    // order, so the same analysis always draws the same picture.
    let in_scope: BTreeMap<&String, &DepNode> = all_nodes
        .iter()
        .filter(|(id, n)| node_in_scope(scope, id, &n.source, &n.rel))
        .collect();
    let mut degree: BTreeMap<&String, usize> = in_scope.keys().map(|k| (*k, 0)).collect();
    for (a, b) in &all_edges {
        for end in [a, b] {
            if let Some(d) = degree.get_mut(end) {
                *d += 1;
            }
        }
    }
    let mut ranked: Vec<(&String, usize)> = degree.into_iter().collect();
    ranked.sort_by(|x, y| y.1.cmp(&x.1).then(x.0.cmp(y.0)));
    let kept: BTreeSet<&String> = ranked.iter().take(top).map(|(k, _)| *k).collect();

    let mut g = DepGraph::default();
    for id in &kept {
        if let Some(v) = all_nodes.get(*id) {
            g.nodes.insert((*id).clone(), v.clone());
        }
        if cycle_files.contains(*id) {
            g.in_cycle.insert((*id).clone());
        }
    }
    for (a, b) in &all_edges {
        if kept.contains(a) && kept.contains(b) {
            g.edges.insert((a.clone(), b.clone()));
        }
    }

    let census = DepCensus {
        total_nodes: all_nodes.len(),
        total_edges: all_edges.len(),
        drawn_nodes: g.nodes.len(),
        drawn_edges: g.edges.len(),
        in_scope_nodes: in_scope.len(),
        cycles,
    };
    render(&g, &census, scope, top)
}

/// Mermaid id for a node — mermaid ids cannot carry `/`, `.` or `:`, so a stable sanitized form plus an
/// index keeps them unique even when two different paths sanitize identically.
fn mermaid_id(index: usize) -> String {
    format!("f{index}")
}

fn render(g: &DepGraph, c: &DepCensus, scope: Option<&str>, top: usize) -> String {
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
    for (a, b) in &g.edges {
        let (Some(ia), Some(ib)) = (index.get(a), index.get(b)) else {
            continue;
        };
        let arrow = if g.in_cycle.contains(a) && g.in_cycle.contains(b) {
            "==>"
        } else {
            "-->"
        };
        out.push_str(&format!(
            "  {} {arrow} {}\n",
            mermaid_id(*ia),
            mermaid_id(*ib)
        ));
    }

    // The disclosure has to survive into the PICTURE, not only the source text — same rule the join map
    // follows, and the reason a `%%` header alone is not enough.
    let dropped_nodes = c.in_scope_nodes.saturating_sub(c.drawn_nodes);
    let dropped_edges = c.total_edges.saturating_sub(c.drawn_edges);
    let note = if dropped_nodes == 0 && dropped_edges == 0 && scope.is_none() {
        format!(
            "complete: all {} files and {} import edges drawn",
            c.total_nodes, c.total_edges
        )
    } else {
        format!(
            "PARTIAL VIEW: {} of {} files drawn ({} dropped by --top {}), {} of {} edges. An edge whose \
             other end was dropped is not drawn. Use zzop facts for the uncapped graph.",
            c.drawn_nodes, c.in_scope_nodes, dropped_nodes, top, c.drawn_edges, c.total_edges
        )
    };
    out.push_str(&format!("  zzopNote[\"{note}\"]\n"));
    if c.cycles > 0 {
        out.push_str(&format!(
            "  zzopCycles[\"{} circular finding(s) — files in a cycle are drawn as hexagons with thick arrows\"]\n",
            c.cycles
        ));
    }
    out
}

#[cfg(test)]
mod tests;
