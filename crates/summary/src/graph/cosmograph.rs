//! The second serialization of `zzop graph` — NDJSON tables for an interactive viewer, alongside (never
//! instead of) the mermaid lane.
//!
//! # Why a second format here, when the join map rejected one
//! `mod.rs` rejected DOT for the JOIN domain and the reasoning holds for that domain still: a join has
//! tens of relations, mermaid draws it inline with no toolchain, and a second emitter bought nothing. The
//! DEPENDENCY domain is the case that reasoning explicitly reserved — `dep.rs`'s own module doc says a
//! mid-size repo has thousands of import edges, that drawing them all "produces a black square, which is
//! worse than drawing nothing because it looks like information", and that a genuinely large graph is
//! where a different format stays a live candidate. So this is not a re-litigation of the format
//! decision; it is the branch that decision left open, arriving.
//!
//! # The whole difference is the cap
//! Mermaid caps nodes (`--top`, default 40) because a DRAWN picture stops being readable long before it
//! stops being renderable. A viewer with zoom and filtering does that job itself, so this lane emits the
//! `DepUniverse` WHOLE. Both formats read the same [`super::dep::collect`] pass, which is what keeps them
//! from becoming two different answers about the same repo.
//!
//! # Why NDJSON rather than CSV
//! Cosmograph ingests CSV, TSV, JSON, NDJSON, Parquet and Arrow, so the choice is ours. NDJSON wins on
//! the only axis that matters here: `serde_json` already owns quoting, so a path containing a comma or a
//! quote cannot corrupt a row. A hand-rolled CSV writer would put that escaping burden in this repo, and
//! escaping bugs are silent — the file still parses, just into the wrong columns. Every other zzop
//! surface is JSON besides.
//!
//! # Two tables, two invocations
//! Cosmograph takes a links table (required) and an optional points table. They are two tables, stdout is
//! one stream, and this repo writes no files (the report lane was retired for exactly that reason), so
//! the format is the selector: `--format cosmograph-links` and `--format cosmograph-nodes`. Column NAMES
//! are ours to pick — the app maps columns interactively — so they are spelled for a human reading the
//! mapping dialog, which is also why `label` is a basename rather than the full path: this is the one
//! zzop surface whose reader is a person looking at a screen, and the self-documenting-long-name rule
//! that governs every other surface would put an unreadable string on every node.
//!
//! # Disclosure goes to stderr, not into the table
//! stdout here is a DATA TABLE that a viewer parses; a `%%` census line like mermaid's would be a corrupt
//! row. The census is therefore returned separately for the CLI to print on stderr, which keeps the
//! honesty channel intact without putting prose in a column. There is no truncation to disclose — the
//! lane is uncapped — so what it reports is the scope filter and the totals.

use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

use super::dep::{node_in_scope, DepUniverse};

/// What the CLI prints on stderr. Computed, never asserted — the same rule the mermaid census follows.
pub(super) struct CosmographCensus {
    pub(super) nodes_emitted: usize,
    pub(super) total_nodes: usize,
    pub(super) links_emitted: usize,
    pub(super) total_edges: usize,
    pub(super) cycles: usize,
    pub(super) scoped: bool,
}

impl CosmographCensus {
    /// One line, shaped for a terminal rather than for a parser — its reader is the person who just ran
    /// the command and is about to drag the file into a viewer.
    pub(super) fn render(&self) -> String {
        let scope_note = if self.scoped {
            format!(
                " (--scope dropped {} node(s) and every edge with an endpoint outside it)",
                self.total_nodes.saturating_sub(self.nodes_emitted)
            )
        } else {
            String::new()
        };
        format!(
            "zzop graph --domain dep --format cosmograph: {} of {} files, {} of {} import edges, \
             {} circular finding(s){scope_note}. UNCAPPED — --top does not apply to this format.",
            self.nodes_emitted, self.total_nodes, self.links_emitted, self.total_edges, self.cycles
        )
    }
}

/// The in-scope node ids, computed once so the nodes table and the links table cannot disagree about
/// what `--scope` kept.
fn kept_ids<'a>(u: &'a DepUniverse, scope: Option<&str>) -> BTreeSet<&'a String> {
    u.nodes
        .iter()
        .filter(|(id, (source, rel))| node_in_scope(scope, id, source, rel))
        .map(|(id, _)| id)
        .collect()
}

