//! `ioChannels` — the per-CHANNEL half of the coverage reply, and the two facts a whole-contribution
//! zero cannot state. Split from the parent module the same way [`super::blind_spots`] is, and for
//! the same reason: the parent file sits near the 300-line source cap and this is a self-contained
//! cell with its own legend.
//!
//! # Why a kind-agnostic zero hides a channel
//! The census's `joinContributionZero` asks ONE question of the whole io contribution: did this tree
//! extract any provide or any keyed consume at all. Measured on gogs (`d460e50`): 12 GORM
//! `db-table` provides made that answer `false` — and `joinVisibility` therefore say "This tree
//! contributed joinable io" — while the http provide channel sat at 0 across 301 structural `.go`
//! files carrying hundreds of hand-counted route registrations. One channel's fill vouched for
//! another channel's emptiness. [`extracted`] carries one row per io kind this build's rules read
//! ([`zzop_core::RULE_READ_IO_KINDS`]), PRESENT EVEN AT ZERO, so the two are read separately.
//!
//! # Why the empty channel is named by LANGUAGE, not by framework
//! The pre-existing zero-route disclosures are all conditioned on RECOGNITION: the S2 tripwire fires
//! on a hand-typed server-framework import list, and the S8 call-graph gap iterates the routes that
//! were EXTRACTED. Both go silent exactly when the gap is largest — a framework outside the list
//! (gogs' `gopkg.in/macaron.v1`) extracts nothing, so nothing is left to condition on.
//! [`zero_extraction`] is keyed on what the TREE contains crossed with what the BUILD declares:
//! this build has a recognizer filling channel C for extension E (`frameworkRecognizers`, each row
//! machine-bound to its adapter's code), the tree has structural files of E, and extraction of C
//! from E returned 0. No framework name is matched, so every unlisted and every future framework is
//! covered at once.
//!
//! # Why it is a FACT and not a warning
//! That derivation fires on a Go CLI and on a pure frontend too — trees that legitimately serve no
//! HTTP. As a `warnings` entry it would be noise that trains readers to skip the channel; as a
//! coverage cell it is simply true and still useful there ("0 http provides from 301 structural .go
//! files" is the honest provide-side contribution of a CLI). So it ships here, on the surface whose
//! whole contract is per-tree facts, and never as an alarm.

use std::collections::BTreeMap;

use serde_json::{json, Value};

/// The db-kind carve-out and the io-side channel table both live on the shared cross now — see
/// [`zzop_engine::zero_extraction`] for why the RULE has one owner while each lane measures its own
/// inputs.
use zzop_engine::zero_extraction::{db_kind, io_side_channels};

/// Per-extension count of the io records on one side that fall inside (or outside) the db kind.
fn by_ext(records: Option<&Vec<Value>>, want_db: bool) -> BTreeMap<String, usize> {
    let mut out: BTreeMap<String, usize> = BTreeMap::new();
    for r in records.into_iter().flatten() {
        let is_db = r.get("kind").and_then(Value::as_str) == Some(db_kind());
        if is_db != want_db {
            continue;
        }
        let Some(file) = r.get("file").and_then(Value::as_str) else {
            continue;
        };
        *out.entry(super::ext_of(file)).or_default() += 1;
    }
    out
}

