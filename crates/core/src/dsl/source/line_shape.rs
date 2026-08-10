//! The minified-LINE-SHAPE check: does this file look like bundler output, judged on line
//! lengths alone? Deliberately the whole module — provenance (`@generated` banners) is a different
//! question with a different detector (`zzop_engine::generated_banner`), and the two living in one
//! place under one two-axis name is the defect this module was carved out of.

/// Whether a file's LINE SHAPE is the shape minification leaves behind. True iff EITHER prong holds:
///
/// 1. **Absolute prong**: any single line is 5000+ bytes long — never hand-written, regardless of how
///    small a fraction of the file it is.
/// 2. **Ratio prong**: any line is 500+ bytes long AND 500+ byte lines account for at least 50% of the
///    file's total bytes — long lines DOMINATE, the signature of bundler output.
///
/// The ratio prong exists because a plain "any 500+ char line" rule causes collateral damage: an ordinary
/// hand-written file can happen to have one long comment or string literal among hundreds of normal
/// lines, and flagging on that alone would silently drop its entire DSL coverage.
///
/// Computed once per file. When true, the engine skips ALL DSL rule-pack evaluation for the file; native
/// structural extraction (symbols/imports/IO) is unaffected. Every DSL matcher is line-granular, so a
/// file whose lines are not lines cannot be judged by one, whatever produced it.
///
/// **Decides nothing about PROVENANCE, and deliberately does not try**: it reads line lengths only, never
/// the file's head, so a machine-generated file with ordinary short lines is false here — the common
/// case, since most generators pretty-print. That axis belongs to `zzop_engine::generated_banner`
/// (banner markers, `vocabulary.generatedFileMarkers`); the two are independent and neither subsumes the
/// other. Until 2026-08-09 this was named `is_minified_or_generated`, claiming an axis no line of its
/// body implements, so "how does zzop decide a file is generated?" met two functions and one detector.
pub fn has_minified_line_shape(text: &str) -> bool {
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
mod minified_line_shape_tests {
    use super::has_minified_line_shape;

    #[test]
    fn normal_short_line_file_is_not_minified() {
        let text = "const x = 1;\nfunction f() {\n  return x;\n}\n";
        assert!(!has_minified_line_shape(text));
    }

    #[test]
    fn a_single_long_line_dominating_a_tiny_file_is_minified() {
        let text = format!(
            "const short = 1;\nconst bundled = \"{}\";\n",
            "x".repeat(600)
        );
        assert!(has_minified_line_shape(&text));
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
            !has_minified_line_shape(&text),
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
        assert!(has_minified_line_shape(&text));
    }

    #[test]
    fn a_499_char_line_is_the_boundary_and_is_not_minified() {
        let line = "x".repeat(499);
        assert_eq!(line.len(), 499);
        let text = format!("{line}\n");
        assert!(!has_minified_line_shape(&text));
    }

    #[test]
    fn a_500_char_line_that_dominates_is_the_boundary_and_is_minified() {
        let line = "x".repeat(500);
        assert_eq!(line.len(), 500);
        let text = format!("{line}\n");
        assert!(has_minified_line_shape(&text));
    }

    #[test]
    fn a_trailing_carriage_return_near_the_boundary_still_counts_correctly() {
        // `split('\n')` leaves a trailing `\r` on each line, so a line whose visible content is exactly
        // 499 chars becomes 500 bytes once its `\r` is counted, tripping the threshold a character
        // earlier than LF source would.
        let visible = "x".repeat(499);
        let text = format!("{visible}\r\n");
        assert!(
            has_minified_line_shape(&text),
            "a 499-char line plus a trailing \\r from CRLF must reach the 500-byte threshold"
        );
    }

    #[test]
    fn an_empty_file_is_not_minified() {
        assert!(!has_minified_line_shape(""));
    }

    #[test]
    fn a_banner_marked_generated_file_with_short_lines_is_not_this_function_s_business() {
        // The boundary the old name `is_minified_or_generated` hid: an unambiguously GENERATED file
        // (two `vocabulary.generatedFileMarkers` markers) that this function calls false, because its
        // only subject is line length. Asserted the other way round it FAILED under the old name
        // (2026-08-09, pre-rename) — the name was a defect while every caller was correct. Provenance
        // is `zzop_engine::generated_banner`'s question; this pin goes red if the two are ever merged.
        let text = "// @generated by openapi-generator. DO NOT EDIT.\nexport const x = 1;\n";
        assert!(
            !has_minified_line_shape(text),
            "this function reads line lengths only — a bannered generated file with short lines is \
             not its subject; see zzop_engine::generated_banner"
        );
    }
}
