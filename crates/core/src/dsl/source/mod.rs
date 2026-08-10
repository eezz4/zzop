//! Rule interpreter input types (`RuleContext`/`SourceFile`) and per-rule timing.
//!
//! The minified-line-shape check that decides whether DSL rules run against a file AT ALL lives in
//! [`line_shape`], not here: it answers a question about a file's raw bytes rather than about the
//! projected facts these types carry, and keeping it in its own module is what stops it from reading as
//! one more property of `SourceFile`. Re-exported below so `zzop_core::dsl::has_minified_line_shape`
//! stays the one public path (2026-08-09: this file was split at exactly the 300-line guard limit).

mod line_shape;

pub use line_shape::has_minified_line_shape;

use serde::{Deserialize, Serialize};

use crate::{
    call_sites::CallSite, io::IoFacts, ir::SourceSymbol, string_literals::BoundStringLiteral,
};

/// Rule interpreter input — the source files a rule pack evaluates against, each already carrying its own
/// projected structural facts (`symbols`/`io`/`loop_spans`/`function_spans`/`call_sites`).
///
/// Deliberately per-file: the tree-wide `CommonIr` is NOT reachable from here. A rule that needs the
/// assembled IR is answered out of process by the `zzop facts` CLI lane, which emits the whole
/// post-assembly substrate — the only stage with an honest cache story, since per-file rules participate
/// in the engine's `ruleset_fingerprint` and no honest fingerprint exists for a user's own program.
pub struct RuleContext<'a> {
    pub files: &'a [SourceFile],
}

/// Per-rule wall-clock timing from one `eval_pack_profiled` call — the substrate for rule profiling.
/// `rule_id` is pack-prefixed (`"{pack.id}/{rule.id}"`). `nanos` varies run-to-run with timer noise, so
/// rank rules by relative cost rather than diffing raw values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleTiming {
    pub rule_id: String,
    pub nanos: u128,
    pub findings: usize,
}

