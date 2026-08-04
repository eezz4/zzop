//! The CAPABILITY-kind framework-recognizer half of the coverage reply — split from the parent
//! module the same way [`super::blind_spots`] is, and for the same reason: the parent file sits at
//! the 300-line source cap and each capability table is a self-contained cell with its own legend.

use serde_json::{json, Value};

/// The CAPABILITY-kind framework table, verbatim from `zzop_engine::framework_recognizers()` — each
/// row declared by the parser crate that owns the adapter and machine-bound to that adapter's own
/// code (`rule_contracts::recognizer_channels`), so this surface restates nothing by hand. Rows keep
/// the aggregator's order (grouped by owning parser, not sorted by name): a missing channel is
/// legible next to its siblings, which is the shape the aggregator's doc measured as the one that
/// ranks parsers honestly.
pub(super) fn table() -> Value {
    Value::Array(
        zzop_engine::framework_recognizers()
            .iter()
            .map(|r| {
                json!({
                    "framework": r.framework,
                    "extensions": r.extensions,
                    "emits": r.emits,
                })
            })
            .collect(),
    )
}

/// The one sentence `frameworkRecognizers` needs to be self-describing, shipped beside it — the
/// `blindSpotMeaning` discipline. Both directions of misreading are named: presence is not idiom
/// completeness, and absence means no recognizer EXISTS in this build (the pre-first-run question no
/// per-run silence tripwire can answer).
pub(super) fn legend() -> Value {
    json!(
        "CAPABILITY cells: every framework recognizer compiled into this build, declared by the \
         parser crate that owns the adapter and machine-checked against its code — a fact of the \
         BUILD, true before any tree is walked, never a claim about this run or this tree. A row \
         means the recognizer RUNS on files with those extensions, not that it models every idiom of \
         the framework; a framework absent from this list has no recognizer in this build at all, so \
         its routes/calls/tables contribute nothing to the cross-layer join no matter what the tree \
         contains — that absence, not any per-run warning, is the answer to \"does this tool know my \
         stack?\". `emits` names the channels the recognizer fills (io.provides = route/handler \
         declarations, io.consumes = outbound calls, io.provides:db-table = table/model facts, \
         evidence.auth-guarded = auth-guard evidence feeding route-auth exemptions): a language whose \
         rows fill only one side of the join sees only half of every conversation, which no row count \
         shows. Rows keep their owning parser's declaration order rather than a name sort, so a \
         missing channel is legible next to its siblings."
    )
}
