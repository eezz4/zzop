// Conservative regex -> route-path conversion for `pathname.match(/re/)` dispatch
//
// Raw-Worker servers commonly route parameterized paths by regex instead of literal equality:
//   const m = pathname.match(/^\/api\/ledger\/([^/]+)\/verify$/);
//   if (m && method === "POST") return verifyCode(request, env, m[1]);
// This turns such a regex SOURCE (swc `Lit::Regex.exp` — the text between the `/.../` delimiters)
// into a normalized route path (`/api/ledger/{}/verify`) that `http_interface_key`'s own
// `{x}`/`:x` -> `{}` normalization then keys identically to the FE template-literal consume side.
//
// NEVER-GUESS: the converter returns `None` for anything it cannot convert without inventing a
// route — an unanchored regex (matches substrings), any flag (case-fold/multiline change the path
// semantics), alternation / optional segments (two different paths), or a segment that is neither a
// clean literal nor one of a fixed allowlist of single-path-segment param matchers. A `None` means
// the site emits no provide and stays in the honesty channel, never a wrong key.

/// Converts a JS regex literal's SOURCE (`exp`) + `flags` into a normalized route path, or `None`
/// when it is not a fully-anchored single-segment-param path matcher we can convert without guessing.
pub(super) fn regex_to_route_path(exp: &str, flags: &str) -> Option<String> {
    // Any flag changes matching semantics a static path key cannot reflect (`i` case-fold, `m`
    // re-points `^`/`$`, `s`/`g`/`u`/`y` …) — bail.
    if !flags.is_empty() {
        return None;
    }
    // Must be FULLY anchored: an unanchored regex matches a substring/prefix, so it names no single
    // route. Strip exactly one leading `^` and one trailing `$`.
    let body = exp.strip_prefix('^')?.strip_suffix('$')?;
    // A rooted path opens at `/`, spelled `\/` inside a `/`-delimited literal.
    let rest = body.strip_prefix("\\/")?;
    if rest.is_empty() {
        return Some("/".to_string()); // `^\/$` — the bare root
    }
    // Split on the escaped slash `\/` (a bare `/` only appears inside a char class like `[^/]`,
    // which is never the two-char `\/` sequence, so it does not split).
    let mut segments = Vec::new();
    for seg in rest.split("\\/") {
        segments.push(convert_segment(seg)?);
    }
    Some(format!("/{}", segments.join("/")))
}

/// One `\/`-delimited segment -> its literal text, `{}` for a recognized single-segment param, or
/// `None` (bail) for any unrecognized shape.
fn convert_segment(seg: &str) -> Option<String> {
    if seg.is_empty() {
        return Some(String::new()); // a leading/trailing-slash artifact; path normalization drops it
    }
    if let Some(lit) = literal_segment(seg) {
        return Some(lit);
    }
    if is_param_segment(seg) {
        return Some("{}".to_string());
    }
    None
}

/// A clean literal path segment: URL-safe chars only, with `\`-escaped punctuation (`\.`, `\-`, …)
/// unescaped to its literal. Returns `None` if any REGEX metacharacter appears (an unescaped `.`,
/// `+`, `[`, `(`, `\d`, …) — that segment is not a plain literal and must be tried as a param or
/// bailed on. Note an unescaped `.` is a wildcard, NOT a literal dot, so it is rejected here.
fn literal_segment(seg: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = seg.chars();
    while let Some(c) = chars.next() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '~' => out.push(c),
            '\\' => match chars.next()? {
                // an escaped literal we can carry through as its plain char
                e @ ('.' | '-' | '/' | '_' | '~') => out.push(e),
                _ => return None, // `\d`, `\w`, `\s`, ... character-class escapes we don't allow -> not a literal
            },
            _ => return None, // any other regex metachar -> not a clean literal
        }
    }
    Some(out)
}

/// True when `seg` is a recognized matcher for exactly ONE path segment — a bare atom or one wrapped
/// in a single capturing `(...)` or non-capturing `(?:...)` group. A fixed allowlist (never a
/// heuristic): the common `[^/]+` / `\d+` / `\w+` / char-class idioms. Anything else (alternation,
/// optional `?`, `.+` multi-segment catch-all, nested groups, named groups, lookarounds) is NOT a
/// param here and the whole regex bails — under-extraction is safe, a wrong key is not.
fn is_param_segment(seg: &str) -> bool {
    let inner = strip_one_group(seg).unwrap_or(seg);
    matches!(
        inner,
        "[^/]+"
            | "[^/]*"
            | "[^/]+?"
            | "[^/]*?"
            | "\\d+"
            | "\\d*"
            | "\\w+"
            | "\\w*"
            | "[\\w-]+"
            | "[\\w-]*"
            | "[0-9]+"
            | "[a-z0-9]+"
            | "[A-Za-z0-9]+"
            | "[a-zA-Z0-9]+"
            | "[a-z0-9-]+"
            | "[a-zA-Z0-9-]+"
            | "[a-zA-Z0-9_-]+"
            | "[A-Za-z0-9_-]+"
            | "[a-f0-9]+"
            | "[a-f0-9-]+"
            | "[A-Fa-f0-9-]+"
    )
}