/// `Default` exists for TEST construction only (`..Default::default()`), so a fixture names just the
/// facts it exercises and a new fact field costs zero edits across the test tree. The three PRODUCTION
/// construction sites (`source/mod.rs`, `envelope/file_pass.rs`, `pipeline/findings.rs`) deliberately
/// keep spelling every field: there, a fact silently defaulting to empty is a fact silently not
/// extracted, and the compiler refusing to build until the new field is wired is the whole defense.
/// Reviewers: reject `..Default::default()` in non-test `SourceFile` construction.
#[derive(Debug, Clone, Default)]
pub struct SourceFile {
    /// Normalized relative path.
    pub rel: String,
    pub text: String,
    /// Per-file symbol spans (functions/methods/classes), consumed by `Matcher::MethodScan`. Empty when
    /// the parser has no support / falls back lexically; line-scan ignores this field.
    pub symbols: Vec<SourceSymbol>,
    /// Per-file IO facts (`Matcher::IoScan`'s substrate), projected alongside `symbols`. `None` when the
    /// parser has no IO adapter / falls back lexically — io-scan rules silently skip such files.
    pub io: Option<IoFacts>,
    /// Per-file loop-body line spans (1-based, inclusive), projected alongside `symbols`. The contract
    /// every language's extractor projects onto: **a call sitting textually inside one of these spans is
    /// PROVEN to run once per iteration** — never "co-occurs with loop syntax somewhere nearby".
    ///
    /// Two span sources, split by that proof:
    /// - **Statement loops — every projecting language**: each `for`(-of/-in/-each/`range`)/`while`/
    ///   `do-while`/`loop` statement's full span (header line included — a call in the loop CONDITION
    ///   runs once per iteration too). Python's `else:` block is excluded (runs at most once).
    /// - **Callback/comprehension forms — only where EAGER evaluation is proven**: the callback ARGUMENT
    ///   of a TS array-iteration call (`.map`/`.forEach`/`.filter`/`.reduce`/... — the callback only,
    ///   not the whole call expression, so a receiver like `(await fetch(u)).items.map(...)` does not
    ///   put the one-shot `fetch` "inside" the loop), and a Python list/set/dict COMPREHENSION (eager).
    ///   **Lazy forms are deliberately SILENT — never a span**: a Python generator expression, a Rust
    ///   `.iter().map(|x| …)` adapter closure, a Java Stream lambda, a C# LINQ lambda. Each runs ZERO
    ///   times unless its pipeline is consumed, so the per-iteration proof cannot be honored; emitting a
    ///   span there would make `trigger_in_loop` findings claim iteration that may never happen. A
    ///   parser adding a new callback-shaped arm must first prove the host evaluates it eagerly — this
    ///   sentence is the boundary's owner; each parser's `loop_spans` module doc restates its own side.
    ///   Known EAGER forms not yet projected: Java `Collection.forEach`, C# `List<T>.ForEach`, Rust
    ///   `Iterator::for_each` — unimplemented, not lazy; adding one follows this doc's eager-proof
    ///   obligation. (Rust `.map` is a deliberate PERMANENT silence, not backlog: `Option::map`/
    ///   `Result::map` spell identically to `Iterator::map` and run at most once, so no lexical
    ///   receiver check can honor the per-iteration proof.)
    ///
    ///   **A SINGLE-LINE callback/comprehension span is never emitted, even when eager.** This channel
    ///   is line-granular, and a `(n, n)` span cannot prove containment: a one-shot call sharing the
    ///   callback's only line (`console.log(items.map((i) => i.id).join(','))`, `print("ids:",
    ///   [r.id for r in rows])`) is indistinguishable from the callback's own body, so emitting the
    ///   span sweeps provably-once code into "per iteration". Producers drop such spans — silence is
    ///   the never-guess direction, at the published cost of INTENDED UNDER-REPORTING (a genuine
    ///   one-line per-iteration call like `xs.forEach(x => log(x))` is lost). STATEMENT-loop one-line
    ///   spans are kept: a `stmt; for (...) f()` line-share is a rare idiom, and that residual
    ///   ambiguity is a published limit rather than a fixed one.
    ///
    /// Consumed by `MethodScan::trigger_in_loop`. Empty when the parser has no support / falls back
    /// lexically — structural rules silently skip such files (graceful degrade, same policy as
    /// `symbols`).
    pub loop_spans: Vec<(u32, u32)>,
    /// Per-file FUNCTION line spans (1-based, inclusive), projected alongside `symbols`: every
    /// function-like node (declaration/expression/arrow/method/constructor/accessor), with one merge —
    /// a function-shaped ARGUMENT of a `.then(...)`/`.catch(...)`/`.finally(...)` member call has its
    /// span START pulled up to that call's PROPERTY-token line, so a promise continuation and the
    /// boundary token scheduling it share ONE span. Nested functions overlap freely; consumers resolve
    /// the INNERMOST containing span ([`SourceFile::innermost_function_start`]).
    ///
    /// Distinct from `symbols`' body spans, which cover only DECLARED symbols (a component function,
    /// not the anonymous closures inside it) — that coarseness is exactly what this fact refines.
    /// Consumed by `MethodScan::after_in_same_function`. Empty when the parser has no support / falls
    /// back lexically — the gate then degrades to a no-op (every line resolves to `None`, so all lines
    /// count as "the same function"), NOT to silence: a rule using it keeps its pre-gate behavior on
    /// such a file. Same graceful-degrade family as `symbols`/`io`/`loop_spans`, but note the direction
    /// differs from `loop_spans` (whose absence silences `trigger_in_loop` entirely).
    pub function_spans: Vec<(u32, u32)>,
    /// Per-file TEST-ONLY line spans (1-based, inclusive): regions the parser PROVED are compiled out of
    /// the shipping build — today `zzop_parser_rust::extract_test_spans`' `#[cfg(test)]` / `#[test]`-family
    /// items, the one convention no path pattern can see because it lives INSIDE the shipping file.
    ///
    /// Unlike every other field here this one is SUBTRACTIVE, and it is applied to every DSL rule rather
    /// than opted into by one: [`SourceFile::is_test_only_line`] gates finding emission in
    /// `dsl::eval`, once, for every matcher type. See that gate's doc for why it is unconditional.
    ///
    /// Empty when the parser has no support / falls back lexically / fails to parse — the SAFE direction
    /// for a subtractive fact: nothing is subtracted, so a degraded file keeps its full judgment rather
    /// than going quiet. This is the same graceful-degrade family as `symbols`/`io`/`loop_spans`, with the
    /// degrade direction chosen by what an empty value MEANS rather than by convention.
    pub test_spans: Vec<(u32, u32)>,
    /// Per-file CALL SITES in source order — `Matcher::CallScan`'s substrate: one fact per witnessed use
    /// of an API family (console write, env read, ...), carrying the callee EXACTLY as written. See
    /// [`crate::call_sites::CallSite`] for the never-guess and no-`level`-field contracts that shape it.
    ///
    /// Degrade direction is the `loop_spans` family — RECALL, not precision. Empty (no producer for this
    /// language, degraded parse, oversized file, envelope mode) means every `CallScan` rule is SILENT
    /// here, never that the file is clean. That is the opposite of `function_spans` (absent = the gate
    /// no-ops, the rule keeps its coarser behavior) and of `test_spans` (absent = nothing subtracted), so
    /// migrating a text-scan rule onto this channel is not a free win: it trades false positives for
    /// blindness on exactly the files that were already hardest to parse.
    pub call_sites: Vec<CallSite>,
    /// Per-file BOUND STRING LITERALS in source order — `Matcher::LiteralScan`'s substrate: name +
    /// value hash + value entropy, NEVER the value ([`crate::string_literals`] owns that contract).
    /// Degrade direction is `call_sites`' exactly: empty means SILENT, never clean.
    pub string_literals: Vec<BoundStringLiteral>,
}

