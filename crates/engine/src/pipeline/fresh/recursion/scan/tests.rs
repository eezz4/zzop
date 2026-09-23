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

#[test]
fn an_unterminated_string_lowers_the_count_it_never_raises_it() {
    // The safe direction: a scan confused by odd input falls back to "let the parser try", which is
    // exactly today's behaviour, rather than refusing a file nobody measured a reason to refuse.
    let s = format!("const t = \"{}", "(".repeat(1000));
    assert_eq!(scan(&s).brackets, 0);
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
