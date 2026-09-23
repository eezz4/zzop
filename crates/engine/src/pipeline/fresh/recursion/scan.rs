//! The one pre-parse scan both caps are read off, split out of [`super`] on 2026-09-08.
//!
//! The parent owns the POLICY — the two cap values, the two censuses that bracket them, and the
//! gate that compares. This file owns the MEASUREMENT. That is the seam that matters here: the
//! numbers move when a corpus is re-scanned, and the scanner moves when a new parser-nesting shape
//! is discovered. Those are different events, and the two that forced this file over the line
//! arrived four hours apart (review ledger V99, then V115).

/// What one pre-parse scan measures: bracket nesting, and the longest binary-operator chain.
///
/// Both are proxies for the same thing — how deep a recursive-descent parser will recurse — and
/// neither sees the other's shape: `((((…` has no operators and `1 + 1 + …` has no brackets.
/// They are counted in one pass because they share the expensive part, which is knowing what is
/// code and what is a string, a comment or a regex body.
///
/// A lexer-shaped scan rather than a parse, because the whole point is to answer before a parser runs.
/// It is deliberately approximate in the safe direction: an unterminated string swallows the rest of the
/// file (depth 0, gate does not fire, parser runs — the status quo), and a regex/division ambiguity
/// resolved the wrong way can only LOWER the count. Nothing here can invent depth that is not there.
pub(crate) struct Depths {
    pub(crate) brackets: usize,
    pub(crate) operator_run: usize,
    /// Significant tokens in the longest statement — the SHAPE-INDEPENDENT needle.
    ///
    /// The two above name a shape each, and a needle that names shapes is only ever as complete as
    /// the list someone thought of. Measured on 2026-09-08, four more shapes walked straight
    /// through both of them into an exit-127 abort (review ledger V129): a conditional chain
    /// (`b ? 1 : b ? 1 : …`, no brackets and no counted operator), a unary chain (`!!!!…b`, at 70 KB
    /// the smallest killer found), a member chain (`a.b.b.b…`), and — worst, because the shape was
    /// supposedly already covered — `- - - b`, whose every `-` IS in the operator set but which the
    /// multi-byte-operator rule below collapses to a run of ONE.
    ///
    /// What all four share is not a shape: it is that the whole file is ONE statement. Real code
    /// ends statements. So this counts significant tokens since the last statement boundary and
    /// takes the maximum, which costs nothing extra (the same pass already knows what is code) and
    /// does not have to be taught the next shape.
    ///
    /// 🔴 WHERE a statement ends is not universal, and this said it was. [`StatementEnd`] is the
    /// correction (review ledger V140).
    pub(crate) statement_tokens: usize,
}

/// WHICH of the three measurements put a file past its bound.
///
/// 🔴 This exists because the self-report was a hand-written sentence beside a growing set. The
/// gate has had three needles since 2026-09-08 and the `degraded` warning named two of them, so a
/// file refused for statement length was told it had more than 256 nested brackets — and the reader
/// who goes and counts brackets finds none and concludes the tool is wrong (review ledger V139).
/// The same file's doc had already recorded that exact failure once, for the operator needle, and
/// the fix chosen then was to add the second clause by hand. That is why there is now a type.
///
/// `StatementTokens` carries its cap because the cap is per-language: naming "the" statement cap in
/// one sentence would be the same mistake one level up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum RecursionNeedle {
    /// Bracket nesting, the first needle and the only one until 2026-09-06.
    Brackets { cap: usize },
    /// An unbroken run of binary operators — bracket depth zero, and just as deep a recursion.
    OperatorRun { cap: usize },
    /// Significant tokens in one statement, the needle that does not have to be taught a shape.
    StatementTokens { cap: usize },
}

/// Where a statement ends, which is a property of the LANGUAGE and not of this scanner.
///
/// 🔴 This existed as an assertion in a comment before it existed as a type: *"Every dispatched
/// language ends a statement or opens a block with one of these three"* — `;`, `{`, `}`. That is
/// false for Python, and the consequence was not cosmetic (review ledger V140). With no boundary
/// ever reached, a `.py` file counted as ONE statement end to end, so `MAX_PYTHON_STATEMENT_TOKENS`
/// stopped being a bound on a statement and became a bound on the FILE: a 4,001-line module of
/// `CONST_i = i` lines — no brackets, no operators, nothing recursive anywhere in it — was refused.
///
/// ⚠ And the census that justified that cap was taken with the same needle, so
/// `LONGEST_REAL_PYTHON_STATEMENT_MEASURED` was never a statement length either. Both numbers had to
/// be re-derived, not just the comparison — the same shape as V129, one language later.
pub(crate) enum StatementEnd {
    /// C-family and Rust: `;` ends one, `{`/`}` open and close a block, `,` flattens a list.
    Delimiters,
    /// Python: all of the above, PLUS a line break — but only a REAL one. A newline inside brackets
    /// is an implicit continuation and a newline after a backslash is an explicit one, so neither
    /// ends the logical line. `depth` already tracks the first and `prev` the second, so this costs
    /// two comparisons and no extra state.
    DelimitersOrLineBreak,
}