/// Strips exactly ONE enclosing group from `seg`: `(...)` -> `...`, `(?:...)` -> `...`. Returns
/// `None` when `seg` is not a single balanced enclosing group (`[^/]+`, or `(\d+)-(\d+)` where the
/// first `(` closes before the end), so the caller falls back to matching `seg` whole (and bails).
fn strip_one_group(seg: &str) -> Option<&str> {
    let inner = seg.strip_prefix('(')?.strip_suffix(')')?;
    // Verify the opening `(` we stripped matches the closing `)` we stripped — i.e. no earlier `)`
    // closes it (which would mean two sibling groups, not one enclosing group).
    let mut depth = 1i32;
    for c in inner.chars() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return None; // closed before the end -> not a single enclosing group
                }
            }
            _ => {}
        }
    }
    Some(inner.strip_prefix("?:").unwrap_or(inner))
}

#[cfg(test)]
mod tests {
    use super::regex_to_route_path;

    fn conv(exp: &str) -> Option<String> {
        regex_to_route_path(exp, "")
    }

    #[test]
    fn single_param_and_multi_param() {
        // The mono-hub raw-Worker corpus shapes.
        assert_eq!(
            conv(r"^\/api\/ledger\/([^/]+)\/verify$").as_deref(),
            Some("/api/ledger/{}/verify")
        );
        assert_eq!(
            conv(r"^\/api\/ledger\/([^/]+)$").as_deref(),
            Some("/api/ledger/{}")
        );
        assert_eq!(
            conv(r"^\/api\/ledger\/([^/]+)\/revision\/(\d+)$").as_deref(),
            Some("/api/ledger/{}/revision/{}")
        );
    }

    #[test]
    fn no_params_and_root() {
        assert_eq!(conv(r"^\/api\/rates$").as_deref(), Some("/api/rates"));
        assert_eq!(conv(r"^\/$").as_deref(), Some("/"));
    }

    #[test]
    fn param_variants_and_noncapturing() {
        assert_eq!(conv(r"^\/u\/(\w+)$").as_deref(), Some("/u/{}"));
        assert_eq!(conv(r"^\/u\/([\w-]+)$").as_deref(), Some("/u/{}"));
        assert_eq!(conv(r"^\/u\/(?:[^/]+)$").as_deref(), Some("/u/{}")); // non-capturing group
        assert_eq!(conv(r"^\/u\/[^/]+$").as_deref(), Some("/u/{}")); // uncaptured bare atom
        assert_eq!(conv(r"^\/f\/(\.)$"), None); // `\.`-in-group is not an allowlisted param atom
    }

    #[test]
    fn escaped_literal_dot_in_segment() {
        assert_eq!(
            conv(r"^\/files\/data\.json$").as_deref(),
            Some("/files/data.json")
        );
    }

    #[test]
    fn bails_on_unanchored() {
        assert_eq!(conv(r"\/api\/x"), None); // no ^ $
        assert_eq!(conv(r"^\/api\/x"), None); // no $
        assert_eq!(conv(r"\/api\/x$"), None); // no ^
    }

    #[test]
    fn bails_on_flag() {
        assert_eq!(regex_to_route_path(r"^\/api\/x$", "i"), None);
    }

    #[test]
    fn bails_on_ambiguous_shapes() {
        assert_eq!(conv(r"^\/(a|b)$"), None); // alternation
        assert_eq!(conv(r"^\/api\/(\d+)?$"), None); // optional segment
        assert_eq!(conv(r"^\/files\/(.+)$"), None); // multi-segment catch-all
        assert_eq!(conv(r"^\/api\/v.1$"), None); // unescaped `.` wildcard, not a literal dot
        assert_eq!(conv(r"^\/api\/(\d+)-(\d+)$"), None); // two groups in one segment
        assert_eq!(conv(r"^\/api\/(?<id>[^/]+)$"), None); // named group
    }
}
