//! Unit tests for [`super::scan_depths`], split out of `scan.rs` on 2026-09-09 for the line cap.
//!
//! They live beside the scanner rather than with the gate's tests because their subject is the
//! MEASUREMENT, not the policy: what counts as one token, where a statement ends, and which way a
//! confused scan is allowed to be wrong (always toward letting the parser try).

use super::*;

/// The C-family boundary, which is what every test below is about unless it says otherwise.
/// A test for the Python boundary calls `scan_depths` directly with the other arm.
fn scan(text: &str) -> Depths {
    scan_depths(text, StatementEnd::Delimiters)
}

/// 🔴 THE SHAPE THE TEST BELOW CANNOT REACH, and the one that crashed the binary.
///
/// Its sibling asserts that `const t = "` + 1,000 `(` counts 0 brackets, and that is safe at ANY
/// size — because the real lexer ALSO sees a string there, so the parser never recurses either. The
/// hole was never "unterminated string": it was "this scanner opens a string where the parser does
/// not", and an apostrophe in prose is the commonest instance of it.
///
/// 📏 On the shipped 0.38.0 this was not a miscount, it was exit 134 with zero bytes of stdout: a
/// README reading `It doesn't matter.` above a deep `<script>` block took the whole run down and
/// every finding in the tree with it, one byte away from a clean report. The gate's own scanner was
/// blind from that apostrophe to EOF, so depth read 0 and the parser it exists to stop went ahead.
#[test]
fn an_apostrophe_in_prose_does_not_blind_the_scan_to_end_of_file() {
    // Line 1 is prose with an apostrophe; line 2 is the depth the gate must still see.
    let s = format!("It doesn't matter.\n{}", "(".repeat(300));
    assert_eq!(
        scan(&s).brackets,
        300,
        "a `'` must close at the newline — if it swallows the file the gate goes silent and the \
         parser runs on input it exists to refuse"
    );

    // The Rust carrier, which is not prose at all: `&'static` is an ordinary lifetime.
    let rs = format!("fn g<'a>(_x: &'a u8) {{}}\n{}", "(".repeat(300));
    assert_eq!(
        scan(&rs).brackets,
        300,
        "a lifetime tick must not open a string body"
    );
}

/// A backtick MAY span lines — that is what a template literal is — so the line bound above is
/// deliberately not applied to it. Pinned so the fix cannot be "simplified" into breaking JS.
#[test]
fn a_backtick_still_spans_lines() {
    let s = format!("const t = `line one\nline two{}`;", "(".repeat(300));
    assert_eq!(
        scan(&s).brackets,
        0,
        "a template literal's body is still one string"
    );
}
/// 🔴 THIS TEST ASSERTED THE CRASH. It is kept, inverted, with the reason — deleting it would
/// lose the record of how a green test licensed a stack overflow for a month.
///
/// It used to read `assert_eq!(scan(&s).brackets, 0)` over `const t = "` + 1,000 `(`, commented
/// "the safe direction … rather than refusing a file nobody measured a reason to refuse". Both
/// halves were true of the shape it tested and false of the gate: the real lexer also sees a string
/// there, so that input never reaches a parser and the assertion could not fail whatever the engine
/// did. Meanwhile the SAME code path, entered from an apostrophe in prose or an unmatched backtick,
/// took the whole run to exit 134 with zero bytes of stdout.
///
/// The contract now: a quote body that reaches EOF without a terminator WAS NOT A STRING — a real
/// one always closes — so the guess is withdrawn and the depth is counted. Counting it can only
/// cost a refusal, and the gate is allowed to be wrong in that direction. Going blind to EOF costs
/// the crash it exists to prevent, and it is not.
#[test]
fn an_unterminated_string_at_eof_is_withdrawn_and_the_depth_is_counted() {
    let s = format!("const t = \"{}", "(".repeat(1000));
    assert_eq!(
        scan(&s).brackets,
        1000,
        "a quote that never closes is not a string; the brackets after it are real and the gate \
         must see them"
    );

    // The same, through the carrier that actually crashed the shipped binary: one unmatched
    // backtick in a line of prose, with depth after it.
    let md = format!("Pass the `--force flag.\n{}", "(".repeat(300));
    assert_eq!(
        scan(&md).brackets,
        300,
        "an unmatched backtick must not blind the scan to EOF"
    );
}