pub(crate) fn scan_depths(text: &str, ends: StatementEnd) -> Depths {
    let b = text.as_bytes();
    let mut depth: usize = 0;
    let mut max: usize = 0;
    // The operator chain resets at every statement boundary, and a reset can only make the count
    // SMALLER — the same safe direction the bracket scan takes.
    let mut run: usize = 0;
    let mut max_run: usize = 0;
    // Tokens since the last statement boundary, and the largest such count seen. Resets exactly
    // where `run` resets, for the same reason: two short statements are not one long one.
    let mut toks: usize = 0;
    let mut max_toks: usize = 0;
    let mut prev: u8 = 0; // last significant byte, for the regex/division decision
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        match c {
            b'/' if i + 1 < b.len() && b[i + 1] == b'/' => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            b'/' if i + 1 < b.len() && b[i + 1] == b'*' => {
                i += 2;
                while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(b.len());
                continue;
            }
            // A `/` that cannot follow an operand starts a regex literal, not a division.
            b'/' if !matches!(prev, b')' | b']' | b'}')
                && !prev.is_ascii_alphanumeric()
                && prev != b'_'
                && prev != b'$' =>
            {
                let mut j = i + 1;
                let mut in_class = false;
                let mut closed = false;
                while j < b.len() {
                    match b[j] {
                        b'\\' => j += 1,
                        b'\n' => break,
                        b'[' => in_class = true,
                        b']' => in_class = false,
                        b'/' if !in_class => {
                            closed = true;
                            break;
                        }
                        _ => {}
                    }
                    j += 1;
                }
                if closed {
                    i = j + 1;
                    prev = b'/';
                    toks += 1;
                    max_toks = max_toks.max(toks);
                    continue;
                }
                i += 1;
                prev = b'/';
                toks += 1;
                max_toks = max_toks.max(toks);
                continue;
            }
            b'"' | b'\'' | b'`' => {
                let quote = c;
                i += 1;
                while i < b.len() {
                    if b[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    if b[i] == quote {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                prev = quote;
                toks += 1;
                max_toks = max_toks.max(toks);
                continue;
            }
            b'(' | b'[' => {
                depth += 1;
                if depth > max {
                    max = depth;
                }
            }
            b'{' => {
                depth += 1;
                if depth > max {
                    max = depth;
                }
                run = 0;
                toks = 0;
            }
            b')' | b']' => depth = depth.saturating_sub(1),
            b'}' => {
                depth = depth.saturating_sub(1);
                run = 0;
                toks = 0;
            }
            // A comma FLATTENS a tree where an operator nests it, so an argument list or an array
            // literal of 100,000 elements is one shallow node, not 100,000 frames. Without this reset the
            // deepest real statement measured jumps from 38,070 tokens to 76,356 and the needle loses its
            // window entirely.
            // A real line break ends a Python statement. `depth == 0` excludes an implicit
            // continuation (inside `(`/`[`/`{`) and `prev != b'\\'` an explicit one; without those two
            // this would reset inside a multi-line killer and undercount it, which is the one
            // direction a reset must never be wrong in.
            b'\n'
                if matches!(ends, StatementEnd::DelimitersOrLineBreak)
                    && depth == 0
                    && prev != b'\\' =>
            {
                run = 0;
                toks = 0;
            }
            // An identifier or number run is ONE token, not one per byte. Counting bytes would make
            // an ordinary file of long names read as a long statement and leave the cap no window.
            _ if c.is_ascii_alphanumeric() || c == b'_' || c == b'$' => {
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'$')
                {
                    i += 1;
                }
                // The run had at least one byte, so i-1 is in bounds. Set once rather than per byte:
                // only the run's LAST byte is what the regex/division decision above reads.
                prev = b[i - 1];
                toks += 1;
                max_toks = max_toks.max(toks);
                continue;
            }
            b';' | b',' => {
                run = 0;
                toks = 0;
            }
            // A multi-byte operator (`&&`, `==`, `<<`) counts once: the parser recurses per
            // OPERATOR, not per character, and counting characters would halve the usable window.
            b'+' | b'-' | b'*' | b'/' | b'%' | b'&' | b'|' | b'^' | b'<' | b'>'
                if !matches!(
                    prev,
                    b'+' | b'-' | b'*' | b'/' | b'%' | b'&' | b'|' | b'^' | b'<' | b'>'
                ) =>
            {
                run += 1;
                if run > max_run {
                    max_run = run;
                }
            }
            _ => {}
        }
        if !c.is_ascii_whitespace() {
            prev = c;
            // Reached only by punctuation and operators: the boundary bytes above already reset,
            // and identifier runs, strings and regex bodies `continue` past here having counted
            // themselves. Comments and whitespace are not significant and count nothing.
            toks += 1;
            max_toks = max_toks.max(toks);
        }
        i += 1;
    }
    Depths {
        brackets: max,
        operator_run: max_run,
        statement_tokens: max_toks,
    }
}

#[cfg(test)]
mod tests;
