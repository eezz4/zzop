//! `CREATE TABLE` -> `db-table` provide extraction (see crate doc for the overall scope).
//!
//! ## What counts as a provide
//! A `CREATE TABLE` statement (case-insensitive keyword match) — optionally with a `GLOBAL`/`LOCAL`/
//! `TEMP`/`TEMPORARY`/`UNLOGGED` modifier (or several, e.g. `CREATE GLOBAL TEMPORARY TABLE`) between
//! `CREATE` and `TABLE` — optionally followed by `IF NOT EXISTS`, followed by a table name: a plain
//! identifier, a quoted identifier (`"users"`, `` `users` ``, `[users]`), or a schema-qualified name
//! (`public.users`, `"public"."users"`, `[dbo].[users]`) — the LAST dot-separated segment is the key
//! (a dot INSIDE a quote pair does not count as a separator — a quoted identifier that literally
//! contains a dot, e.g. `"my.table"`, is one whole name), and everything before that last segment (the
//! schema/catalog qualifier) is dropped. Rationale: a `db-table` join key is cross-layer identity for
//! "which table", and application code touching the same table rarely repeats the schema qualifier at
//! the call site (an ORM model/accessor almost always names the bare table) — keeping the qualifier
//! would make the DDL-side key un-joinable against the app-side key for the common case, defeating the
//! channel's purpose.
//!
//! ## Casing — the channel canonical: lower-first (accessor casing)
//! The extracted name gets its FIRST character lowercased (nothing else changes; the quote pair, if
//! any, is stripped first). This is the `db-table` channel's canonical transform, decided at review
//! when this crate landed alongside the Prisma schema PROVIDE side: `zzop_parser_prisma::analysis::
//! accessor_casing` lower-firsts model names (`model Article` -> `table:article`) to meet the
//! client-side consume keys (`prisma.article...` -> `table:article`), and a Prisma-generated
//! migration declares `CREATE TABLE "Article"` — without the same transform here, the SAME physical
//! table would ride two different keys (`table:Article` vs `table:article`) in one tree. Lower-first
//! is a no-op for the already-lowercase/snake_case names typical of hand-written DDL (`users`,
//! `article_tags`), so non-Prisma stacks are unaffected. snake_case DDL vs camelCase accessors still
//! cannot be joined by casing alone (needs `@@map` awareness — documented out of v1 scope in the
//! policy inventory).
//!
//! ## Ignored on purpose
//! `ALTER TABLE` / `INSERT INTO` / `DROP TABLE` / views / indexes / everything else: v1 scope is `CREATE
//! TABLE` only (`DROP TABLE` is a DSL line-scan rule's business, not this parser's). A `CREATE TABLE`
//! occurrence inside a string literal or a `--`/`/* */` comment is not filtered out — this is a
//! regex-level scanner, not a tokenizer, and a real `CREATE TABLE` hiding inside a comment/string in an
//! actual migration file is not a realistic shape worth the complexity of comment/string stripping. A
//! malformed/unparseable name (e.g. an empty quoted identifier, or a trailing bare qualifier like
//! `public.`) is skipped rather than guessed — no entry is emitted for that occurrence.

use std::sync::OnceLock;

use regex::Regex;
use zzop_core::IoProvide;

/// Extract `db-table` provide entries from one `.sql` file's raw source, in file order (top to bottom —
/// `Regex::captures_iter` walks the text left-to-right, which is line-ascending for well-formed text, so
/// no separate sort is needed). Empty for a file with no `CREATE TABLE` statement (a non-DDL `.sql`
/// script — seed data, a plain query file, ...).
pub fn extract_db_table_provides(rel: &str, text: &str) -> Vec<IoProvide> {
    let mut out = Vec::new();
    for caps in create_table_re().captures_iter(text) {
        let Some(whole) = caps.get(0) else {
            continue;
        };
        // A `TEMP`/`TEMPORARY` table (incl. `GLOBAL`/`LOCAL TEMPORARY`) is session-local, not a shared
        // persistent schema object, so it must NOT mint a cross-layer `db-table` provide (a temp table
        // named `results`/`staging`/... would false-join against an ORM accessor). `UNLOGGED` stays
        // (persistent, just not crash-safe). Group 1 is the modifier run.
        if caps
            .get(1)
            .is_some_and(|m| m.as_str().to_ascii_uppercase().contains("TEMP"))
        {
            continue;
        }
        let Some(name_raw) = caps.get(2) else {
            continue;
        };
        let Some(name) = bare_table_name(name_raw.as_str()) else {
            continue;
        };
        out.push(IoProvide {
            kind: "db-table".to_string(),
            key: format!("table:{name}"),
            file: rel.to_string(),
            line: line_of(text, whole.start()),
            symbol: None,
            body: None,
        });
    }
    out
}

