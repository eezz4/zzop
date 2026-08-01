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
//! # Every axis always, and a missing one is a missing KEY
//! The points table carries every axis a viewer can style by, on every run — there is no flag that adds
//! or removes a column, because a schema that changes per invocation is one no consumer can write
//! against. What varies instead is whether a row HAS a given key: an axis this run did not measure
//! (`loc` for a file outside the scanned set, the git axes on a run with git collection off) is OMITTED,
//! never written as `0`. `churn: 0` would say "this file never changed" in the same bytes as "nobody
//! looked", and a viewer colouring by it would draw the second as the first — the silent-failure shape
//! this repo refuses everywhere else. A viewer offers the columns its rows actually contain, so an
//! absent axis correctly disappears from the styling menu instead of appearing as a flat zero field.
//!
//! # Disclosure goes to stderr, not into the table
//! stdout here is a DATA TABLE that a viewer parses; a `%%` census line like mermaid's would be a corrupt
//! row. The census is therefore returned separately for the CLI to print on stderr, which keeps the
//! honesty channel intact without putting prose in a column. There is no truncation to disclose — the
//! lane is uncapped — so what it reports is the scope filter, the totals, which measured axes actually
//! rode, and WHICH GIT WINDOW the history columns were summed over (`super::dep::window` — a churn is a
//! different number over 90 days than over a repo's whole life, and a row cannot say so itself).
//!
//! It also reports WHICH graph the `fanIn`/`fanOut`/`degree` columns describe. Those names are
//! graph-theoretic and correct about the graph they measure, so they were not renamed when the census's
//! `resolvedImportEdges` was (2026-07-31); the missing fact was that this graph holds resolved in-tree
//! edges only. That sentence has one owner (`zzop_core::DEP_GRAPH_RESOLVED_ONLY`, re-exported through
//! `zzop-facade`) and rides the census here rather than being copied into every column's description.

use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

use super::dep::{node_in_scope, DepUniverse, GitWindows};

/// What the CLI prints on stderr. Computed, never asserted — the same rule the mermaid census follows.
pub(super) struct CosmographCensus {
    pub(super) nodes_emitted: usize,
    pub(super) total_nodes: usize,
    pub(super) links_emitted: usize,
    pub(super) total_edges: usize,
    pub(super) cycles: usize,
    pub(super) scoped: bool,
    /// How many emitted rows actually carried each measured axis — `None` for the LINKS table, which
    /// has no node axes and would be describing a table its reader is not looking at.
    pub(super) measured: Option<MeasuredAxes>,
    /// Which git window the history COLUMNS were measured over — `None` for the LINKS table for the
    /// same reason `measured` is: it carries no git column, so it has no window to caveat. See
    /// [`super::dep::GitWindows`].
    pub(super) window: Option<GitWindows>,
}

/// Emitted-row counts for the axes that can be absent. Omitting an unmeasured axis is honest but
/// SILENT — the viewer just has one fewer column to offer — so the count rides the census, which is
/// this lane's honesty channel. `0 of N` is the answer to "where did colour-by-churn go?".
pub(super) struct MeasuredAxes {
    pub(super) loc: usize,
    pub(super) git: usize,
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
        let axes_note = match &self.measured {
            None => String::new(),
            Some(m) => format!(
                " Measured axes: loc on {} of {} row(s), git history on {} of {} — an axis this run \
                 did not measure is an ABSENT column, never a zero.",
                m.loc, self.nodes_emitted, m.git, self.nodes_emitted
            ),
        };
        // COVERAGE (`axes_note`) and WINDOW are two different questions about the same columns: how
        // many rows got them, and what the numbers on those rows are sums over. A row can have all
        // four git columns and still mean 90 days rather than a lifetime.
        let window_note = match &self.window {
            None => String::new(),
            Some(w) => format!(" {}", w.note()),
        };
        format!(
            "zzop graph --domain dep --format cosmograph: {} of {} files, {} of {} import edges, \
             {} circular finding(s){scope_note}. UNCAPPED — --top does not apply to this format.\
             {axes_note}{window_note} {}",
            self.nodes_emitted,
            self.total_nodes,
            self.links_emitted,
            self.total_edges,
            self.cycles,
            // The `fanIn`/`fanOut`/`degree` columns this lane emits are graph-theoretic terms and are
            // correct ABOUT the graph they describe — so they are NOT renamed. What needed saying is
            // WHICH graph that is, and it is said once, from one owner (2026-07-31). See
            // `zzop_core::DEP_GRAPH_RESOLVED_ONLY`.
            zzop_facade::DEP_GRAPH_RESOLVED_ONLY
        )
    }
}

