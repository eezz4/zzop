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

mod folded;
mod node;
mod render;
mod window;

use super::fold::{self, Fold};

pub(super) use node::DepNode;
use render::render;
pub(super) use window::GitWindows;

/// One drawn node, keyed by its display id so two trees' identical relative paths cannot collide. A node
/// is a FILE at the default granularity and a path prefix under `--fold` — see [`super::fold`]; the
/// ranking, capping and rendering below are written once and read the same shape either way.
#[derive(Default)]
struct DepGraph {
    /// display id -> node
    nodes: BTreeMap<String, DepNode>,
    /// `(from, to) -> how many FILE-LEVEL import edges this drawn edge stands for`. Always 1 without
    /// `--fold`, which is why the label is emitted only when folding: a picture where every edge says
    /// `|1|` teaches nothing and costs width.
    edges: BTreeMap<(String, String), usize>,
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
    /// The git window(s) the nodes' history axes were measured over — a property of the RUN, not of a
    /// file, so it rides here rather than on every [`DepNode`]. See [`window`].
    pub(super) git_windows: GitWindows,
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
    let mut git_windows = GitWindows::default();
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
        // ... and WHICH window those history axes cover. Same per-tree gate the axes themselves use,
        // asked separately because the answer describes the run rather than a file.
        git_windows.observe(t);
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
        git_windows,
    }
}

/// `analyzeTrees` output -> mermaid text for the dependency domain. Pure, like `super::project`.
pub(super) fn project(v: &Value, scope: Option<&str>, top: usize, fold: Fold) -> String {
    let DepUniverse {
        nodes: all_nodes,
        edges: all_edges,
        cycle_files,
        cycles,
        // The cosmograph census's business: the mermaid lane draws topology and emits no history
        // column, so it has nothing here to caveat.
        git_windows: _,
    } = collect(v);

    // Pass 2 — scope, then (optionally) fold, then rank by degree and cap. That ORDER is the contract,
    // and `super::fold`'s module doc carries why: `--scope` keeps meaning "which files are in this
    // picture" and `--top` keeps meaning "how many boxes are drawn".
    let in_scope: BTreeMap<&String, &DepNode> = all_nodes
        .iter()
        .filter(|(id, n)| node_in_scope(scope, id, &n.source, &n.rel))
        .collect();

    // The two granularities differ ONLY in what a node is; everything below this point is written once.
    // The unfolded arm keeps the file-level counts it always had — the fold's own loss is reported by
    // `fold_note` against a different denominator, because "the cap dropped it" and "the fold merged it"
    // are two reasons a reader is not seeing something and one number cannot carry both.
    let (universe, edges, in_cycle, fold_note, total_nodes, total_edges) = if fold.is_on() {
        let f = folded::collapse(fold, &in_scope, &all_edges, &cycle_files);
        let note = fold::census(
            fold,
            in_scope.len(),
            f.nodes.len(),
            f.unfoldable,
            f.file_edges,
            f.edges.len(),
        );
        let (n, e) = (f.nodes.len(), f.edges.len());
        (f.nodes, f.edges, f.in_cycle, note, n, e)
    } else {
        let universe: BTreeMap<String, DepNode> = in_scope
            .iter()
            .map(|(id, n)| ((*id).clone(), (*n).clone()))
            .collect();
        let edges: BTreeMap<(String, String), usize> =
            all_edges.iter().map(|e| (e.clone(), 1)).collect();
        (
            universe,
            edges,
            cycle_files.clone(),
            String::new(),
            all_nodes.len(),
            all_edges.len(),
        )
    };

    // Ranking is (degree desc, id asc): a total order, so the same analysis always draws the same
    // picture.
    let mut degree: BTreeMap<&String, usize> = universe.keys().map(|k| (k, 0)).collect();
    for (a, b) in edges.keys() {
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
        if let Some(v) = universe.get(*id) {
            g.nodes.insert((*id).clone(), v.clone());
        }
        if in_cycle.contains(*id) {
            g.in_cycle.insert((*id).clone());
        }
    }
    for ((a, b), weight) in &edges {
        if kept.contains(a) && kept.contains(b) {
            g.edges.insert((a.clone(), b.clone()), *weight);
        }
    }

    let census = DepCensus {
        total_nodes,
        total_edges,
        drawn_nodes: g.nodes.len(),
        drawn_edges: g.edges.len(),
        in_scope_nodes: if fold.is_on() {
            universe.len()
        } else {
            in_scope.len()
        },
        cycles,
    };
    render(&g, &census, scope, top, &fold_note, fold)
}

#[cfg(test)]
mod tests;
