//! The STRUCTURE/RISK domain of `zzop graph` — blast-radius hubs and extraction seams, the two
//! structural facts that are actually shaped like a graph.
//!
//! # This domain is where the 3D question finally closes, by being split rather than answered
//! The visualization decision left per-domain dimension open and named this domain as the one that
//! might genuinely need something other than a flowchart — "a whole-repo risk surface with thousands of
//! nodes and continuous dimensions". Building it shows that description covers TWO different things
//! that were being treated as one:
//!
//! - **`critical` and `seams` ARE graph-shaped.** A critical file is a hub with a blast radius; a seam
//!   is a folder with edges crossing its boundary. Nodes and edges, named, few. A flowchart is right,
//!   and 3D would add a viewer and a coordinate system to draw the same thing worse.
//! - **`scores` is NOT a graph at all.** Seventeen 0-100 health dimensions is a TABLE of numbers. It is
//!   the "continuous dimensions" half of that description, and the honest answer for it is not 3D — it
//!   is that it should not be forced into a picture at all. `zzop analyze` already carries the composite
//!   (`architecture.pain`) and `zzop facts` carries all seventeen. A flowchart of seventeen numbers is
//!   strictly worse than the table that exists.
//!
//! So the answer is not "2D wins again", it is that the thing which looked like it needed 3D was never
//! this surface's job. **3D is closed for every shipped and planned domain**, and the way to reopen it
//! is to find graph-shaped data too large for a flowchart — not numeric data, which wants a table.
//!
//! Consequently this module draws `critical` + `seams` and NAMES the omission of `scores` in the
//! document itself, the same way the join map names what it cannot render rather than letting a reader
//! infer completeness from a picture.
//!
//! # What a node IS
//! Two kinds, deliberately distinguishable without styling:
//! - a **hub** — one critical FILE, labelled with its path and blast radius, drawn as a doubled-edge
//!   node because it is the thing a reader is being warned about;
//! - a **seam** — one candidate FOLDER, labelled with its file count and boundary-edge count, drawn as a
//!   subroutine node.
//!
//! An edge means CONTAINMENT (`seam folder --> hub file inside it`), never an import. Import direction is
//! `--domain dep`'s job, and drawing two different edge meanings in one picture with one arrow style is
//! how a diagram starts lying.
//!
//! # Ordering and caps
//! Hubs come pre-ranked by the engine (`risk_score` descending); seams likewise. This module preserves
//! that order and caps by `--top` per KIND, so a long hub list cannot push every seam out of the picture
//! — the same per-bucket reasoning the join map uses, for the same reason.

use serde_json::Value;

/// Default per-kind cap. Both lists are already engine-ranked and short; this is a readability bound,
/// not a truncation policy.
pub const DEFAULT_RISK_TOP: usize = 12;

struct Hub {
    path: String,
    blast: u64,
    loc: u64,
    source: String,
}

struct Seam {
    folder: String,
    files: u64,
    boundary: u64,
    source: String,
}

/// `analyzeTrees` output -> mermaid text for the structure/risk domain. Pure, like its siblings.
pub(super) fn project(v: &Value, scope: Option<&str>, top: usize) -> String {
    let empty = Vec::new();
    let trees = v["trees"].as_array().unwrap_or(&empty);
    let multi = trees.len() > 1;
    let n = |s: &Value, k: &str| s.get(k).and_then(Value::as_u64).unwrap_or(0);

    let mut hubs: Vec<Hub> = Vec::new();
    let mut seams: Vec<Seam> = Vec::new();
    let mut scores_present = 0usize;
    for t in trees {
        let source = t["sourceId"].as_str().unwrap_or("").to_string();
        if !t["output"]["scores"].is_null() {
            scores_present += 1;
        }
        for c in t["output"]["critical"].as_array().unwrap_or(&empty) {
            let Some(path) = c["path"].as_str() else {
                continue;
            };
            hubs.push(Hub {
                path: path.to_string(),
                blast: n(c, "blastRadius"),
                loc: n(c, "loc"),
                source: source.clone(),
            });
        }
        for s in t["output"]["seams"].as_array().unwrap_or(&empty) {
            let Some(folder) = s["folder"].as_str() else {
                continue;
            };
            seams.push(Seam {
                folder: folder.to_string(),
                files: n(s, "files"),
                boundary: n(s, "boundaryEdges"),
                source: source.clone(),
            });
        }
    }

    let keep = |src: &str, path: &str| match scope {
        None => true,
        Some(p) => src.starts_with(p) || path.starts_with(p),
    };
    let total_hubs = hubs.len();
    let total_seams = seams.len();
    hubs.retain(|h| keep(&h.source, &h.path));
    seams.retain(|s| keep(&s.source, &s.folder));
    let scoped_hubs = hubs.len();
    let scoped_seams = seams.len();
    // Engine order is the ranking; truncate rather than re-sort, so the picture agrees with every other
    // surface about which hub is worst.
    hubs.truncate(top);
    seams.truncate(top);

    render(
        &hubs,
        &seams,
        Census {
            total_hubs,
            total_seams,
            scoped_hubs,
            scoped_seams,
            trees_with_scores: scores_present,
        },
        multi,
        scope,
        top,
    )
}

