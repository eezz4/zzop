//! `CREATE TABLE` -> `db-table` provide extraction (see crate doc for the overall scope).
//!
//! ## What counts as a provide
//! A `CREATE TABLE` statement (case-insensitive keyword match), optionally followed by `IF NOT EXISTS`,
//! followed by a table name: a plain identifier, a quoted identifier (`"users"`, `` `users` ``,
//! `[users]`), or a schema-qualified name (`public.users`, `"public"."users"`, `[dbo].[users]`) — the
//! LAST dot-separated segment is the key, and everything before it (the schema/catalog qualifier) is
//! dropped. Rationale: a `db-table` join key is cross-layer identity for "which table", and application
//! code touching the same table rarely repeats the schema qualifier at the call site (an ORM
//! model/accessor almost always names the bare table) — keeping the qualifier would make the DDL-side
//! key un-joinable against the app-side key for the common case, defeating the channel's purpose.
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
        let Some(name_raw) = caps.get(1) else {
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
/// name token, quote pair stripped, then lower-firsted (the channel canonical — module doc "Casing";
/// mirrors `zzop_parser_prisma::analysis::accessor_casing`, kept as a local twin rather than a
/// cross-parser dependency edge for one 3-line transform). `None` when the last segment is empty (a
/// malformed trailing dot, e.g. `public.`) — skipped rather than guessed (see module doc).
fn bare_table_name(raw: &str) -> Option<String> {
    let last = raw.rsplit('.').next()?.trim();
    if last.is_empty() {
        return None;
    }
    let stripped = strip_quotes(last);
    let mut chars = stripped.chars();
    let first = chars.next()?;
    Some(first.to_lowercase().collect::<String>() + chars.as_str())
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

/// `CREATE TABLE [IF NOT EXISTS] <name>` — `<name>` (group 1) is one or more [`SEGMENT`]s joined by `.`
/// (schema-qualification), captured as one whole string; [`bare_table_name`] trims it to the last
/// segment. Case-insensitive (`(?i)`) so `create table` / `Create Table` / `CREATE TABLE` all match.
fn create_table_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(&format!(
            r"(?i)\bCREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?({SEGMENT}(?:\s*\.\s*{SEGMENT})*)",
        ))
        .unwrap()
    })
}

#[cfg(test)]
mod tests;