/// MEASURED per-kind fill, one row for EVERY kind in [`zzop_core::RULE_READ_IO_KINDS`] whether or not
/// this tree carried any — an absent row would put "this kind was empty" and "this kind does not
/// exist" back into the same shape. Consume counts keep the census's own keyed/unresolved split, so
/// the two surfaces are comparable field for field.
///
/// A tree whose `ir.io` is absent entirely (the field is `skip_serializing_if = "Option::is_none"`)
/// reads as all zeros here, exactly as `CoverageCensus::compute` already treats it.
fn extracted(provides: Option<&Vec<Value>>, consumes: Option<&Vec<Value>>) -> Value {
    let count = |records: Option<&Vec<Value>>, kind: &str, keyed: Option<bool>| -> usize {
        records
            .into_iter()
            .flatten()
            .filter(|r| r.get("kind").and_then(Value::as_str) == Some(kind))
            .filter(|r| keyed.is_none_or(|want| r.get("key").is_some_and(|k| !k.is_null()) == want))
            .count()
    };
    Value::Array(
        zzop_core::RULE_READ_IO_KINDS
            .iter()
            .map(|kind| {
                json!({
                    "kind": kind,
                    "provides": count(provides, kind, None),
                    "consumesKeyed": count(consumes, kind, Some(true)),
                    "consumesUnresolved": count(consumes, kind, Some(false)),
                })
            })
            .collect::<Vec<_>>(),
    )
}

/// The CAPABILITY x MEASURED cross — module doc. One row per `(channel, extension)` where this build
/// declares at least one recognizer filling that channel on that extension, the extension is a
/// PRINCIPAL filetype of the tree, and extraction of that channel from that extension came back 0.
///
/// # Why a principal-share floor, and why THAT one
/// Without it the cross is arithmetically true and practically unreadable: measured 2026-08-20, zzop's
/// own tree produced 25 rows of which 22 named an extension under 10% of it (`cs` at 9 files, `tsx` at
/// 4), and directus produced a row for a 2-file `.cjs`. A fact table whose rows are dominated by
/// rounding-error extensions buries the one row that carries the finding — on gogs, `io.provides`/`go`
/// at 301 files, which is ~300 unextracted macaron routes.
///
/// Measured with the floor in place, on four trees: directus 10 -> 1, redash 10 -> 8, gogs 11 -> 4,
/// zzop's own repo 25 -> 4. Every signal the audits turned on survives — gogs' `go` and redash's `py`
/// both clear it comfortably. ⚠ These four pairs are a HAND-COPIED measurement and this doc has already
/// been stale once: its first version carried 10 -> 5 / 11 -> 2 / 25 -> 3, which were the numbers a
/// different denominator would have produced, written before the denominator below was settled and not
/// re-measured after. Re-derive rather than trust: `zzop coverage <tree>` and count
/// `trees[].ioChannels.zeroExtraction`.
///
/// The constant is [`zzop_engine::MIN_UNCOVERED_EXTENSION_SHARE_PCT`], not a second number: the engine's
/// `NO loaded DSL rule targets …` and `THIN DSL rule reach` reports and this surface's own
/// `unreadExtensions` cell all ask "is this a filetype the tree is MADE OF", and that question has one
/// answer.
///
/// The DENOMINATOR differs from `unreadExtensions`' on purpose, and the difference follows from what
/// each cell is about. That cell's subject is filetypes NOT read structurally, so it must divide by
/// every file in the tree — a language with no parser has no structural population to be a share of.
/// This cell's subject is filetypes that WERE read and still yielded nothing, so its share is taken
/// against the structurally-read population: a repo that is 80% images does not thereby make its own
/// source a minor filetype. Both keep every measured signal (`go` is 11.4% of gogs by all files and
/// 57% by structural ones), so this is a statement about which question each cell asks, not a tuning
/// knob — and neither number is hand-written, both being sums of columns the reply already renders.
///
/// Determinism: the `(channel, ext)` key space is a `BTreeMap` and the recognizer names a
/// `BTreeSet`, so the array is sorted on its own key pair rather than on the tree's file order.
fn zero_extraction(
    structural_files: &BTreeMap<String, usize>,
    provides: Option<&Vec<Value>>,
    consumes: Option<&Vec<Value>>,
) -> Value {
    // MEASURE here, JUDGE there. This lane's substrate is an assembled tree's io arrays, so counting
    // is its own job; the cross that turns those counts into rows is shared with the analyze warning
    // that delivers the same fact, and lives in the engine.
    let extracted: BTreeMap<&'static str, BTreeMap<String, usize>> = io_side_channels()
        .into_iter()
        .map(|(chan, reads_provides, is_db)| {
            (
                chan,
                by_ext(if reads_provides { provides } else { consumes }, is_db),
            )
        })
        .collect();
    Value::Array(
        zzop_engine::zero_extraction::zero_extraction_rows(structural_files, &extracted)
            .into_iter()
            .map(|row| {
                json!({
                    "channel": row.channel,
                    "ext": row.ext,
                    "structuralFiles": row.structural_files,
                    "extracted": 0,
                    "recognizers": row.recognizers,
                })
            })
            .collect::<Vec<_>>(),
    )
}

