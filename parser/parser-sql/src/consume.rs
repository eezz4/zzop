//! DML table-reference extraction — the CONSUME-side twin of [`crate::extract`]'s `CREATE TABLE`
//! PROVIDE side, for SQL that lives inside application source as a string rather than in a `.sql`
//! file. Input is ONE candidate string (a TypeScript string literal, say); output is the bare table
//! names that statement reads or writes, already in the `db-table` channel's canonical casing.
//!
//! ## Statement-shape gate — why a keyword alone is not enough
//! A caller hands this function every string literal it saw, so "contains the word SELECT" would turn
//! an i18n label into a `db-table` consume. Extraction therefore starts only when the WHOLE string
//! opens with a complete statement head: `SELECT ... FROM` / `WITH ... SELECT ... FROM` /
//! `INSERT [OR <verb>] INTO` / `REPLACE INTO` / `UPDATE <name> SET` / `DELETE FROM` — leading
//! whitespace allowed, nothing else before it.
//!
//! ## Keywords must be UPPERCASE — and that is the whole precision gate
//! Every pattern here is case-SENSITIVE. English prose is what makes this necessary rather than
//! pedantic: `"Select a date from the list"` opens with `Select`, reaches `from`, and is followed by
//! a bare word — structurally indistinguishable from `SELECT <col> FROM <table> <alias>`, because SQL
//! permits an implicit alias. No amount of shape analysis separates the two; letter case does, since
//! embedded SQL uppercases its keywords by near-universal convention and prose never uppercases
//! `FROM`. The cost is an honest under-approximation: `select id from users` written in lower case is
//! not recognized, and the tree simply reports no consume for it (absence is never a claim of "no
//! table access" — see the crate's graceful-degrade contract). This trade was measured, not assumed:
//! all 46 query sites in the Cloudflare-D1 dogfood corpus uppercase their keywords.
//!
//! ## What is extracted
//! `FROM <name>` (which also covers `DELETE FROM`), `JOIN <name>`, `INTO <name>` (`INSERT INTO`,
//! `INSERT OR REPLACE INTO`, `REPLACE INTO`, T-SQL `SELECT ... INTO`), and `UPDATE <name> SET`.
//! `<name>` is one or more [`crate::extract::SEGMENT`]s joined by `.`, run through
//! [`crate::extract::bare_table_name`] — the SAME transform the DDL provide side uses, so a consume
//! key can never drift from the provide key for the same physical table.
//!
//! ## Deliberate under-approximation (never-guess)
//! - **Comma-joined tables** (`FROM a, b`) contribute only `a`: one name per keyword occurrence.
//! - **A subquery** (`FROM (SELECT ...)`) contributes nothing at that position; the inner `FROM`
//!   still matches on its own.
//! - **`EXTRACT`/`SUBSTRING`/`TRIM`/`POSITION`/`OVERLAY`** use `FROM` as an argument separator
//!   (`EXTRACT(YEAR FROM created_at)`); those `FROM`s are vetoed so a COLUMN never mints a fake
//!   table key.
//! - **CTE names** (`WITH recent AS (...) SELECT ... FROM recent`) are vetoed: `recent` is a query-
//!   local alias, not a table, and minting `table:recent` would false-join a real table of that name.
//! - No dialect grammar, no tokenizer: a regex-level scanner, the same deliberate scope the crate
//!   doc states for the DDL side.

use std::collections::HashSet;
use std::sync::OnceLock;

use regex::Regex;

use crate::extract::{bare_table_name, SEGMENT};

