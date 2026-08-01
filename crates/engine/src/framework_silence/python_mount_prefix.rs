//! S14 — the unread router-mount-prefix self-report for Python.
//!
//! `parser-python-3`'s `match_include_router` takes a `prefix=` directly only when it is a string
//! literal. A DOTTED reference now rides as a `RouterMountEntry::MountRef` and is resolved at assemble
//! time, but every other shape (a call, an f-string, a subscript) still cannot be read at all — and
//! skipping the mount does NOT skip the routes. The child router simply stops being mounted, so its
//! routes are emitted at their own paths, without the prefix the deployment actually serves.
//!
//! That is the wrong-key family again (S12 gateway rewrites, S13 C# base controllers), now in Python:
//! not an absence, a confident wrong answer, which reads exactly like a correct one.
//!
//! A dotted prefix is deliberately NOT reported here: the composer either resolves it or names the exact
//! ref it could not, and doing both would tell a tree whose prefix resolved perfectly that it may be
//! mis-keyed. This line owns only the shapes nothing downstream will ever speak for.
//!
//! ## The measurement that motivated it, and the part of it that was wrong
//! On the 17-tree corpus join (2026-08-01 pre-tag audit), `be-fastapi-fs` mounts every route with
//! `app.include_router(api_router, prefix=settings.API_V1_STR)` where `API_V1_STR = "/api/v1"` sits in
//! `app/core/config.py`. Result: **22 of the join's 24 `unprovidedConsumes` were the same key minus
//! that prefix**, sitting in `unconsumedProvides` at the same time — a whole frontend reported as
//! calling routes nobody serves, while the server that serves them was in the same run. None of the
//! 108 warnings that run emitted said why.
//!
//! ⚠ That measurement supported "the prefix is unread", but the inference drawn from it — *"reading the
//! prefix would join those 22"* — was WRONG, and re-measuring after the ref channel shipped is what
//! showed it. On that corpus the join does not move, because the same tree ALSO cannot resolve
//! `app.api.main` to `backend/app/api/main.py`: `python_import_candidates` tries the tree root and
//! `src/`, and this project interposes `backend/`. Reading the prefix is NECESSARY and not SUFFICIENT
//! there. The resolution is proven end to end by
//! `analyze_python_cross_layer::a_non_literal_include_router_prefix_resolves_through_the_const_map`,
//! not by that corpus.
//!
//! ## Scope, stated rather than implied
//! This reports that a prefix was NOT READ. It does not report whether the prefix mattered, because
//! that needs the value. A tree whose unread prefix happens to be `""` gets a warning it can dismiss in
//! one glance; a tree whose consume side is keyed the same wrong way (both sides in-tree, both missing
//! the prefix) still joins fine and also gets the warning. Both are cheap next to the measured cost of
//! silence above. What this must never do is claim the routes ARE mis-keyed — it says they are keyed
//! without a prefix zzop could not read, which is exactly what is known.

use std::path::Path;

/// Mount sites named in the message, at most this many.
const MAX_EXAMPLES: usize = 3;

/// How far past `include_router(` to scan for the matching `)`. A mount call is a few lines; the bound
/// keeps a minified or generated file from turning one unbalanced paren into a whole-file scan.
const MAX_CALL_SPAN: usize = 2_000;

/// One warning when this tree mounts FastAPI routers with a prefix the parser could not read, given a
/// tree that produced http provides at all. `None` otherwise.
pub fn python_mount_prefix_warning(
    root: &Path,
    py_rels: &[String],
    http_provide_count: usize,
) -> Option<String> {
    // With no routes extracted, an unread prefix is not this tree's problem — S1 is already saying
    // something larger, and adding this line would bury it.
    if http_provide_count == 0 {
        return None;
    }
    let mut sites: Vec<String> = Vec::new();
    let mut total = 0usize;
    for rel in py_rels {
        let Ok(text) = std::fs::read_to_string(root.join(rel)) else {
            continue;
        };
        if !text.contains("include_router") {
            continue;
        }
        for expr in unread_prefix_exprs(&text)
            .into_iter()
            .filter(|e| !is_dotted(e))
        {
            total += 1;
            if sites.len() < MAX_EXAMPLES {
                sites.push(format!("{rel} (prefix={expr})"));
            }
        }
    }
    if total == 0 {
        return None;
    }
    Some(format!(
        "Unread router mount prefix(es): {total} `include_router(...)` call(s) pass a `prefix=` this \
         build cannot read because it is not a string literal, e.g. {}. The prefix is skipped rather \
         than guessed, so every route under those routers is keyed WITHOUT it — `GET /users/me` where \
         the app serves `GET /api/v1/users/me`. That is a wrong key rather than a missing one, so the \
         cross-layer join reports unprovided consumes for routes that ARE served, and the matching \
         provides sit in the same reply's `crossLayer.unconsumedProvides`. Declare the effective prefix with \
         `trees[].topology.mountedAt` in your config, or pass a literal prefix at the mount.",
        sites.join(", ")
    ))
}