/// A CLOSED string still hides its contents — the withdrawal above is only for the EOF case, and
/// confusing the two would re-introduce the false refusals the 2026-09 grafana incident fixed.
#[test]
fn a_closed_string_still_hides_its_brackets() {
    let s = format!("const t = \"{}\";", "(".repeat(1000));
    assert_eq!(scan(&s).brackets, 0);
}

/// Rust raw strings span lines by design. Line-bounding `\"` without this made clap's own
/// 4,832-line `tests/builder/help.rs` — hand-written, 132 `r#\"…\"#` literals — refused outright
/// with zero findings, and the reply then told the reader it was "generated or bundled output".
#[test]
fn a_rust_raw_string_spans_lines_and_hides_its_brackets() {
    let s = format!("let s = r#\"\n{}\n\"#;", "(".repeat(300));
    assert_eq!(
        scan(&s).brackets,
        0,
        "r#\"…\"# is one string, newlines included"
    );

    // The hash run must MATCH: a lone `\"#` inside an `r##\"` body does not end it.
    let s2 = format!("let s = r##\"\n\"# {}\n\"##;", "(".repeat(300));
    assert_eq!(
        scan(&s2).brackets,
        0,
        "the terminator is the quote plus its own `#` run"
    );
}

#[test]
fn depth_is_counted_across_all_three_bracket_kinds() {
    assert_eq!(scan("({[]})").brackets, 3);
    assert_eq!(scan("()()()").brackets, 1);
}
#[test]
fn a_multi_byte_operator_counts_as_one_frame_not_two() {
    // The parser recurses once per OPERATOR. Counting bytes would halve the usable window and
    // make `&&`-heavy real code read as twice the depth it has — which is why these two agree.
    assert_eq!(scan("a && b || c").operator_run, 2);
    assert_eq!(scan("a & b | c").operator_run, 2);
}

#[test]
fn the_run_resets_at_statement_and_block_boundaries() {
    // Two short chains are not one long one. Without the reset an ordinary long file would clear
    // the cap on sheer accumulation and be refused for arithmetic no parser would struggle with.
    assert_eq!(scan("a + b + c; d + e + f;").operator_run, 2);
    assert_eq!(scan("fn f() { a + b } fn g() { c + d }").operator_run, 1);
}

#[test]
fn operators_inside_strings_and_comments_do_not_count() {
    // Same reason the bracket half skips them, and the same failure if it did not: a file whose
    // real chain is empty would be refused for one that exists only inside a string body.
    let s = format!(
        "const t = \"{}\"; // {}",
        "+".repeat(9_000),
        "+".repeat(9_000)
    );
    assert_eq!(scan(&s).operator_run, 0);
}

#[test]
fn an_identifier_run_is_one_token_and_commas_end_a_statement() {
    // 🔴 The test the byte-counting bug walked past. An arm spliced into the wrong match counted
    // every byte of a name as its own token, and the census that read it put real code three to
    // six times higher than it is -- 76,356 where the truth was 12,734 (review ledger V129).
    assert_eq!(scan("aaaaaaaaaa").statement_tokens, 1);
    assert_eq!(scan("alpha beta").statement_tokens, 2);
    assert_eq!(scan("a + a + a").statement_tokens, 5);

    // A comma FLATTENS: an argument list is one shallow node, not one frame per element, so it
    // ends the run exactly as a semicolon does. Without this a generated table of 100,000 literals reads
    // as the deepest statement in the corpus and the cap loses its window.
    assert_eq!(scan("f(a, b, c)").statement_tokens, 3);
    // The boundary byte itself counts as the first token of the run it opens: the reset arms fall
    // through to the same per-token increment every other significant byte takes. That is an
    // off-by-one per statement in the SAFE direction and is left alone -- a separator is a token.
    assert_eq!(scan("x = 1; y = 2;").statement_tokens, 4);

    // Comments and string bodies are not code, the same way the two other needles read them.
    assert_eq!(scan("// a b c d e f g").statement_tokens, 0);
    assert_eq!(scan("t = \"a b c d e\"").statement_tokens, 3);
}