/// The distinct bare table names a SQL statement string reads or writes, in first-appearance order.
/// Empty when `text` is not statement-shaped (see the module doc's gate) or names nothing extractable.
pub fn extract_statement_table_refs(text: &str) -> Vec<String> {
    if !statement_head_re().is_match(text) {
        return Vec::new();
    }
    let vetoed_from = separator_from_offsets(text);
    let cte_names = cte_names(text);

    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<String> = Vec::new();

    for caps in table_ref_re().captures_iter(text) {
        // Group 1 = `FROM`/`JOIN`/`INTO` target, group 2 = `UPDATE <name> SET` target. Exactly one
        // is set per match (the alternation's two arms).
        let raw = match (caps.get(1), caps.get(2)) {
            (Some(m), _) => {
                // A `FROM` that belongs to `EXTRACT(... FROM col)` and friends names a column, not a
                // table. The recorded offsets are keyword starts, and this arm's whole match starts
                // at the same keyword (`\b` is zero-width).
                let keyword_start = caps.get(0).map(|w| w.start()).unwrap_or(0);
                if vetoed_from.contains(&keyword_start) {
                    continue;
                }
                m.as_str()
            }
            (None, Some(m)) => m.as_str(),
            (None, None) => continue,
        };
        let Some(name) = bare_table_name(raw) else {
            continue;
        };
        if !cte_names.contains(&name) && seen.insert(name.clone()) {
            out.push(name);
        }
    }
    out
}

/// Byte offsets (of the whole `FROM <name>` match) where the `FROM` is an argument separator inside a
/// standard SQL function call rather than a table clause — see the module doc's never-guess list.
/// `[^()]*` keeps the scan inside ONE parenthesis group, so a genuine subquery `FROM` is never vetoed.
fn separator_from_offsets(text: &str) -> HashSet<usize> {
    separator_from_re()
        .captures_iter(text)
        .filter_map(|c| c.get(1).map(|m| m.start()))
        .collect()
}

/// Names bound by a common-table-expression head (`WITH x AS (`, `, y AS (`), already channel-cased so
/// they compare against extracted names directly.
fn cte_names(text: &str) -> HashSet<String> {
    cte_re()
        .captures_iter(text)
        .filter_map(|c| c.get(1))
        .filter_map(|m| bare_table_name(m.as_str()))
        .collect()
}

/// The statement-head gate (module doc). `[\s\S]*?` rather than `.*?` so a multi-line statement — the
/// normal shape for an embedded query — still reaches its `FROM`.
fn statement_head_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(&format!(
            r"^\s*(?:SELECT\b[\s\S]*?\bFROM\b|WITH\b[\s\S]*?\bSELECT\b[\s\S]*?\bFROM\b|INSERT\s+(?:OR\s+[A-Za-z]+\s+)?INTO\b|REPLACE\s+INTO\b|DELETE\s+FROM\b|UPDATE\s+(?:OR\s+[A-Za-z]+\s+)?{SEGMENT}(?:\s*\.\s*{SEGMENT})*\s+SET\b)",
        ))
        .unwrap()
    })
}

/// `FROM`/`JOIN`/`INTO` (group 1) and `UPDATE <name> SET` (group 2) table positions.
fn table_ref_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        let name = format!(r"{SEGMENT}(?:\s*\.\s*{SEGMENT})*");
        Regex::new(&format!(
            r"(?:\b(?:FROM|JOIN|INTO)\s+({name})|\bUPDATE\s+(?:OR\s+[A-Za-z]+\s+)?({name})\s+SET\b)",
        ))
        .unwrap()
    })
}

/// `EXTRACT(YEAR FROM col)` / `SUBSTRING(s FROM 1)` / `TRIM(BOTH ' ' FROM s)` / `POSITION(a IN b)`-style
/// `OVERLAY(... FROM ...)`: group 1 starts at the `FROM` keyword so the offset lines up with
/// [`table_ref_re`]'s whole-match start.
fn separator_from_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\b(?:EXTRACT|SUBSTRING|TRIM|POSITION|OVERLAY)\s*\([^()]*?(\bFROM\b)").unwrap()
    })
}

/// A CTE binding head: `WITH <name> AS (` / `WITH RECURSIVE <name> AS (` / `, <name> AS (`.
fn cte_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(&format!(
            r"(?:\bWITH\s+(?:RECURSIVE\s+)?|,\s*)({SEGMENT})\s+AS\s*\(",
        ))
        .unwrap()
    })
}

#[cfg(test)]
mod tests;
