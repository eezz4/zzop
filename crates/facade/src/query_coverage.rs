//! `queryCoverage` — "how much of THIS tree does zzop actually see?", computed by pure post-processing
//! over an already-produced `analyzeTrees` output. Same contract as [`super::query_file`]: no
//! re-analysis, no cache interaction, one core shared by every host.
//!
//! # The three-value cell rule (2026-07-31 user ruling — the design IS the honesty policy)
//! Every fact this reply carries is one of exactly three kinds, and each kind may only say what its
//! source can back:
//!
//! | kind         | source                    | may say                                    |
//! |--------------|---------------------------|--------------------------------------------|
//! | MEASURED     | this run's output         | "in this tree it was N"                    |
//! | CAPABILITY   | code, independent of runs | "this build can/cannot see X"              |
//! | UNMEASURED   | the schema itself         | "never measured — absence of data, not 0"  |
//!
//! **There is deliberately NO single score field, and one must never be added.** Folding the axes into
//! one number would have to either include the unmeasured axis (recall) — manufacturing a claim — or
//! exclude it, in which case the number gets quoted without its exclusion list and reads as "zzop sees
//! N% of my repo" with the missing axis being exactly the one that matters. The `unmeasured` array is a
//! FIELD, not a caveat sentence, precisely so it cannot be dropped in transit the way prose is.
//!
//! The failures this surface closes were all measured on real trees (2026-07): a 91-file Python tree
//! sat at 3 import edges for months because no output gave a per-extension baseline to read 3 against;
//! and 0 findings under a TypeScript-only recognizer reads as "no bug" when it means "not analyzed" —
//! the per-extension dispatch table is the run-level fact that makes both visible. The per-RULE
//! sightline half of the second failure is the CAPABILITY-kind `blindSpots` cell, which was
//! deliberately absent until it could be DERIVED from rule metadata rather than restated by hand — it
//! now is: [`blind_spots`] crosses `zzop_engine::rule_sightlines` (each declaration living WITH its
//! rule, built from the same pinned claim constants the finding prose uses) with the tree's measured
//! extension mix, so `docs/rules/catalog.md`'s sightline prose is never hand-copied here.
//!
//! # Extensions, not language names
//! Files group by their extension (the tail after the last `.`, lowercased), NOT by a language label.
//! Mapping `rs -> "rust (parser-rust)"` here would require a second copy of the engine's dispatch
//! table, and a facade copy is exactly the kind of shadow table this repo keeps finding stale. The
//! extension is a fact of the tree; which dispatch class its files landed in is a fact of the run;
//! both are derivable with no table at all.

use serde_json::{json, Map, Value};

mod blind_spots;
mod dispatch_meaning;
mod io_channels;
mod join_visibility;
mod native_roster;
pub(crate) mod recognizers;
mod tree_view;
mod unread;

/// Answers over an `analyzeTrees` output. No query parameters: the whole point is the aggregate view,
/// and a caller wanting one file has `queryFile`.
pub fn query_coverage_json(analysis_json: &str) -> Result<String, String> {
    let analysis: Value =
        serde_json::from_str(analysis_json).map_err(|e| format!("analysis JSON: {e}"))?;
    let trees = analysis
        .get("trees")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            "this analysis has no `trees` — the coverage query runs over a multi-tree analysis \
             output (the kind with a `trees` array)"
                .to_string()
        })?;

    // CAPABILITY-kind input, read once per reply: the per-rule sightline declarations compiled into
    // this build (see `blind_spots`'s module doc) — a fact of the code, not of this run.
    let sightlines = zzop_engine::rule_sightlines();
    let mut out = Map::new();
    out.insert(
        "trees".to_string(),
        Value::Array(
            trees
                .iter()
                .map(|t| tree_view::tree_view(t, &sightlines))
                .collect(),
        ),
    );
    out.insert("dispatchMeaning".to_string(), dispatch_meaning::legend());
    out.insert("blindSpotMeaning".to_string(), blind_spots::legend());
    out.insert("unreadExtensionMeaning".to_string(), unread::legend());
    // The vocabulary for the per-tree `nativeAnalyses` roster forwarded below. Root-level and stated
    // ONCE, like every other legend here: it is build-constant, so a copy per tree would be N copies
    // of one sentence that can then disagree.
    out.insert(
        "nativeAnalysesMeaning".to_string(),
        serde_json::to_value(crate::native_analyses_legend()).unwrap_or(Value::Null),
    );
    // The other CAPABILITY table this build carries, and until now the one with no user surface at
    // all: which frameworks the compiled-in parsers recognize, channel by channel. Top-level and
    // UNCROSSED with any tree, deliberately — it is a fact of the code (true before any tree is
    // walked), and crossing it with a tree's extension mix would manufacture a per-run claim the
    // declarations do not make. `blindSpots` is the crossed cell; this is the raw capability half.
    out.insert("frameworkRecognizers".to_string(), recognizers::table());
    out.insert(
        "frameworkRecognizerMeaning".to_string(),
        recognizers::legend(),
    );
    // UNMEASURED cells — a schema position, so no consumer can receive the measured axes without
    // receiving the statement of what was never measured. Fixed content by design: it changes when the
    // capability changes, not per run.
    out.insert(
        "unmeasured".to_string(),
        json!([{
            "axis": "recall",
            "note": "How many of the findings that EXIST in this tree zzop reports has never been \
                     measured on this tree. The committed detection benchmark (cases/) scores zzop's \
                     own labeled corpus, not yours — its numbers do not transfer. This is also why \
                     this reply has no single coverage score: folding measured axes into one number \
                     would present it as an answer to the question this axis leaves open."
        }]),
    );
    serde_json::to_string_pretty(&Value::Object(out)).map_err(|e| e.to_string())
}

/// The per-tree aggregation: every fact here is MEASURED (this run) except `blindSpots`, the one
/// CAPABILITY×MEASURED cross (declared sightlines × this tree's structural extensions), and the
/// `channels` sentences say what each number means for the reader instead of leaving a bare scalar to
/// be misread.
/// every other F4 test authors one side's fixture by hand.
pub(super) fn ext_of(rel: &str) -> String {
    // Delegated rather than copied: this lane and the engine's zero-extraction cross key the SAME rows
    // by extension, and the cross moved into the engine while this key stayed here — which is exactly
    // the drift the cross's own module doc was written to prevent, reintroduced one function below it.
    zzop_engine::zero_extraction::ext_of(rel)
}

#[cfg(test)]
mod tests;