impl SourceFile {
    /// Whether `line` (1-based) sits inside any [`SourceFile::test_spans`] entry — "the parser proved
    /// this line is not shipped code". Linear over the spans, which are per-ITEM (one `#[cfg(test)] mod
    /// tests` is one entry, not one per function inside it), so this stays small on real files.
    pub fn is_test_only_line(&self, line: u32) -> bool {
        self.test_spans
            .iter()
            .any(|&(start, end)| start <= line && line <= end)
    }

    /// START line of the INNERMOST [`SourceFile::function_spans`] entry containing `line`, or `None`
    /// when no span does (module top level — or a file with no projected spans at all, which is what
    /// makes the gate that consumes this a no-op under graceful degrade).
    ///
    /// "Innermost" = the greatest start line; ties broken by the smallest end line. The start-first
    /// order matters for a chained continuation (`p.then(cb).catch(cb2)`), where the `.then` callback's
    /// closing line is also the `.catch` callback's opening line: neither span contains the other, and
    /// the shared line belongs to the later link.
    ///
    /// The START is what callers need, not the span identity: `MethodScan::after_in_same_function` asks
    /// "is an earlier line still INSIDE the function that encloses this one?", which — since the earlier
    /// line is by construction at or before the trigger, and the trigger is inside the span — reduces to
    /// "is it at or after this start?". Comparing span IDENTITY instead would be wrong for a line that
    /// belongs to two nested spans at once: `await import(m).then((x) => x.f());` has a merged
    /// continuation span covering only that line, so identity-matching would hide the `await` from the
    /// enclosing async function whose continuation genuinely resumes on the next line (measured on
    /// mono-hub: a real true positive lost that way).
    pub fn innermost_function_start(&self, line: u32) -> Option<u32> {
        let mut best: Option<(u32, u32)> = None;
        for &(start, end) in &self.function_spans {
            if start > line || line > end {
                continue;
            }
            let better = match best {
                None => true,
                Some((bs, be)) => start > bs || (start == bs && end < be),
            };
            if better {
                best = Some((start, end));
            }
        }
        best.map(|(start, _)| start)
    }
}
