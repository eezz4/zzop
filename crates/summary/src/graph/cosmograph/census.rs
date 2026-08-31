//! The stderr CENSUS of [`super`] — what the person who just ran the command is told about the two
//! tables they are about to drag into a viewer.
//!
//! Split out of the parent on 2026-08-31 for the per-file line cap, and the seam is the one the parent's
//! own module doc already draws: stdout is a DATA TABLE a viewer parses, and every honest-channel
//! sentence goes to stderr instead so a `%%`-style comment can never corrupt a row. The emitters next
//! door decide what is IN the tables; this file decides what a reader is TOLD about them — the same
//! split `dep/render.rs` makes for the mermaid lane, which is what makes "did we disclose it?" a
//! one-file question on both lanes.
//!
//! Every sentence here is COMPUTED from the run and none is asserted, and the two that belong to the
//! whole engine rather than to this lane are re-exported constants, never paraphrases:
//! `DEP_GRAPH_RESOLVED_ONLY` (which graph the degree columns describe) and
//! `CYCLE_GRAPH_EXCLUDES_ERASED_IMPORTS` (that the cycle columns were computed over a smaller edge set
//! than the rows carry).

use super::super::dep::GitWindows;

/// What the CLI prints on stderr. Computed, never asserted — the same rule the mermaid census follows.
pub(in crate::graph) struct CosmographCensus {
    pub(in crate::graph) nodes_emitted: usize,
    pub(in crate::graph) total_nodes: usize,
    pub(in crate::graph) links_emitted: usize,
    pub(in crate::graph) total_edges: usize,
    pub(in crate::graph) cycles: usize,
    /// See [`super::dep::DepUniverse::mutual_outside_cycles`]. On BOTH tables: the links table is where
    /// a reader SEES the two-way pair, the points table is where they read `inCycle: false` about it.
    pub(in crate::graph) mutual_outside_cycles: usize,
    pub(in crate::graph) scoped: bool,
    /// How many emitted rows actually carried each measured axis — `None` for the LINKS table, which
    /// has no node axes and would be describing a table its reader is not looking at.
    pub(in crate::graph) measured: Option<MeasuredAxes>,
    /// Which git window the history COLUMNS were measured over — `None` for the LINKS table for the
    /// same reason `measured` is: it carries no git column, so it has no window to caveat. See
    /// [`super::dep::GitWindows`].
    pub(in crate::graph) window: Option<GitWindows>,
}

/// Emitted-row counts for the axes that can be absent. Omitting an unmeasured axis is honest but
/// SILENT — the viewer just has one fewer column to offer — so the count rides the census, which is
/// this lane's honesty channel. `0 of N` is the answer to "where did colour-by-churn go?".
pub(in crate::graph) struct MeasuredAxes {
    pub(in crate::graph) loc: usize,
    pub(in crate::graph) git: usize,
}

impl CosmographCensus {
    /// One line, shaped for a terminal rather than for a parser — its reader is the person who just ran
    /// the command and is about to drag the file into a viewer.
    pub(in crate::graph) fn render(&self) -> String {
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
        // Printed only when the two axes actually disagree in this run: a caveat that rides every census
        // is read as boilerplate by the third run, and a COUNT that appears exactly when the reader can
        // go find the rows it describes is the one they believe.
        let noncycle_note = if self.mutual_outside_cycles == 0 {
            String::new()
        } else {
            format!(
                " NOTE — {} file pair(s) in this graph import EACH OTHER while at least one end is in \
                 no reported cycle: {}",
                self.mutual_outside_cycles, zzop_facade::CYCLE_GRAPH_EXCLUDES_ERASED_IMPORTS
            )
        };
        format!(
            "zzop graph --domain dep --format cosmograph: {} of {} files, {} of {} import edges, \
             {} circular finding(s){scope_note}. UNCAPPED — --top does not apply to this format.\
             {axes_note}{window_note}{noncycle_note} {}",
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
