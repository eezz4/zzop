//! The CO-CHANGE domain of `zzop graph` — which files move together in git history, the relation the
//! import graph structurally cannot see.
//!
//! # Why this is its own picture and not an overlay on `--domain dep`
//! Both domains draw edges over the same nodes, and drawing them in one frame would be the more
//! convenient answer. It is refused because **the two edges are true in different ways**:
//!
//! - an import edge is READ FROM SOURCE — the specifier is there or it is not;
//! - a co-change edge is a SAMPLE OF HISTORY, filtered twice before it reaches here (commits outside
//!   `zzop_metrics::MIN_FILES_PER_COMMIT..=MAX_FILES_PER_COMMIT` form no pair at all, and each file
//!   keeps only its strongest `COUPLING_TOP_PER_FILE` partners — the tail is dropped, not summed), and
//!   it is EMPTY on a run with no git history rather than false.
//!
//! One frame with one edge style asserts the two are the same kind of fact. They are not, and a reader
//! who takes a co-change edge for a dependency will refactor against a coincidence.
//!
//! The pairing a reader most wants — "no import between them, yet they always change together" — is
//! therefore a THIRD question, and the honest way to answer it is a third picture over the intersection,
//! not a blend of two truth conditions in this one.
//!
//! # What a node and an edge ARE
//! A node is a file that appears in at least one drawn pair; an edge is `a --- b` (undirected on
//! purpose: co-change has no direction, unlike an import) labelled with the number of commits that
//! touched both. Nothing is inferred about WHY they move together — a shared owner, a copy relationship,
//! a contract enforced by a guard elsewhere are all indistinguishable here, and the picture says so
//! rather than guessing.
//!
//! # Ordering and caps
//! The engine ranks edges by count descending (`zzop_metrics::co_change_edges`), so this module
//! truncates rather than re-sorts and the picture agrees with every other surface about which tie is
//! strongest. The census names the drop, and the "not measured" case is named SEPARATELY from the
//! "measured, nothing found" case — folding those two is the one error this domain must not make.

use std::collections::BTreeMap;

use serde_json::Value;

use super::fold::{self, Fold};

mod folded;
mod render;

use folded::fold_edges;
use render::{render, Census};

/// Default cap. Lower than `dep`'s: a co-change edge carries a weight a reader has to compare, and forty
/// weighted edges is already past the point where a flowchart reads as a picture rather than a list.
pub const DEFAULT_COCHANGE_TOP: usize = 30;

struct Edge {
    a: String,
    b: String,
    /// Commits touching BOTH ends at file granularity; under `--fold` this becomes the number of
    /// file-level pairs the box-to-box edge collapsed instead — see [`fold_edges`] for why the two
    /// cannot be the same number.
    count: u64,
    source: String,
}

/// `analyzeTrees` output -> mermaid text for the co-change domain. Pure, like its siblings.
pub(super) fn project(v: &Value, scope: Option<&str>, top: usize, fold: Fold) -> String {
    let empty = Vec::new();
    let trees = v["trees"].as_array().unwrap_or(&empty);
    let multi = trees.len() > 1;

    let mut edges: Vec<Edge> = Vec::new();
    let mut trees_measured = 0usize;
    // Per tree, the files this RUN analyzed. Git history is a different population from the working
    // tree — see `gone` in the census for what the gap means and why it is measured rather than
    // caveated.
    let mut analyzed: BTreeMap<String, std::collections::BTreeSet<String>> = BTreeMap::new();
    for t in trees {
        let source = t["sourceId"].as_str().unwrap_or("").to_string();
        if let Some(loc) = t["output"]["ir"]["loc"].as_object() {
            analyzed
                .entry(source.clone())
                .or_default()
                .extend(loc.keys().cloned());
        }
        let field = &t["output"]["coChange"];
        // `null` = git inactive or collection failed; `[]` = measured and nothing co-changed. The census
        // below reports these separately, so the distinction has to survive to here.
        let Some(list) = field.as_array() else {
            continue;
        };
        trees_measured += 1;
        for e in list {
            let (Some(a), Some(b)) = (e["a"].as_str(), e["b"].as_str()) else {
                continue;
            };
            edges.push(Edge {
                a: a.to_string(),
                b: b.to_string(),
                count: e["count"].as_u64().unwrap_or(0),
                source: source.clone(),
            });
        }
    }

    // An edge is in scope when EITHER endpoint is — a tie reaching out of the scoped area is exactly what
    // a reader scoping to a folder wants to see, not something to hide.
    let keep = |e: &Edge| match scope {
        None => true,
        Some(p) => e.source.starts_with(p) || e.a.starts_with(p) || e.b.starts_with(p),
    };
    let total = edges.len();
    edges.retain(keep);
    let scoped = edges.len();

    // How many endpoints in this picture are NOT in the run's analyzed file set. Measured, not asserted:
    // git history and the working tree are different populations, and a path can be in the first and not
    // the second for three reasons a picture cannot tell apart (deleted, excluded by config, or simply
    // not a source file zzop parses). Folding made this impossible to ignore — `packages/napi`, removed
    // in v0.16.0, drew as one of the largest boxes on this repo's own graph — but it was equally true of
    // the file-level picture, which said nothing.
    let (endpoints, gone) = {
        let mut seen: std::collections::BTreeSet<(&str, &str)> = std::collections::BTreeSet::new();
        for e in &edges {
            seen.insert((e.source.as_str(), e.a.as_str()));
            seen.insert((e.source.as_str(), e.b.as_str()));
        }
        let gone = seen
            .iter()
            .filter(|(s, p)| !analyzed.get(*s).is_some_and(|set| set.contains(*p)))
            .count();
        (seen.len(), gone)
    };

    // Fold AFTER scope and BEFORE the cap — the order `super::fold` fixes for both relation domains.
    let (mut edges, fold_note) = if fold.is_on() {
        let (folded, n) = fold_edges(fold, edges);
        let note = fold::census(
            fold,
            n.file_nodes,
            n.boxes,
            n.unfoldable,
            n.file_pairs,
            folded.len(),
        );
        (folded, note)
    } else {
        (edges, String::new())
    };
    let after_fold = edges.len();
    edges.truncate(top);

    render(
        &edges,
        Census {
            total,
            scoped,
            after_fold,
            endpoints,
            gone,
            trees_measured,
            trees_total: trees.len(),
        },
        multi,
        scope,
        top,
        &fold_note,
        fold,
    )
}

#[cfg(test)]
mod tests;
