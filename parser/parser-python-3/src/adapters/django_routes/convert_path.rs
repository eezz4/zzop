//! Regex/route-string -> literal-`{}`-param key reduction for `adapters::django_routes`. Both reducers
//! are CONSERVATIVE: a shape they cannot cleanly reduce to a literal path with `{}` param holes yields
//! `None` (the caller skips that entry), never a guessed key — mis-keying a route is worse than an
//! honest under-report (the never-guess doctrine).

/// A placeholder no source path or regex contains, standing in for a reduced param position while the
/// residual is veto-checked; substituted to `{}` last (so a literal `{`/`}` in the source — a regex
/// quantifier like `{2,3}` — is vetoed rather than mistaken for a param hole).
const PARAM: char = '\u{0}';

/// A Django `url()`/`re_path()` regex -> a normalized literal key, or `None` when it does not cleanly
/// reduce (see module doc). Strips a leading `^`, a trailing `$`, a trailing `/?` (optional slash), and
/// a trailing `/`; reduces each `(?P<name>...)` named group to a `{}` param; then vetoes any residual
/// regex metacharacter (an unnamed group, alternation, a bare character class or escape, a quantifier).
pub(super) fn regex_to_key(raw: &str) -> Option<String> {
    let mut s = raw;
    s = s.strip_prefix('^').unwrap_or(s);
    s = s.strip_suffix('$').unwrap_or(s);
    s = s.strip_suffix("/?").unwrap_or(s);
    s = s.strip_suffix('/').unwrap_or(s);

    let reduced = reduce_named_groups(s)?;
    if !reduced
        .chars()
        .all(|c| c == PARAM || is_regex_literal_char(c))
    {
        return None; // a residual regex metacharacter — not a clean literal path
    }
    Some(ensure_leading_slash(&reduced.replace(PARAM, "{}")))
}

/// A modern Django `path()` route -> a normalized literal key, or `None` when a residual character is
/// not a clean path literal. Strips one trailing `/`; reduces each `<converter:name>` / `<name>`
/// angle-bracket converter to a `{}` param.
pub(super) fn path_to_key(raw: &str) -> Option<String> {
    let s = raw.strip_suffix('/').unwrap_or(raw);
    let reduced = reduce_angle_params(s)?;
    if !reduced
        .chars()
        .all(|c| c == PARAM || is_path_literal_char(c))
    {
        return None;
    }
    Some(ensure_leading_slash(&reduced.replace(PARAM, "{}")))
}

/// Replace each `(?P<name>...)` named group (balanced-paren span, one level of internal nesting allowed
/// — a nested group INSIDE a named param position is still part of that one `{}` hole) with [`PARAM`].
/// `None` on an unbalanced/unterminated group. A `(` that does not open a `(?P<` group is left in the
/// residual, where the metachar veto rejects it (an unnamed group is not reducible).
fn reduce_named_groups(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < s.len() {
        if s[i..].starts_with("(?P<") {
            let mut depth = 0usize;
            let mut j = i;
            let mut closed = false;
            while j < s.len() {
                match bytes[j] {
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            j += 1;
                            closed = true;
                            break;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            if !closed {
                return None;
            }
            out.push(PARAM);
            i = j;
        } else {
            let ch = s[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    Some(out)
}

/// Replace each `<...>` angle-bracket path converter with [`PARAM`]. `None` on an unterminated `<`.
fn reduce_angle_params(s: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(ch) = chars.next() {
        if ch == '<' {
            let mut closed = false;
            for c in chars.by_ref() {
                if c == '>' {
                    closed = true;
                    break;
                }
            }
            if !closed {
                return None;
            }
            out.push(PARAM);
        } else {
            out.push(ch);
        }
    }
    Some(out)
}

/// Characters a reduced REGEX residual may hold and still be a clean literal path. Deliberately EXCLUDES
/// `.` — an unescaped `.` is the regex any-char metachar, and treating it as a literal dot would guess a
/// key; a route that truly needs a literal dot (rare) is honestly skipped instead.
fn is_regex_literal_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '/' | '-' | '_' | '~')
}

/// Characters a reduced `path()` residual may hold. A `path()` route is not a regex, so a literal `.`
/// (`sitemap.xml`) is unambiguous and allowed here.
fn is_path_literal_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '/' | '-' | '_' | '~' | '.')
}

/// Ensure a single leading `/` (an empty reduced path — a bare `^$` root — becomes `/`).
fn ensure_leading_slash(s: &str) -> String {
    if s.starts_with('/') {
        s.to_string()
    } else {
        format!("/{s}")
    }
}
