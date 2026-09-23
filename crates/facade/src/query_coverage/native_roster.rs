//! The per-tree native-analysis roster, forwarded onto the coverage reply.
//!
//! A module of its own for the reason every sibling cell here has one (`blind_spots`, `unread`,
//! `io_channels`, `join_visibility`): the file owns ONE question, and the reasoning behind it is
//! longer than the code — which is exactly what makes a composition root unreadable when it is kept
//! inline.

use serde_json::Value;

/// The tree's own native-analysis roster, FORWARDED from the analysis it was given rather than
/// recomputed — the same `.get()`-gated degradation `packsLoadedMeaning` takes, so an older engine
/// whose reply predates the field omits the key instead of writing a JSON `null`.
///
/// This surface exists to answer "how much of my stack does this tool see", and until 2026-09-04 it
/// could not answer the largest single subtraction: three analyses ship OFF (61.7% of every finding
/// across the dogfood corpus), and only the `analyze` reply said so. Reachable-in-another-lane is the
/// shape `output-philosophy.md` §0 refuses — a disclosure the reader has to already suspect is not a
/// disclosure — and it is worse here than anywhere, because THIS is the lane they opened to ask.
///
/// PER TREE, never unioned to the root. Whether a shipped-off analysis ran is settled per tree (a
/// config can turn one back on), so a root-level fold would have no honest entry for a run that opted
/// in for one tree and not another — the same reasoning `zzop_summary::cross::native_analyses` writes
/// down for the join reply, reached independently here because the gate has the same shape.
pub(super) fn native_analyses_of(tree: &Value) -> Option<Value> {
    tree.pointer("/output/nativeAnalyses").cloned()
}
