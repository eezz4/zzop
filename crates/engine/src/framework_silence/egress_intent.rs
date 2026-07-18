//! Internal-intent classifier shared by S5 (`builtin_fetch`) and S7 (`fetch_wrapper`).
//!
//! The census tripwires exist to flag a tree/app whose HTTP egress goes JOIN-dark. But an app that
//! only ever calls ABSOLUTE external services (a CDN, an RSS source, a third-party API) is not dark —
//! it has no internal contract to join, and telling its author to "inject a Mode B adapter to restore
//! cross-layer visibility" is wrong advice. This module is the discriminator that keeps such apps out.
//!
//! ## Why "has a non-absolute literal", not "has a `/`-headed literal"
//! The TS extractor already keys almost every STATIC relative-path literal (`fetch("/x")`,
//! `fetch("users/login")`, `fetch(`${B}/me`)`), so an app that hits the call-site mass floor via such
//! literals already has keyed consumes and is correctly NOT dark. The census's only real targets are
//! the shapes the extractor leaves UNRESOLVED: computed/dynamic internal egress like
//! `fetch(`${base}${path}`)` / `fetch(`${base}users`)` (a literal template, but no statically-known
//! path) and hand-rolled wrappers. Those carry a string/template LITERAL but no absolute-URL scheme.
//! So internal-intent is defined as: **a string or template literal is present in the URL argument,
//! AND no absolute (`http(s)://` or protocol-relative `//`) URL appears in it.** This counts the
//! dynamic-internal dark shapes while excluding `fetch(CONST)` (a bare identifier — the external corpus
//! idiom `fetch(CLASS_NAMES_URL)` / `fetch(URL)`) and `fetch("https://…")`.
//!
//! Deliberately lexical (regex + a bracket-depth arg slicer), independent of the extractor's own URL
//! resolution — same "a self-report must not share extraction's judgment" stance as every S1-S7
//! tripwire. Over-disclosure is the safe failure mode (see each tripwire's own doc).

use std::sync::OnceLock;

use regex::Regex;

/// Which portion of a call's argument list to classify. `First` = only the first argument (S5's
/// `fetch(URL, {opts})` — an options object's own string literals, e.g. `{ method: "POST" }`, must not
/// mark a bare-const external `fetch(CONST, …)` as internal). `All` = the whole argument list (S7's
/// wrapper call `request("GET", "/api/…")` carries the path in a later positional arg).
#[derive(Clone, Copy)]
pub(super) enum ArgSpan {
    First,
    All,
}

/// Upper bound on how many bytes past the call's `(` the classifier scans — a cost/pathological-input
/// guard (a non-policy cost cap, census-tracked alongside the other display/cost caps `MAX_SAMPLES`/
/// `MAX_EXAMPLES`; it carries no firing threshold or vocabulary). If the cap trips mid-argument the
/// region collected so far is classified as-is (over-disclosure-safe).
const ARG_SCAN_CAP: usize = 1024;

