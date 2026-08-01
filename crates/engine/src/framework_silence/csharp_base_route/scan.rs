//! S13's lexical scanner — the part that reads C# text and yields class declarations.
//!
//! Split from the parent purely by size, but the seam is real: everything here is about SPELLING (what
//! a declaration looks like in source text), and nothing here knows what a route prefix is. The parent
//! owns the judgement; this module owns the reading. Its declared limits live on [`class_declarations`]
//! and [`strip_comments`] — an undeclared limit is a silence, and silence is this family's fatal direction.

use super::BASE_LIST_LOOKAHEAD;
/// One class declaration that carries a base list.
pub(super) struct ClassDecl {
    pub(super) name: String,
    pub(super) base: String,
    /// Index into the stripped lines of the line carrying the class NAME — the anchor `declares_own_route`
    /// needs. Carried rather than re-found by name: a `UsersControllerBase` declared above a
    /// `UsersController` contains the shorter name as a substring, and a re-find binds to the wrong one
    /// (silencing the real suspect, which is the failure this module exists to prevent).
    pub(super) line: usize,
    pub(super) is_controller: bool,
}

/// Every `class` declaration with a base list, over comment-stripped lines.
///
/// ## Declared limits — the spellings this scan does NOT see
/// * `record` / `struct` / `interface` declarations. ASP.NET controllers must be classes, so this is a
///   restriction on the input space rather than a gap: a record on a project base is a DTO.
/// * A class whose name or base is spelled across a line break other than at the base-list `:`.
/// * A base list starting more than [`BASE_LIST_LOOKAHEAD`] lines below the name.
/// * A base list whose head is an INTERFACE is treated as "no base class". C# only requires the base
///   class to come first IF there is one — an interface-only list means no inherited prefix exists, so
///   dropping it is correct. The test is the `IPascal` naming convention, which is universal in .NET but
///   is a convention, not a language rule: a base class named `IfcRoot` reads as an interface here.
/// * A `partial` half in a file that does not itself contain `[Http`/`[Route`.
pub(super) fn class_declarations(lines: &[String]) -> Vec<ClassDecl> {
    let mut out = Vec::new();
    for (idx, line) in lines.iter().enumerate() {
        let Some(after_kw) = after_class_keyword(line) else {
            continue;
        };
        // Name runs to the first of `<` (generics), `(` (C# 12 primary constructor), `:`, `{` or space.
        let name: String = after_kw
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if name.is_empty() || name.chars().next().is_some_and(|c| c.is_numeric()) {
            continue;
        }
        let rest = &after_kw[name.len()..];
        let Some(base) = base_head(rest, lines, idx) else {
            continue;
        };
        out.push(ClassDecl {
            is_controller: is_controller(lines, idx, &name),
            name,
            base,
            line: idx,
        });
    }
    out
}

/// The text following a `class` KEYWORD on this line, or `None`. Whole-word: `subclass X` and a
/// `MyClass` identifier must not open a declaration.
fn after_class_keyword(line: &str) -> Option<&str> {
    let mut from = 0usize;
    while let Some(hit) = line[from..].find("class ") {
        let at = from + hit;
        let before_ok = at == 0
            || !line[..at]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '_');
        if before_ok {
            return Some(line[at + "class ".len()..].trim_start());
        }
        from = at + 1;
    }
    None
}

/// The base list's HEAD type for a class whose name ends at `rest`, looking ahead for a wrapped base
/// list. `None` when there is no base class (no list, a `where` constraint's `:`, or an interface head).
fn base_head(rest: &str, lines: &[String], idx: usize) -> Option<String> {
    let mut tail = skip_balanced(rest.trim_start(), '<', '>');
    tail = skip_balanced(tail.trim_start(), '(', ')').trim_start();
    let after_colon = if let Some(t) = tail.strip_prefix(':') {
        t
    } else {
        // The base list may sit on a following line — but only if this one ends after the name.
        if !tail.is_empty() {
            return None; // a `where` constraint or an opening body: no base list here
        }
        let mut found = None;
        for look in lines.iter().skip(idx + 1).take(BASE_LIST_LOOKAHEAD) {
            let t = look.trim_start();
            if t.is_empty() {
                continue;
            }
            found = t.strip_prefix(':');
            break;
        }
        found?
    };
    let head: String = after_colon
        .trim_start()
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '.')
        .collect();
    let head = head.rsplit('.').next().unwrap_or(&head).to_string();
    if head.is_empty() || is_interface_name(&head) {
        return None;
    }
    Some(head)
}