/// The whole cell, with the sentence that states each array's membership rule beside it — the
/// self-describing-reply discipline `dispatchMeaning`/`blindSpotMeaning` already carry.
pub(super) fn io_channels(tree: &Value, structural_files: &BTreeMap<String, usize>) -> Value {
    let io = tree.pointer("/output/ir/io");
    let side = |name: &str| io.and_then(|io| io.get(name)).and_then(Value::as_array);
    let provides = side("provides");
    let consumes = side("consumes");
    json!({
        "extracted": extracted(provides, consumes),
        "zeroExtraction": zero_extraction(structural_files, provides, consumes),
        "meaning": meaning(),
    })
}

/// One place for both arrays' membership rules — pinned by this module's own test, so the cell can
/// never ship a number without the sentence that says what population it counts.
fn meaning() -> Value {
    json!(
        "MEASURED per-CHANNEL fill, because a kind-agnostic zero cannot tell channels apart. \
         `extracted` carries one row for every read io kind this build's rules consume \
         (`RULE_READ_IO_KINDS`), PRESENT EVEN AT ZERO: a tree with a full `db-table` channel and an \
         empty `http` one reports `joinContributionZero: false` and \"contributed joinable io\", both \
         true of the tree and both silent about its routes — read the rows, not the aggregate. \
         `zeroExtraction` crosses that measurement with this build's CAPABILITY table \
         (`frameworkRecognizers`): one row per (channel, extension) where a SINGLE recognizer row in \
         this build names both that channel and that extension, that extension is a PRINCIPAL filetype of what this run \
         read structurally (at least a tenth of the structurally-read files — the same floor the \
         `unreadExtensions` cell and the engine's reach reports apply, and the reason a 2-file `.cjs` \
         gets no row while it is still counted in `extensions`), and extraction returned 0. A filetype \
         under that floor is therefore ABSENT from this list rather than cleared by it. It is keyed on \
         the TREE and the BUILD, never on recognizing a framework by name, \
         so a framework no hand-kept vocabulary lists still produces the row — which is the point: \
         the absence of a framework-named warning is not evidence that a tree has no routes. Each row \
         is a COVERAGE FACT, not a defect claim: a CLI, a library or a frontend that serves no HTTP \
         legitimately reads 0 here. `recognizers` is the COMPLETE set this build has for that exact \
         (channel, extension) — nothing more and nothing less — and the row's 0 is relative to those \
         alone: a table or route written in an idiom none of them reads produces the SAME 0 as one \
         that does not exist, so a short list is a statement about this build's reach, not only about \
         your tree. The two readings separate on the list, and the db kind is split by SIDE for that \
         reason (`io.provides:db-table` = table DECLARATIONS, `io.consumes:db-table` = queries \
         against one); a query-side recognizer is never offered as evidence a declaration channel \
         could have been filled. A (channel, extension) pair with NO recognizer in this build \
         produces no row at all rather than an empty one — that absence is `frameworkRecognizers`' \
         and `unreadExtensions`' answer, not this cell's. Rules that read an empty channel evaluated \
         nothing on it, so read their empty findings against this list rather than as clean."
    )
}

#[cfg(test)]
#[path = "io_channels_tests.rs"]
mod tests;