/// The in-scope node ids, computed once so the nodes table and the links table cannot disagree about
/// what `--scope` kept.
fn kept_ids<'a>(u: &'a DepUniverse, scope: Option<&str>) -> BTreeSet<&'a String> {
    u.nodes
        .iter()
        .filter(|(id, n)| node_in_scope(scope, id, &n.source, &n.rel))
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
    let mut measured = MeasuredAxes { loc: 0, git: 0 };
    for (id, n) in &u.nodes {
        if !kept.contains(id) {
            continue;
        }
        let (fan_in, fan_out) = deg.get(id).copied().unwrap_or((0, 0));
        // A basename for the screen and the full path for identification — both, because a viewer shows
        // the label always and the row on demand.
        let (folder, label) = match n.rel.rsplit_once('/') {
            Some((dir, base)) => (dir, base),
            None => ("", n.rel.as_str()),
        };
        let mut row = json!({
            "id": id,
            "source": n.source,
            "path": n.rel,
            "label": label,
            "folder": folder,
            "fanIn": fan_in,
            "fanOut": fan_out,
            "degree": fan_in + fan_out,
            "inCycle": u.cycle_files.contains(id),
        });
        // MEASURED axes are appended, never defaulted. Everything above is derived from the graph
        // itself, so it exists for every node by construction; everything below comes from a
        // measurement that can be missing, and the two cases must not share a value. See `DepNode`'s
        // doc — a missing key is "nobody looked", `0` would be a claim. A viewer reads its columns from
        // the rows it actually got, so an absent axis simply is not offered as a styling choice, which
        // is the correct outcome.
        if let Some(obj) = row.as_object_mut() {
            if let Some(loc) = n.loc {
                measured.loc += 1;
                obj.insert("loc".to_string(), json!(loc));
            }
            if let Some(g) = &n.git {
                measured.git += 1;
                obj.insert("changeCount".to_string(), json!(g.change_count));
                obj.insert("churn".to_string(), json!(g.churn));
                obj.insert("authorCount".to_string(), json!(g.author_count));
                if let Some(last) = &g.last_modified {
                    obj.insert("lastModified".to_string(), json!(last));
                }
            }
        }
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
        measured: Some(measured),
        window: Some(u.git_windows.clone()),
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
            // The ENDPOINT fact the mermaid lane draws as a thick arrow, kept as a column so the same
            // distinction survives into a viewer that has no arrow styles.
            //
            // RENAMED from `inCycle` (2026-07-31, user ruling) because the old name claimed an EDGE fact
            // this lane does not compute. `cycle_files` is the UNION of every reported cycle's members,
            // flattened at `dep::collect` time, so "both ends are cycle members" is true for a chord
            // between two members of one cycle and for an edge BRIDGING two different cycles — neither of
            // which lies on a cycle. The new name states the membership rule the code actually applies.
            // The NODE-level `inCycle` is untouched: there the claim and the computation agree.
            "endpointsInCycle": u.cycle_files.contains(a) && u.cycle_files.contains(b),
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
        // The links table carries no node axes, so it has no coverage to report — see `measured`'s doc.
        measured: None,
        // ... and for the same reason no git window: `source`/`target`/`endpointsInCycle` are read off
        // the graph, and none of them is a history number a window could have scoped.
        window: None,
    };
    (out, census)
}

#[cfg(test)]
mod tests;
