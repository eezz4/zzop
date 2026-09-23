//! The process-wide parse counter behind [`super::parse_tree`] — and the record of a memo that was
//! built here, measured, and taken back out.
//!
//! ## What was measured (2026-09-08, review ledger V116)
//!
//! An external review found a C# file that never finishes: `b ? 1 : b ? 1 : …` at 20,000 terms
//! (160 KB) hangs, and on an MCP host that is a server which answers `initialize` and then answers
//! nothing, with no error and no exit. Re-measured on the release binary: 1,000 terms 1.70s · 2,000
//! 3.79s · 4,000 13.58s · 8,000 44.20s, with `ruleTimings.totalNanos` flat at 0.10–0.15s throughout.
//! Net of a ~1.3s floor the marginal cost multiplies by 6.3× · 4.9× · 3.5× per doubling — quadratic.
//! Three controls place the blame precisely: at 4,000 terms nested ternaries cost 9,967ms (32 KB),
//! while 4,000 FLAT ternaries in a 91 KB file cost 2,706ms, 4,000 nested PARENS cost 1,361ms, and
//! 4,000 chained additions cost 1,636ms. The biggest file is the cheapest per byte. It is not size and
//! it is not nesting in general; it is nested conditional expressions.
//!
//! ## The count, which is what this file keeps
//!
//! 🔴 Localizing that turned up something the review had not: **"how many times do we parse one file"
//! was a machine-checked invariant for ONE language out of eight.** TypeScript got a one-entry memo and
//! a pinned census on 2026-08-08 (`PARSES_PER_TS_FILE = 1`); C# got neither. Nine public entry points
//! reach this crate and sixteen call sites inside it reach `parse_tree`, and `parse_csharp` alone
//! parses once as a failure gate and again per sub-extractor — which its own comment stated in prose
//! while no number held it. Measured: **fourteen parses per C# file.**
//!
//! That number now lives in `crates/engine/tests/analyze_parse_census.rs` beside its TypeScript
//! sibling. It is the part of this finding a machine can hold, it costs one relaxed atomic add, and it
//! is what makes the next attempt at the quadratic term legible.
//!
//! ## The memo that is NOT here, and why
//!
//! The obvious fix was TypeScript's: a one-entry thread-local memo, since a file's extractors run
//! consecutively. It was built, it worked, and it cut the count **14 → 3** (the residual three being
//! the per-file lane's one plus the two whole-project passes, which run in their own phase over their
//! own file lists and cannot share a one-entry slot).
//!
//! 📏 **And the wall clock did not move.** Alternating A/B in one window, fresh tree per run, 4,000-term
//! ternary: without 12.26 / 9.88 / 10.11s, with 10.40 / 10.23 / 10.91s. ⚠ An earlier reading said the
//! memo was 57% SLOWER (17.7–18.6s against 10–13s) and that was an artifact — those samples were taken
//! while a release build competed for memory on a machine with 3–4 GB free. Only the alternating runs
//! are evidence. That is this repo's own rule about wall-clock contamination, arriving one round late
//! and catching the person who had just written it down.
//!
//! ⚠ The memory axis — the one this repo actually declares as its acceptance bar — was **not**
//! measured: peak working set sampled 44 MB then 20 MB for the same binary on the same input, so the
//! instrument was wrong and no number is claimed from it.
//!
//! ⇒ Removing eleven of fourteen parses changes nothing measurable, which is itself the finding: **the
//! quadratic term is not the repeated parsing.** Keeping a correct, tested, effect-free construct is
//! debt by this repo's own rule, so the memo went and this note stays in its place.
//!
//! 🔵 **Revival condition**, so the next person does not re-derive it: a population where the parse
//! COUNT dominates rather than one deep file — many medium C# files, where fourteen full trees per file
//! is fourteen allocations rather than one — or a machine where peak RSS is measurable, since that is
//! the axis a memo would actually be defended on. Neither was available here.

use std::sync::atomic::{AtomicU64, Ordering};

/// Process-wide count of tree-sitter parses of C# source. [`super::parse_tree`] is the ONLY place this
/// crate hands text to tree-sitter, so incrementing there counts every parse whatever public entry
/// point asked for it.
///
/// `Relaxed`, for the same reason the TypeScript counter uses it: this is a census, not a
/// synchronization point, and a reader wanting an exact number serializes the run it measures.
static PARSE_COUNT: AtomicU64 = AtomicU64::new(0);

/// How many times this process has parsed C# source so far.
///
/// Exists because the per-file parse count is a fact a prose comment cannot hold: `lib.rs` said "each
/// sub-call below re-parses independently", which was true and which nothing could regress on.
pub fn parse_count() -> u64 {
    PARSE_COUNT.load(Ordering::Relaxed)
}

/// Resets [`parse_count`] to 0 and returns the previous value — for a measurement that wants a clean
/// window. Mirrors `zzop_parser_typescript::reset_parse_count`.
pub fn reset_parse_count() -> u64 {
    PARSE_COUNT.swap(0, Ordering::Relaxed)
}

/// Counts one parse. Called by [`super::parse_tree`] and nowhere else.
pub(crate) fn record_parse() {
    PARSE_COUNT.fetch_add(1, Ordering::Relaxed);
}

// ## Why the count is NOT pinned here
//
// It was, for about ten minutes, and the test failed with `left: 1, right: 3` — which is the whole
// reason `crates/engine/tests/analyze_parse_census.rs` is a standalone binary. `PARSE_COUNT` is
// process-global; cargo runs this crate's tests concurrently on several threads; and most of them
// parse C#. So `reset_parse_count()` in one test lands inside another test's window and the number
// read back is somebody else's. A local unit test cannot own this fact however it is written.