struct Census {
    total_hubs: usize,
    total_seams: usize,
    scoped_hubs: usize,
    scoped_seams: usize,
    trees_with_scores: usize,
}

fn label(multi: bool, source: &str, rest: &str) -> String {
    let base = if multi {
        format!("{source}::{rest}")
    } else {
        rest.to_string()
    };
    base.replace('"', "'")
}

fn render(
    hubs: &[Hub],
    seams: &[Seam],
    c: Census,
    multi: bool,
    scope: Option<&str>,
    top: usize,
) -> String {
    let mut out = String::new();
    out.push_str("%% zzop graph --domain risk — blast-radius hubs and extraction seams\n");
    out.push_str(&format!(
        "%% hubs: drawn {} / in-scope {} / total {} | seams: drawn {} / in-scope {} / total {}\n",
        hubs.len(),
        c.scoped_hubs,
        c.total_hubs,
        seams.len(),
        c.scoped_seams,
        c.total_seams
    ));
    out.push_str(&format!(
        "%% per-kind cap --top {top}{}\n",
        scope.map(|s| format!(" | --scope {s}")).unwrap_or_default()
    ));
    // Named, not inferred — the same rule the join map's header follows.
    out.push_str(&format!(
        "%% NOT drawn: the 17 structural health scores ({} tree(s) computed them). They are a table of \
         numbers, not a graph; a flowchart of them would be worse than the table. Composite in `zzop \
         analyze`'s architecture.pain, all seventeen in `zzop facts`.\n",
        c.trees_with_scores
    ));
    out.push_str("flowchart TD\n");

    for (i, s) in seams.iter().enumerate() {
        out.push_str(&format!(
            "  s{i}[[\"{}<br/>{} files, {} boundary edges\"]]\n",
            label(multi, &s.source, &s.folder),
            s.files,
            s.boundary
        ));
    }
    for (i, h) in hubs.iter().enumerate() {
        out.push_str(&format!(
            "  h{i}[[\"{}\"]]\n",
            format_args!(
                "{}<br/>blast {} · {} loc",
                label(multi, &h.source, &h.path),
                h.blast,
                h.loc
            )
        ));
    }
    // Containment only — see the module doc. A hub inside a seam folder is the one relation these two
    // lists genuinely share.
    for (hi, h) in hubs.iter().enumerate() {
        for (si, s) in seams.iter().enumerate() {
            if h.source == s.source && h.path.starts_with(&format!("{}/", s.folder)) {
                out.push_str(&format!("  s{si} --> h{hi}\n"));
            }
        }
    }

    let dropped =
        (c.scoped_hubs.saturating_sub(hubs.len())) + (c.scoped_seams.saturating_sub(seams.len()));
    let note = if dropped == 0 && scope.is_none() {
        format!(
            "complete: all {} hubs and {} seams drawn",
            c.total_hubs, c.total_seams
        )
    } else {
        format!(
            "PARTIAL VIEW: {} of {} hubs and {} of {} seams drawn ({} dropped by --top {}). Arrows mean \
             CONTAINMENT, not imports — use --domain dep for import direction.",
            hubs.len(),
            c.scoped_hubs,
            seams.len(),
            c.scoped_seams,
            dropped,
            top
        )
    };
    out.push_str(&format!("  zzopNote[\"{note}\"]\n"));
    if hubs.is_empty() && seams.is_empty() {
        out.push_str(
            "  zzopEmpty[\"No hubs or seams: this run computed none. Both need git signals — an \
             analysis with git collection off produces neither, which is NOT the same as a repo with \
             no risk.\"]\n",
        );
    }
    out
}

#[cfg(test)]
mod tests;
