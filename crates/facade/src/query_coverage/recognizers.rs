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

/// The `framework-recognizers` CONTRACT document — the same capability table as markdown, served
/// run-free.
///
/// # The question this answers, and why no existing surface answered it
///
/// External review round 20 (ledger V225 ⑸): *"there is no way to know whether zzop reads my Go
/// router, my Django URLconf or my MyBatis XML **before running it**."* Every channel that carried
/// this table needed a tree first — [`table`] rides in the `coverage` reply, and every
/// framework-silence tripwire is per-run and fires only on a tree already showing the symptom. So the
/// buyer's first question had to be answered by running the tool on their own repository and reading
/// what came back, which is the one thing they were deciding whether to do.
///
/// The table itself was always run-free — `zzop_engine::recognizers`' own doc calls it "CAPABILITY-kind
/// ('this build can/cannot see X'), independent of any run". Only its delivery was not.
///
/// # Rendered, never embedded
///
/// The third RENDERED contract row, for the same reason as the two before it: there is no file to
/// `include_str!`, and a checked-in copy would be a second hand-maintained list of what this binary can
/// read — which is exactly the drift the aggregator exists to prevent. `resources/list`'s description,
/// the `coverage` reply's cell and this document are three views of one compiled-in registry.
///
/// Rows keep the aggregator's order (grouped by owning parser) rather than sorting by name, because a
/// parser missing a whole CHANNEL is legible next to its siblings and invisible in an alphabet.
pub fn contract_text() -> String {
    let mut out = String::from(
        "# Framework recognizers\n\n\
         Every framework recognizer compiled into THIS build, grouped by the parser crate that owns \
         the adapter. This is a fact of the build: it is true before any tree is walked, and answers \
         \"will zzop read my stack\" without running an analysis.\n\n\
         **A row means the recognizer runs on files with those extensions. It does not mean every \
         idiom of that framework is modelled** — each adapter's own module doc states its recognized \
         shapes and its documented gaps. **An absent framework means no recognizer for it exists in \
         this build at all**, which is the one question no per-run silence tripwire can answer: those \
         fire on a tree already showing the symptom.\n\n\
         `emits` names the cross-layer channel each recognizer fills. A language with route \
         recognizers and no consume-side one contributes only half of a cross-layer join, and that \
         asymmetry is why this column is a column rather than prose.\n\n\
         | framework | extensions | emits |\n|---|---|---|\n",
    );
    for r in zzop_engine::framework_recognizers() {
        out.push_str(&format!(
            "| {} | {} | {} |\n",
            r.framework,
            r.extensions.join(", "),
            r.emits.join(", "),
        ));
    }
    out.push_str(
        "\nThe per-run counterpart is the `coverage` reply's `ioChannels` block, which says what a \
         given tree actually YIELDED through these recognizers — a filled row here with a zero there \
         is an extraction gap on that tree, not a missing capability.\n",
    );
    out
}
