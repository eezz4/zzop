//! Line-local string-literal masking for the `strip_string_literals` matcher option (see
//! `def::matcher::LineScan`/`MethodScan`'s field docs). Replaces the INTERIOR of every closed string
//! literal on a single line with spaces, leaving the quote delimiters and all code outside strings
//! intact, so a DSL pattern never matches a token that only appears inside a string literal.
//!
//! Deliberately line-local — the DSL matchers test one physical line at a time, so this scanner sees one
//! line at a time too. A string opened but not closed on the line (a multi-line template/triple-quote
//! opener) has no close for the scanner to find, so everything from that opener to end-of-line is left
//! UNMASKED: masking to EOL would risk blanking real code on a mis-detected opener, and the false-positive
//! this option exists to kill (a token inside a fully-inline string literal) is by definition on one line.
//!
//! Length- and column-preserving: each interior byte becomes a single space (0x20), so the masked string
//! has the same byte length as the input and byte offsets line up — the matchers only need a boolean
//! `is_match`, but preserving length keeps `^`/`$`/`\b` boundary semantics identical to the original line.
//!
//! ## Known limits (all bias to UNDER-report, never a false positive — safe for an opt-in filter)
//! This is a byte scanner, not a JS lexer, so a `'`/`"`/`` ` `` that is NOT a string delimiter can open a
//! phantom literal that swallows the rest of the line:
//! - **Regex literals**: `const re = /'/; realCall()` — the lone `'` inside the `/'/` regex opens a
//!   phantom string and masks the real `realCall()`. Needs `/`-division-vs-regex lexing to fix; out of
//!   scope. Rare (a regex literal with an unbalanced quote AND a real target token on the same line).
//! - **Template interpolations**: `` `${realCall()}` `` — the interpolation holds executable code but is
//!   masked as opaque string interior, so a target token inside `${…}` is not seen. The option masks
//!   string *content*; interpolated code counts as content here.
//!
//! Both directions LOSE a match (the masked line is a subset of the raw line's matches), so a rule that
//! opts in can only ever fire LESS, never more — which is why the two matcher prefilters (the line-scan
//! `RegexSet` candidacy and the method-scan whole-file pre-skip), computed on RAW text, stay correct: a
//! file the masked scan would hit is always a file the raw prefilter already admitted. The one shape this
//! subset argument does NOT cover is a pattern that matches the SPACES masking introduces (e.g. `"\s+"`) —
//! self-contradictory with the option's purpose (a rule that both strips strings and matches whitespace),
//! so not a real combination.

/// Mask the interior of every closed `'…'` / `"…"` / `` `…` `` literal on `line` to spaces. Backslash
/// escapes inside a literal (`\"`, `\'`, `` \` ``) are honored so an escaped quote does not close the
/// literal early. An unterminated literal is left intact (see module doc).
pub(super) fn mask_string_literals(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut out = bytes.to_vec();
    let mut i = 0;
    while i < bytes.len() {
        let quote = bytes[i];
        if quote == b'\'' || quote == b'"' || quote == b'`' {
            if let Some(close) = find_close(bytes, i, quote) {
                // Blank the interior (exclusive of both delimiters).
                for b in out.iter_mut().take(close).skip(i + 1) {
                    *b = b' ';
                }
                i = close + 1;
                continue;
            }
            // Unterminated on this line — leave the rest untouched and stop scanning.
            break;
        }
        i += 1;
    }
    // The only mutation is ASCII-quote-delimited interior bytes -> ASCII space, so the result is still
    // valid UTF-8 (a multi-byte char's bytes each become a space). The fallback never triggers in practice.
    String::from_utf8(out).unwrap_or_else(|_| line.to_string())
}

