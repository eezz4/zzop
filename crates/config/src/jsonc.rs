//! JSONC stripping — the byte-for-byte port of the removed JS CLI's `jsonc.js` (2026-07-20), specifically its `stripJsonComments`:
//! two string-aware passes (comments, then trailing commas) that preserve newline COUNT so
//! `serde_json` error positions stay meaningful. Quirks that MUST survive the port (see the JS
//! source): `//`/`/*` inside double-quoted strings are copied through untouched; block comments
//! preserve embedded newlines (not full run-length space padding); trailing commas are BLANKED to a
//! space (not removed) when followed, across whitespace, by `}` or `]` — a purely lexical rule, kept
//! safe inside strings by the same string-awareness.

/// Strips `//` and `/* */` comments plus trailing commas from JSONC, returning valid JSON input for
/// `serde_json`. Mirrors the JS implementation exactly — do not "improve" it (no JSON5 extras).
pub fn strip_json_comments(input: &str) -> String {
    strip_trailing_commas(&strip_comments(input))
}

/// Pass 1 — port of `stripJsonComments`'s main scan loop: removes `//` line comments (the newline
/// terminator itself is left for the next iteration to copy through untouched) and `/* */` block
/// comments (embedded newlines are individually re-pushed so line numbers in later `serde_json`
/// errors stay meaningful), while copying double-quoted string contents through byte-for-byte —
/// including any `//`/`/*` sequences inside them, and escape-aware so `\"` never ends a string early.
fn strip_comments(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut out = String::with_capacity(input.len());
    let mut in_string = false;
    let mut escaped = false;
    let mut i = 0usize;

    while i < len {
        let ch = chars[i];
        let next = chars.get(i + 1).copied();

        if in_string {
            out.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }

        if ch == '"' {
            in_string = true;
            out.push(ch);
            i += 1;
            continue;
        }

        if ch == '/' && next == Some('/') {
            // Line comment: skip to end of line. Leave `i` sitting exactly on the newline/CR
            // terminator (or at `len` on EOF) so the next loop iteration copies the terminator
            // through the normal path — this is what preserves the newline itself.
            i += 2;
            while i < len && chars[i] != '\n' && chars[i] != '\r' {
                i += 1;
            }
            continue;
        }

        if ch == '/' && next == Some('*') {
            // Block comment: skip to the closing `*/`, re-pushing any newline crossed so embedded
            // line breaks still count toward downstream line numbers, then skip past the `*/` itself.
            i += 2;
            while i < len && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                if chars[i] == '\n' {
                    out.push('\n');
                }
                i += 1;
            }
            i += 2; // consume the closing "*/" (safe even past EOF: the outer `while i < len` exits)
            continue;
        }

        out.push(ch);
        i += 1;
    }

    out
}

/// Pass 2 — port of `stripTrailingCommas`: blanks a `,` to a single space (never removed, so byte
/// offsets stay stable) when, skipping whitespace, the next non-whitespace character is `}` or `]`.
/// Re-scans string-aware from scratch (independent of pass 1's state) since pass 1's output still
/// contains untouched string literals that may themselves contain commas.
fn strip_trailing_commas(input: &str) -> String {
    let mut chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut in_string = false;
    let mut escaped = false;
    let mut i = 0usize;

    while i < len {
        let ch = chars[i];

        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }

        if ch == '"' {
            in_string = true;
            i += 1;
            continue;
        }

        if ch == ',' {
            let mut j = i + 1;
            while j < len && chars[j].is_whitespace() {
                j += 1;
            }
            if j < len && (chars[j] == '}' || chars[j] == ']') {
                chars[i] = ' ';
            }
        }

        i += 1;
    }

    chars.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_comment_is_stripped_and_newline_preserved() {
        let input = "{\n  \"a\": 1, // trailing note\n  \"b\": 2\n}";
        let out = strip_json_comments(input);
        assert_eq!(out.lines().count(), input.lines().count());
        assert!(!out.contains("trailing note"));
        let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
        assert_eq!(parsed["a"], 1);
        assert_eq!(parsed["b"], 2);
    }

    #[test]
    fn block_comment_is_stripped_and_embedded_newlines_preserved() {
        let input = "{\n  \"a\": /* one\ntwo\nthree */ 1\n}";
        let out = strip_json_comments(input);
        // The block comment spans 2 embedded newlines; both must survive as blank-ish content so
        // downstream line numbers are unaffected.
        assert_eq!(out.matches('\n').count(), input.matches('\n').count());
        let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
        assert_eq!(parsed["a"], 1);
    }

    #[test]
    fn double_slash_inside_a_string_is_left_untouched() {
        let input = r#"{"url": "https://example.com"}"#;
        let out = strip_json_comments(input);
        assert_eq!(out, input);
        let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
        assert_eq!(parsed["url"], "https://example.com");
    }

    #[test]
    fn block_comment_markers_inside_a_string_are_left_untouched() {
        let input = r#"{"note": "/* not a comment */"}"#;
        let out = strip_json_comments(input);
        assert_eq!(out, input);
        let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
        assert_eq!(parsed["note"], "/* not a comment */");
    }

    #[test]
    fn escaped_quote_does_not_terminate_a_string_early() {
        // Without escape-awareness, the `\"` would be read as the string's closing quote and the
        // `//` right after it would then be treated (wrongly) as a real comment.
        let input = r#"{"a": "he said \"//not a comment\""}"#;
        let out = strip_json_comments(input);
        assert_eq!(out, input);
        let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
        assert_eq!(parsed["a"], "he said \"//not a comment\"");
    }

    #[test]
    fn trailing_comma_before_closing_brace_is_blanked_to_a_space() {
        let input = r#"{"a": 1,}"#;
        let out = strip_json_comments(input);
        // Blanked, not removed: same length, comma replaced by a space.
        assert_eq!(out.len(), input.len());
        assert_eq!(out, r#"{"a": 1 }"#);
        let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
        assert_eq!(parsed["a"], 1);
    }

    #[test]
    fn trailing_comma_before_closing_bracket_is_blanked_to_a_space() {
        let input = r#"[1, 2,]"#;
        let out = strip_json_comments(input);
        assert_eq!(out.len(), input.len());
        assert_eq!(out, r#"[1, 2 ]"#);
        let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
        assert_eq!(parsed.as_array().unwrap().len(), 2);
    }

    #[test]
    fn trailing_comma_across_whitespace_and_newlines_is_still_blanked() {
        let input = "{\"a\": 1,\n\n  }";
        let out = strip_json_comments(input);
        assert_eq!(out.len(), input.len());
        let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
        assert_eq!(parsed["a"], 1);
    }

    #[test]
    fn a_comma_inside_a_string_is_never_blanked() {
        let input = r#"{"a": "x, }"}"#;
        let out = strip_json_comments(input);
        assert_eq!(out, input);
    }

    #[test]
    fn a_non_trailing_comma_is_left_alone() {
        let input = r#"{"a": 1, "b": 2}"#;
        let out = strip_json_comments(input);
        assert_eq!(out, input);
    }

    #[test]
    fn representative_jsonc_blob_round_trips_through_serde_json() {
        let input = r#"{
  // top-level comment
  "roots": ["."], // trailing
  /* a block
     comment */
  "rules": {
    "circular": "warn", // inline
  },
  "exclude": ["legacy/",],
}"#;
        let out = strip_json_comments(input);
        let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
        assert_eq!(parsed["roots"][0], ".");
        assert_eq!(parsed["rules"]["circular"], "warn");
        assert_eq!(parsed["exclude"][0], "legacy/");
    }
}