/// The `prefix=` argument EXPRESSION of every `include_router(...)` call whose prefix is not a plain
/// string literal, in source order.
///
/// ## Declared limits
/// * Lexical: a `prefix=` inside a string or comment within the call's own parentheses is read as an
///   argument. A mount call is short and this has no observed instance, but it is not impossible.
/// * An f-string counts as unread, and correctly: the parser matches `Expr::StringLiteral`, and an
///   f-string is a different node even when it interpolates nothing.
/// * Implicit concatenation (`prefix="/api" "/v1"`) reads as a literal here and is skipped by the
///   parser — a false NEGATIVE, i.e. this line stays quiet where it should speak. Unmeasured in any
///   corpus held here; written down rather than left for someone to rediscover.
fn unread_prefix_exprs(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut from = 0usize;
    while let Some(hit) = text[from..].find("include_router") {
        let at = from + hit;
        from = at + "include_router".len();
        // Whole-word: `my_include_router(` is a different function.
        if at > 0 {
            let prev = bytes[at - 1];
            if prev.is_ascii_alphanumeric() || prev == b'_' {
                continue;
            }
        }
        let Some(open) = text[from..].find('(').map(|i| from + i) else {
            continue;
        };
        // Only whitespace may sit between the name and its `(` — otherwise this is not the call.
        if !text[from..open].trim().is_empty() {
            continue;
        }
        let Some(args) = call_args(text, open) else {
            continue;
        };
        if let Some(expr) = prefix_expr(args) {
            if !is_string_literal(&expr) {
                out.push(expr);
            }
        }
    }
    out
}

/// The text between `open`'s `(` and its matching `)`, or `None` when unbalanced within
/// [`MAX_CALL_SPAN`].
fn call_args(text: &str, open: usize) -> Option<&str> {
    let mut depth = 0usize;
    for (i, c) in text[open..].char_indices() {
        if i > MAX_CALL_SPAN {
            return None;
        }
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[open + 1..open + i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// The `prefix=<expr>` argument's expression text, trimmed, or `None` when the call passes no `prefix`.
/// Stops at the top-level comma that ends the argument, so a nested call keeps its own commas.
fn prefix_expr(args: &str) -> Option<String> {
    let mut from = 0usize;
    let bytes = args.as_bytes();
    let at = loop {
        let hit = from + args[from..].find("prefix")?;
        from = hit + "prefix".len();
        let before_ok =
            hit == 0 || !(bytes[hit - 1].is_ascii_alphanumeric() || bytes[hit - 1] == b'_');
        // `prefix =` and `prefix=` both bind; `prefix==` is a comparison, not a keyword argument.
        let rest = args[from..].trim_start();
        if before_ok && rest.starts_with('=') && !rest.starts_with("==") {
            break from + args[from..].find('=')? + 1;
        }
    };
    let mut depth = 0usize;
    for (i, c) in args[at..].char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => return Some(args[at..at + i].trim().to_string()),
            _ => {}
        }
    }
    Some(args[at..].trim().to_string())
}

/// Whether an expression is a plain dotted access (`settings.API_V1_STR`) — the shape the parser now
/// carries as a `RouterMountEntry::MountRef` for the composer to resolve against the project-wide const
/// map.
///
/// These are EXCLUDED from this warning, and that exclusion is the point: for a dotted ref the composer
/// knows the real answer. If it resolves, the routes are keyed correctly and there is nothing to report;
/// if it does not, the composer emits its own line naming the exact ref it could not resolve. Reporting
/// here too would mean a tree whose prefix resolved perfectly still gets told its routes may be
/// mis-keyed — measured on a fixture the moment the resolution landed, which is how this narrowing was
/// found. What remains for this line is the shapes that can never become a ref at all.
fn is_dotted(expr: &str) -> bool {
    let e = expr.trim();
    !e.is_empty()
        && !e.starts_with(|c: char| c.is_ascii_digit())
        && e.contains('.')
        && e.chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
}

/// Whether an expression is a plain string literal — the only shape the parser reads. An `f`/`rf`/`b`
/// prefix disqualifies it: those are different AST nodes, so the parser skips them too.
fn is_string_literal(expr: &str) -> bool {
    let e = expr.trim();
    (e.starts_with('"') && e.ends_with('"') && e.len() >= 2)
        || (e.starts_with('\'') && e.ends_with('\'') && e.len() >= 2)
}

#[cfg(test)]
#[path = "python_mount_prefix_tests.rs"]
mod tests;
