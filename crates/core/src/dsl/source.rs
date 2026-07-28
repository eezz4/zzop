//! Rule interpreter input types (`RuleContext`/`SourceFile`), per-rule timing, and
//! minified/generated-file detection.

use serde::{Deserialize, Serialize};

use crate::{io::IoFacts, ir::SourceSymbol};

/// Rule interpreter input — the source files a rule pack evaluates against, each already carrying its own
/// projected structural facts (`symbols`/`io`/`loop_spans`/`function_spans`).
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
    /// Per-file loop-body line spans (1-based, inclusive), projected alongside `symbols`: each
    /// `for`/`for-of`/`for-in`/`while`/`do-while` statement's full span (header line included — a call in
    /// the loop CONDITION runs once per iteration too), plus the span of the callback ARGUMENT of an
    /// array-iteration call (`.map`/`.forEach`/`.filter`/`.reduce`/...) — the callback only, not the whole
    /// call expression, so a receiver like `(await fetch(u)).items.map(...)` does not put the one-shot
    /// `fetch` "inside" the loop. Consumed by `MethodScan::trigger_in_loop`. Empty when the parser has no
    /// support / falls back lexically — structural rules silently skip such files (graceful degrade,
    /// same policy as `symbols`).
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
}

impl SourceFile {
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