/// `IPascal` — the .NET interface convention. `Item` and `Iso8601Clock` are NOT interfaces: the char
/// after `I` must be upper-case AND the one after THAT must exist and not be, so `IO` reads as a type
/// and `IDisposable` as an interface. A two-letter interface (`IA`) therefore reads as a base class —
/// it fires rather than hides, which is the direction this family errs in.
pub(super) fn is_interface_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next() == Some('I')
        && chars.next().is_some_and(char::is_uppercase)
        && chars.next().is_some_and(|c| !c.is_uppercase())
}

/// Skips a balanced `open`..`close` run at the head of `s` (generic parameter list, primary constructor
/// parameter list). Returns `s` unchanged when it does not start with `open`.
fn skip_balanced(s: &str, open: char, close: char) -> &str {
    if !s.starts_with(open) {
        return s;
    }
    let mut depth = 0usize;
    for (i, c) in s.char_indices() {
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return &s[i + c.len_utf8()..];
            }
        }
    }
    s
}

/// Mirrors [`attribute_controller`]'s own gate: an `[ApiController]`/`[Controller]` attribute, or a
/// simple name ending in `Controller`. Anything else is a DTO, a handler or a helper — naming one of
/// those a "controller class" is the false statement that made the first cut of this module worthless.
fn is_controller(lines: &[String], idx: usize, name: &str) -> bool {
    name.ends_with("Controller")
        || attribute_block(lines, idx)
            .any(|l| l.contains("[ApiController") || l.contains("[Controller"))
}

/// Whether a `[Route(...)]` attribute annotates the class declared at `idx` — on the declaration line
/// itself (`[Route("x")] public class C : B` is legal) or in the attribute block directly above it.
pub(super) fn declares_own_route(lines: &[String], idx: usize) -> bool {
    let on_own_line = lines[idx]
        .split("class ")
        .next()
        .is_some_and(|before| before.contains("[Route"));
    on_own_line || attribute_block(lines, idx).any(|l| l.contains("[Route"))
}

/// The attribute lines directly above `idx`. C# attributes sit immediately before the thing they
/// annotate, so the block ends at the first non-attribute, non-blank line.
fn attribute_block(lines: &[String], idx: usize) -> impl Iterator<Item = &str> {
    lines[..idx]
        .iter()
        .rev()
        .map(|l| l.trim())
        .take_while(|t| t.is_empty() || t.starts_with('['))
}

/// Source with comments and string CONTENTS blanked out, one entry per input line.
///
/// A tripwire that names an entity must not name one that exists only in a comment: the first cut
/// reported `// public class OldUsersController : LegacyApiController` as a live controller, and read
/// the pair `Users : kept` out of a `<summary>` sentence. Blanking (rather than deleting) keeps line
/// indices aligned with the source.
///
/// Declared limit: C# 11 raw strings (`"""`) are not tracked; one on a declaration line blanks the rest
/// of that line, which drops a declaration rather than inventing one.
pub(super) fn strip_comments(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_block = false;
    let mut in_verbatim = false;
    for line in text.lines() {
        let mut kept = String::with_capacity(line.len());
        let mut in_str = false;
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0usize;
        while i < chars.len() {
            let c = chars[i];
            let next = chars.get(i + 1).copied();
            if in_block {
                if c == '*' && next == Some('/') {
                    in_block = false;
                    i += 2;
                    continue;
                }
            } else if in_verbatim {
                if c == '"' {
                    if next == Some('"') {
                        i += 2; // `""` is an escaped quote inside a verbatim string
                        continue;
                    }
                    in_verbatim = false;
                }
            } else if in_str {
                if c == '\\' {
                    i += 2;
                    continue;
                }
                if c == '"' {
                    in_str = false;
                }
            } else if c == '/' && next == Some('/') {
                break; // rest of the line is a comment
            } else if c == '/' && next == Some('*') {
                in_block = true;
                i += 2;
                continue;
            } else if c == '@' && next == Some('"') {
                in_verbatim = true;
                i += 2;
                continue;
            } else if c == '"' {
                in_str = true;
            } else {
                kept.push(c);
                i += 1;
                continue;
            }
            i += 1;
        }
        out.push(kept);
    }
    out
}
