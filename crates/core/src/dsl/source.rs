//! Rule interpreter input types (`RuleContext`/`SourceFile`), per-rule timing, and
//! minified/generated-file detection.

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

#[derive(Debug, Clone)]
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

/// A file is "minified/generated" iff EITHER prong holds:
///
/// 1. **Absolute prong**: any single line is 5000+ bytes long — never hand-written, regardless of how
///    small a fraction of the file it is.
/// 2. **Ratio prong**: any line is 500+ bytes long AND 500+ byte lines account for at least 50% of the
///    file's total bytes — long lines DOMINATE, the signature of bundler/generated output.
///
/// The ratio prong exists because a plain "any 500+ char line" rule causes collateral damage: an ordinary
/// hand-written file can happen to have one long comment or string literal among hundreds of normal
/// lines, and flagging on that alone would silently drop its entire DSL coverage.
///
/// Computed once per file. When true, the engine skips ALL DSL rule-pack evaluation for the file; native
/// structural extraction (symbols/imports/IO) is unaffected.
pub fn is_minified_or_generated(text: &str) -> bool {
    const LONG_LINE: usize = 500;
    const BLOB_LINE: usize = 5000;
    let mut total_bytes: usize = 0;
    let mut long_line_bytes: usize = 0;
    let mut has_long_line = false;
    for line in text.split('\n') {
        let len = line.len();
        total_bytes += len;
        if len >= BLOB_LINE {
            return true;
        }
        if len >= LONG_LINE {
            has_long_line = true;
            long_line_bytes += len;
        }
    }
    // Ratio prong: long lines must dominate (>= 50% of total bytes). `total_bytes == 0` (empty file)
    // never reaches a `true` here: `has_long_line` is false. Integer math, no float.
    has_long_line && long_line_bytes * 2 >= total_bytes
}

#[cfg(test)]
mod minified_tests {
    use super::is_minified_or_generated;

    #[test]
    fn normal_short_line_file_is_not_minified() {
        let text = "const x = 1;\nfunction f() {\n  return x;\n}\n";
        assert!(!is_minified_or_generated(text));
    }

    #[test]
    fn a_single_long_line_dominating_a_tiny_file_is_minified() {
        let text = format!(
            "const short = 1;\nconst bundled = \"{}\";\n",
            "x".repeat(600)
        );
        assert!(is_minified_or_generated(&text));
    }

    #[test]
    fn one_long_comment_line_inside_a_large_normal_file_is_not_minified() {
        let long_comment = format!("// {}", "word ".repeat(114)); // 573 bytes, >= 500
        assert!(long_comment.len() >= 500 && long_comment.len() < 600);
        let normal_line = "const someOrdinaryVariable = computeSomething();"; // ~49 bytes
        let mut text = String::new();
        for _ in 0..50 {
            text.push_str(normal_line);
            text.push('\n');
        }
        text.push_str(&long_comment);
        text.push('\n');
        for _ in 0..50 {
            text.push_str(normal_line);
            text.push('\n');
        }
        assert!(
            !is_minified_or_generated(&text),
            "one long comment line among 100 normal lines must not classify the file as minified"
        );
    }

    #[test]
    fn a_5000_char_blob_line_inside_a_large_normal_file_is_minified() {
        // The absolute prong fires even though the ratio prong alone would not (~5000 long-line bytes vs
        // ~14700 normal bytes is well under 50% dominance).
        let blob = "x".repeat(5000);
        let normal_line = "const someOrdinaryVariable = computeSomething();";
        let mut text = String::new();
        for _ in 0..150 {
            text.push_str(normal_line);
            text.push('\n');
        }
        text.push_str(&blob);
        text.push('\n');
        for _ in 0..150 {
            text.push_str(normal_line);
            text.push('\n');
        }
        assert!(is_minified_or_generated(&text));
    }

    #[test]
    fn a_499_char_line_is_the_boundary_and_is_not_minified() {
        let line = "x".repeat(499);
        assert_eq!(line.len(), 499);
        let text = format!("{line}\n");
        assert!(!is_minified_or_generated(&text));
    }

    #[test]
    fn a_500_char_line_that_dominates_is_the_boundary_and_is_minified() {
        let line = "x".repeat(500);
        assert_eq!(line.len(), 500);
        let text = format!("{line}\n");
        assert!(is_minified_or_generated(&text));
    }

    #[test]
    fn a_trailing_carriage_return_near_the_boundary_still_counts_correctly() {
        // `split('\n')` leaves a trailing `\r` on each line, so a line whose visible content is exactly
        // 499 chars becomes 500 bytes once its `\r` is counted, tripping the threshold a character
        // earlier than LF source would.
        let visible = "x".repeat(499);
        let text = format!("{visible}\r\n");
        assert!(
            is_minified_or_generated(&text),
            "a 499-char line plus a trailing \\r from CRLF must reach the 500-byte threshold"
        );
    }

    #[test]
    fn an_empty_file_is_not_minified() {
        assert!(!is_minified_or_generated(""));
    }
}