/// 1-based line number of byte offset `pos` in `text` (count of `\n` bytes before it, + 1). `pos` is
/// always a regex match boundary here, so it always lands on a valid UTF-8 char boundary.
fn line_of(text: &str, pos: usize) -> u32 {
    text.as_bytes()[..pos]
        .iter()
        .filter(|&&b| b == b'\n')
        .count() as u32
        + 1
}

/// The bare table name: the LAST dot-separated segment of a possibly schema-qualified, possibly-quoted
/// name token, quote pair stripped, then lower-firsted via [`zzop_core::db_table_channel_casing`] (the
/// channel canonical — module doc "Casing"; the same shared transform `zzop_parser_prisma::analysis::
/// accessor_casing` calls, so the two independent extractors cannot drift). `None` when the last segment
/// is empty (a malformed trailing dot, e.g. `public.`) — skipped rather than guessed (see module doc).
///
/// The dot-split is quote-aware: a `.` INSIDE a `"..."`/`` `...` ``/`[...]` quote pair does not split
/// (same "quoted identifier is opaque" rule [`SEGMENT`] already applies at the regex level) — so a quoted
/// identifier that happens to literally contain a dot, e.g. `"my.table"`, is one whole segment named
/// `my.table`, NOT split into a fake schema-qualifier `my` + bare name `table`. Only a `.` outside any
/// quote pair (schema-qualification, e.g. `public.users` / `"public"."users"`) splits.
fn bare_table_name(raw: &str) -> Option<String> {
    let mut last_start = 0usize;
    let mut quote_close: Option<u8> = None;
    for (i, &b) in raw.as_bytes().iter().enumerate() {
        if let Some(close) = quote_close {
            if b == close {
                quote_close = None;
            }
            continue;
        }
        match b {
            b'"' => quote_close = Some(b'"'),
            b'`' => quote_close = Some(b'`'),
            b'[' => quote_close = Some(b']'),
            b'.' => last_start = i + 1,
            _ => {}
        }
    }
    // Safe: every byte matched above (`"`, `` ` ``, `[`, `.`) is single-byte ASCII, so `i + 1` always
    // lands on a UTF-8 char boundary even when `raw` contains multi-byte identifier characters.
    let last = raw[last_start..].trim();
    if last.is_empty() {
        return None;
    }
    let stripped = strip_quotes(last);
    if stripped.is_empty() {
        return None;
    }
    Some(zzop_core::db_table_channel_casing(&stripped))
}

/// Strips a matching `"..."` / `` `...` `` / `[...]` quote pair; returns the input unchanged for a plain
/// unquoted identifier (none of those three shapes).
fn strip_quotes(s: &str) -> String {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 {
        let (first, last) = (bytes[0], bytes[bytes.len() - 1]);
        if (first == b'"' && last == b'"')
            || (first == b'`' && last == b'`')
            || (first == b'[' && last == b']')
        {
            return s[1..s.len() - 1].to_string();
        }
    }
    s.to_string()
}

/// One identifier segment: a quoted form (double-quote / backtick / bracket) or a plain
/// `[A-Za-z_$][\w$]*` identifier. Shared by the two repetitions inside [`create_table_re`]'s pattern.
const SEGMENT: &str = r#"(?:"[^"]*"|`[^`]*`|\[[^\]]*\]|[A-Za-z_$][\w$]*)"#;

/// `CREATE [modifier...] TABLE [IF NOT EXISTS] <name>` — `<name>` (group 1) is one or more [`SEGMENT`]s
/// joined by `.` (schema-qualification), captured as one whole string; [`bare_table_name`] trims it to
/// the last (unquoted-dot) segment. Case-insensitive (`(?i)`) so `create table` / `Create Table` /
/// `CREATE TABLE` all match.
///
/// The optional modifier slot between `CREATE` and `TABLE` is a closed keyword set — `GLOBAL` / `LOCAL` /
/// `TEMP` / `TEMPORARY` / `UNLOGGED` — repeatable (`CREATE GLOBAL TEMPORARY TABLE t`) so Postgres/ANSI
/// temporary/unlogged-table DDL is recognized. Deliberately closed rather than `\S+`/`\w+` (any word) so
/// this stays `CREATE TABLE`-shaped and doesn't start matching unrelated `CREATE <noise> TABLE` text.
///
/// Capture group 1 is the modifier run (possibly empty), group 2 the table name — the caller reads group
/// 1 to drop `TEMP`/`TEMPORARY` tables (session-local, not a shared persistent schema object; see
/// `extract_db_table_provides`).
fn create_table_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(&format!(
            r"(?i)\bCREATE\s+((?:(?:GLOBAL|LOCAL|TEMP|TEMPORARY|UNLOGGED)\s+)*)TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?({SEGMENT}(?:\s*\.\s*{SEGMENT})*)",
        ))
        .unwrap()
    })
}

#[cfg(test)]
mod tests;