fn absolute_url_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // `http://` / `https://` anywhere, OR a string/template literal whose body opens with `//`
    // (protocol-relative). Case-insensitive on the scheme.
    RE.get_or_init(|| Regex::new(r#"(?i)https?://|["'`]//"#).unwrap())
}

fn string_literal_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // Presence of any string/template-literal delimiter — a URL argument that is a bare identifier
    // (`fetch(CONST)`) or a call (`fetch(getUrl())`) has none.
    RE.get_or_init(|| Regex::new(r#"["'`]"#).unwrap())
}

/// True when `region` (a sliced call-argument region) shows internal-intent egress: a string/template
/// literal is present and no absolute URL scheme appears. See the module doc for the rationale.
pub(super) fn region_is_internal_intent(region: &str) -> bool {
    string_literal_re().is_match(region) && !absolute_url_re().is_match(region)
}

/// Slices the argument region of a call whose `(` ends at byte `open_paren_end` in `text`. Tracks
/// bracket depth so a nested call/array/object does not end the region early, and treats each string /
/// template literal as opaque (so a `)` or `,` inside a literal — or inside a `${…}` interpolation of a
/// template — never terminates the slice). Stops at the call's matching `)` (depth back to 0), or, for
/// [`ArgSpan::First`], at the first top-level `,`. Bounded by [`ARG_SCAN_CAP`]. Returns the raw region
/// text for [`region_is_internal_intent`] to classify.
pub(super) fn arg_region(text: &str, open_paren_end: usize, span: ArgSpan) -> &str {
    let bytes = text.as_bytes();
    let start = open_paren_end;
    let hard_end = bytes.len().min(start + ARG_SCAN_CAP);
    let mut depth: i32 = 0; // 0 = inside the call's own parens (relative to the consumed `(`)
    let mut i = start;
    while i < hard_end {
        let c = bytes[i];
        match c {
            b'"' | b'\'' | b'`' => {
                // Skip an opaque string/template literal, escape-aware. A backtick template is treated
                // as one opaque span (its `${…}` interpolations are not structurally re-entered — any
                // brackets/commas inside stay inside the argument, which is what we want for slicing).
                i = skip_literal(bytes, i, hard_end);
                continue;
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                if depth == 0 {
                    break; // the call's closing `)`
                }
                depth -= 1;
            }
            b',' if depth == 0 => {
                if let ArgSpan::First = span {
                    break;
                }
            }
            _ => {}
        }
        i += 1;
    }
    // `start` is a char boundary (a regex `.end()` just past the ASCII `(`), and every structural
    // `break` point is at an ASCII byte. But `i` can be `hard_end` — the `ARG_SCAN_CAP` bound or an
    // unterminated-literal end — which may land inside a multi-byte UTF-8 char (CJK/emoji in a long
    // argument region), so floor it to a char boundary before slicing or `&text[..]` would panic.
    while i > start && !text.is_char_boundary(i) {
        i -= 1;
    }
    &text[start..i]
}

/// Advances past the string/template literal whose opening quote is at `open` (byte value `bytes[open]`),
/// returning the index just after the closing quote (or `hard_end` if unterminated within the cap).
/// Escape-aware (`\"` / `` \` `` do not close). Backtick spans are opaque (interpolations not re-entered).
fn skip_literal(bytes: &[u8], open: usize, hard_end: usize) -> usize {
    let quote = bytes[open];
    let mut i = open + 1;
    while i < hard_end {
        match bytes[i] {
            b'\\' => i += 2, // skip the escaped char
            c if c == quote => return i + 1,
            _ => i += 1,
        }
    }
    hard_end
}

#[cfg(test)]
mod tests {
    use super::{arg_region, region_is_internal_intent, ArgSpan};

    #[test]
    fn relative_and_computed_literals_are_internal() {
        assert!(region_is_internal_intent(r#""/api/x""#));
        assert!(region_is_internal_intent("`${b}/api/x`"));
        assert!(region_is_internal_intent("`${base}${path}`")); // computed-internal dark shape
        assert!(region_is_internal_intent("`${base}users`"));
        assert!(region_is_internal_intent(r#"'a'"#));
    }

    #[test]
    fn bare_identifiers_and_absolute_urls_are_not_internal() {
        assert!(!region_is_internal_intent("CLASS_NAMES_URL")); // bare const -> no literal
        assert!(!region_is_internal_intent("getUrl()"));
        assert!(!region_is_internal_intent(r#""https://cdn.example.com/x""#));
        assert!(!region_is_internal_intent("`https://x/${p}`"));
        assert!(!region_is_internal_intent(r#""//cdn/x""#)); // protocol-relative
        assert!(!region_is_internal_intent(r#""HTTPS://X""#)); // case-insensitive scheme
    }

    #[test]
    fn first_span_excludes_a_later_options_object_literal() {
        // S5: `fetch(CONST, { method: "POST" })` — the URL arg is a bare const; the "POST" literal
        // lives in the options object and must NOT make this count as internal.
        let text = r#"fetch(CONST, { method: "POST" })"#;
        let open = text.find('(').unwrap() + 1;
        let region = arg_region(text, open, ArgSpan::First);
        assert!(!region_is_internal_intent(region), "region={region:?}");
    }

    #[test]
    fn first_span_keeps_a_literal_url_first_arg() {
        let text = r#"fetch("/api/x", { method: "GET" })"#;
        let open = text.find('(').unwrap() + 1;
        let region = arg_region(text, open, ArgSpan::First);
        assert!(region_is_internal_intent(region), "region={region:?}");
    }

    #[test]
    fn all_span_reaches_a_later_positional_path_arg() {
        // S7: `request("GET", "/api/group/x")` — the path is the second positional arg.
        let text = r#"request("GET", "/api/group/x")"#;
        let open = text.find('(').unwrap() + 1;
        let region = arg_region(text, open, ArgSpan::All);
        assert!(region_is_internal_intent(region), "region={region:?}");
    }

    #[test]
    fn an_unterminated_multibyte_region_past_the_cap_does_not_panic() {
        // Regression: a >ARG_SCAN_CAP argument region of 3-byte chars must floor the cap offset to a
        // char boundary — `&text[start..cap]` would otherwise panic inside a multi-byte char. `\u{AC00}`
        // is a 3-byte UTF-8 char; a leading pad walks the 1024 cap into the middle of one.
        let three_byte = "\u{AC00}".repeat(1500);
        for pad in 0..4 {
            let text = format!("fetch({}`{three_byte}", "x".repeat(pad));
            let open = text.find('(').unwrap() + 1;
            let region = arg_region(&text, open, ArgSpan::First); // must not panic
            assert!(region_is_internal_intent(region)); // backtick literal, no scheme -> internal
        }
    }

    #[test]
    fn nested_parens_and_commas_inside_a_template_do_not_end_the_region_early() {
        // `${f(a,b)}` holds a comma + parens inside the template; the whole `fetch(...)` is one URL arg.
        let text = "fetch(`${f(a,b)}/api/x`)";
        let open = text.find('(').unwrap() + 1;
        let region = arg_region(text, open, ArgSpan::First);
        assert_eq!(region, "`${f(a,b)}/api/x`");
        assert!(region_is_internal_intent(region));
    }
}
