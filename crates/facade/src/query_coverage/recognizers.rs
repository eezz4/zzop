//! The CAPABILITY-kind framework-recognizer half of the coverage reply — split from the parent
//! module the same way [`super::blind_spots`] is, and for the same reason: the parent file sits at
//! the 300-line source cap and each capability table is a self-contained cell with its own legend.

use serde_json::{json, Value};

/// The CAPABILITY-kind framework table, verbatim from `zzop_engine::framework_recognizers()` — each
/// row declared by the parser crate that owns the adapter, with `emits` bound to that adapter's own
/// code (`rule_contracts::recognizer_channels`) and `extensions` bound to the dispatcher
/// (`zzop_engine::recognizers`'s `every_declared_extension_routes_to_the_declaring_parser`), so this
/// surface restates nothing by hand. The extension binding landed 2026-08-26: the legend below had
/// claimed the whole row was machine-checked while that field was checked by nothing. Rows keep
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
         parser crate that owns the adapter — a fact of the \
         BUILD, true before any tree is walked, never a claim about this run or this tree. A row \
         means the recognizer RUNS on files with those extensions, not that it models every idiom of \
         the framework; a framework absent from this list has no recognizer in this build at all, so \
         its routes/calls/tables contribute nothing to the cross-layer join no matter what the tree \
         contains — that absence, not any per-run warning, is the answer to \"does this tool know my \
         stack?\". `emits` names the channels the recognizer fills (io.provides = route/handler \
         declarations, io.consumes = outbound calls, io.provides:db-table = table/model \
         DECLARATIONS, io.consumes:db-table = queries against a table, \
         evidence.auth-guarded = auth-guard evidence feeding route-auth exemptions): a language whose \
         rows fill only one side of the join sees only half of every conversation, which no row count \
         shows, and the db kind is split by side for the same reason — a recognizer that reads \
         queries is not evidence that table declarations can be read. WHAT IS MACHINE-CHECKED, \
         exactly: `emits` against the io each recognizer's own modules construct (side and kind \
         both), and `extensions` against the dispatcher, so a row cannot name an extension the \
         declaring parser never receives. What is NOT: whether a row covers every idiom of its \
         framework, and whether an adapter's own gate narrows it below its language's full extension \
         set. Rows keep their owning parser's declaration order rather than a name sort, so a \
         missing channel is legible next to its siblings."
    )
}