/// Fan-in and fan-out per node, over the UNCAPPED edge set. Degree is the size axis a reader is most
/// likely to reach for first, and computing it here rather than in the viewer means the number means the
/// same thing as the one `dep.rs` ranks by.
fn degrees(u: &DepUniverse) -> BTreeMap<&String, (usize, usize)> {
    let mut d: BTreeMap<&String, (usize, usize)> =
        u.nodes.keys().map(|k| (k, (0usize, 0usize))).collect();
    for (from, to) in &u.edges {
        if let Some(e) = d.get_mut(from) {
            e.1 += 1;
        }
        if let Some(e) = d.get_mut(to) {
            e.0 += 1;
        }
    }
    d
}

/// The points table: one line per file, carrying every axis a viewer can style by. Emission order is the
/// `BTreeMap`'s sorted order, so the bytes are stable for the same analysis.
pub(super) fn nodes_ndjson(u: &DepUniverse, scope: Option<&str>) -> (String, CosmographCensus) {
    let kept = kept_ids(u, scope);
    let deg = degrees(u);
    let mut out = String::new();
    for (id, (source, rel)) in &u.nodes {
        if !kept.contains(id) {
            continue;
        }
        let (fan_in, fan_out) = deg.get(id).copied().unwrap_or((0, 0));
        // A basename for the screen and the full path for identification — both, because a viewer shows
        // the label always and the row on demand.
        let (folder, label) = match rel.rsplit_once('/') {
            Some((dir, base)) => (dir, base),
            None => ("", rel.as_str()),
        };
        let row = json!({
            "id": id,
            "source": source,
            "path": rel,
            "label": label,
            "folder": folder,
            "fanIn": fan_in,
            "fanOut": fan_out,
            "degree": fan_in + fan_out,
            "inCycle": u.cycle_files.contains(id),
        });
        out.push_str(&row.to_string());
        out.push('\n');
    }
    let links = links_rows(u, &kept).count();
    let census = CosmographCensus {
        nodes_emitted: kept.len(),
        total_nodes: u.nodes.len(),
        links_emitted: links,
        total_edges: u.edges.len(),
        cycles: u.cycles,
        scoped: scope.is_some(),
    };
    (out, census)
}

/// Edges whose BOTH ends survived `--scope`. An edge with one end outside the filter has nothing to
/// attach to, so it is dropped rather than pointing at a node the table does not contain — the same rule
/// the mermaid lane states in its note ("an edge whose other end was dropped is not drawn").
fn links_rows<'a>(
    u: &'a DepUniverse,
    kept: &'a BTreeSet<&'a String>,
) -> impl Iterator<Item = (&'a String, &'a String)> {
    u.edges
        .iter()
        .filter(move |(a, b)| kept.contains(a) && kept.contains(b))
        .map(|(a, b)| (a, b))
}

/// The links table — the one Cosmograph actually requires. `source`/`target` are spelled the way the
/// app's mapping step already guesses, so the common case needs no mapping at all.
pub(super) fn links_ndjson(u: &DepUniverse, scope: Option<&str>) -> (String, CosmographCensus) {
    let kept = kept_ids(u, scope);
    let mut out = String::new();
    let mut emitted = 0usize;
    for (a, b) in links_rows(u, &kept) {
        let row = json!({
            "source": a,
            "target": b,
            // Both ends in a cycle is the edge-level fact the mermaid lane draws as a thick arrow. Kept
            // as a column so the same distinction survives into a viewer that has no arrow styles.
            "inCycle": u.cycle_files.contains(a) && u.cycle_files.contains(b),
        });
        out.push_str(&row.to_string());
        out.push('\n');
        emitted += 1;
    }
    let census = CosmographCensus {
        nodes_emitted: kept.len(),
        total_nodes: u.nodes.len(),
        links_emitted: emitted,
        total_edges: u.edges.len(),
        cycles: u.cycles,
        scoped: scope.is_some(),
    };
    (out, census)
}

#[cfg(test)]
mod tests;