/// Byte index of the closing `quote` for a literal opened at `open`, honoring `\`-escapes, or `None` if
/// the literal is not closed before end-of-line.
fn find_close(bytes: &[u8], open: usize, quote: u8) -> Option<usize> {
    let mut j = open + 1;
    while j < bytes.len() {
        match bytes[j] {
            b'\\' => {
                // Skip the escaped byte (whatever it is). If `\` is the last byte, the literal is
                // unterminated — `j += 2` walks past the end and the loop exits with `None`.
                j += 2;
            }
            b if b == quote => return Some(j),
            _ => j += 1,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::mask_string_literals;

    #[test]
    fn interior_of_a_double_quoted_literal_is_blanked_delimiters_kept() {
        assert_eq!(
            mask_string_literals(r#"a("process.exit(2)")"#),
            r#"a("               ")"#
        );
    }

    #[test]
    fn single_quotes_and_backticks_are_masked_too() {
        // `DROP TABLE t` = 12 chars, `a.b.c(1)` = 8 — use repeat() rather than hand-counted spaces.
        assert_eq!(
            mask_string_literals("x = 'DROP TABLE t'"),
            format!("x = '{}'", " ".repeat(12))
        );
        assert_eq!(
            mask_string_literals("y = `a.b.c(1)`"),
            format!("y = `{}`", " ".repeat(8))
        );
    }

    #[test]
    fn code_outside_strings_is_untouched() {
        // The real point: a pattern testing the masked line still sees the CALL, not the string token.
        let masked = mask_string_literals(r#"exit("process.exit(2)")"#);
        assert!(masked.contains("exit("));
        assert!(!masked.contains("process.exit(2)"));
    }

    #[test]
    fn escaped_quote_does_not_close_the_literal_early() {
        // `"a\"b"` is ONE literal with interior `a\"b`; the inner `\"` must not be read as the close.
        assert_eq!(mask_string_literals(r#"f("a\"b")"#), r#"f("    ")"#);
    }

    #[test]
    fn unterminated_literal_leaves_the_rest_intact() {
        // A multi-line template opener: no close on this line, so nothing after the quote is masked.
        let line = "html = `<div>process.exit(2)";
        assert_eq!(mask_string_literals(line), line);
    }

    #[test]
    fn two_separate_literals_on_one_line_are_both_masked() {
        assert_eq!(
            mask_string_literals(r#"m("aaa", "bbb")"#),
            r#"m("   ", "   ")"#
        );
    }

    #[test]
    fn length_is_preserved_including_multibyte_interior() {
        // A multi-byte char inside the string becomes spaces byte-wise; length is unchanged.
        let line = "t(\"héllo\")";
        assert_eq!(mask_string_literals(line).len(), line.len());
        assert!(!mask_string_literals(line).contains('é'));
    }

    #[test]
    fn a_line_with_no_strings_is_returned_verbatim() {
        assert_eq!(mask_string_literals("process.exit(2);"), "process.exit(2);");
    }

    #[test]
    fn a_single_quote_inside_a_double_quoted_string_does_not_open_a_new_literal() {
        // `"it's here"` is ONE double-quoted literal; the inner `'` is masked as content, not an opener.
        // Interior `it's here` = 9 chars.
        assert_eq!(
            mask_string_literals(r#"x = "it's here""#),
            format!(r#"x = "{}""#, " ".repeat(9))
        );
    }

    // --- Documented known limitations (see module doc): a non-string-delimiter quote opens a phantom
    // literal. Both UNDER-report (mask real code) — pinned here so a future JS-lexer-aware fix is visible.

    #[test]
    fn known_limit_regex_literal_quote_masks_following_code() {
        // The `'` inside the `/'/` regex opens a phantom string that swallows the real call. Pins the
        // current (under-report) behavior — NOT the desired one.
        let masked = mask_string_literals("const re = /'/; process.exit('x')");
        assert!(
            !masked.contains("process.exit("),
            "known limit changed (regex-literal quote now handled?): {masked}"
        );
    }

    #[test]
    fn known_limit_template_interpolation_is_masked_as_opaque_interior() {
        // A `${…}` interpolation holds real code but is masked as string content. Pins current behavior.
        let masked = mask_string_literals("x = `${process.exit(2)}`");
        assert!(
            !masked.contains("process.exit("),
            "known limit changed (interpolation now preserved?): {masked}"
        );
    }
}
