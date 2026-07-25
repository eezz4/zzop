//! Raw-SQL-string -> `db-table` CONSUME extraction — the ORM-LESS member of the db-table consume
//! family (siblings: `db_table_consume`'s Prisma accessor chains, `typeorm_repository`'s repository
//! calls, and parser-python-3's Django/SQLAlchemy adapters). A stack that talks to its database
//! through SQL text — Cloudflare D1 (`env.DB.prepare("SELECT ... FROM ledger")`), `better-sqlite3`,
//! node-`pg`, `mysql2`, or any `sql\`...\`` tag — names its tables ONLY inside a string, so every
//! existing consume producer (all of which key off an ORM symbol) is silent on it. The provide side
//! is not: a `migrations/*.sql` file already mints `table:*` provides through `zzop_parser_sql`, so
//! before this adapter such a tree showed its whole schema as "declared, consumed by nobody".
//!
//! ## Where the SQL vocabulary lives
//! Nowhere in this file. This adapter contributes exactly one thing — the set of string values a
//! TypeScript file contains, with their source lines — and hands each to
//! [`zzop_parser_sql::extract_statement_table_refs`], which owns the statement-shape gate, the table
//! extraction, and the channel's canonical casing. That is the same split the DDL provide side uses,
//! so a CONSUME key and the PROVIDE key for one physical table cannot drift; and it keeps the
//! recognizer in the producer (never in a rule re-lexing source text).
//!
//! ## What counts as a candidate string
//! Every string literal and every template literal in the file — position-agnostic on purpose. The
//! precision gate is the SQL statement SHAPE (the string must OPEN a complete statement head, in
//! UPPERCASE keywords — see `extract_statement_table_refs`'s doc for why case is load-bearing), not
//! the syntactic slot: keying on "argument of a call" would miss the very common
//! `const SELECT_ROW = "SELECT ... FROM fx_rates ..."; db.prepare(SELECT_ROW)` hoist — one file's
//! own constant — while adding no precision the shape gate does not already provide. Tagged
//! templates (`` sql`SELECT ...` ``) fall out for free: their inner template literal is visited like
//! any other.
//!
//! ## Template interpolation is never guessed
//! A `${...}` placeholder becomes a sentinel that is itself identifier-SHAPED, so it fuses with any
//! adjacent identifier characters instead of letting them stand alone; a table name that ends up
//! containing the sentinel is then dropped. `` `DELETE FROM sessions WHERE id IN (${ids})` `` yields
//! `sessions` (the table is literal and the placeholder is elsewhere), while `` `SELECT * FROM
//! ${table}` `` and `` `... FROM ${a}_${b}` `` yield nothing rather than a fabricated key — the same
//! never-guess boundary `egress`'s URL resolution draws.
//!
//! ## Not covered (honest scope)
//! Statement text assembled by concatenation or held in another file's constant is not resolved: the
//! string this file sees must itself be statement-shaped. A `CREATE TABLE` string is deliberately not
//! a consume (DDL is the provide side's business).

use swc_core::common::{SourceMap, Span};
use swc_core::ecma::ast::{Str, Tpl};
use swc_core::ecma::visit::{Visit, VisitWith};
use zzop_core::IoConsume;

/// Stands in for a `${...}` template placeholder. Deliberately identifier-shaped and all-lowercase:
/// it must GLUE to identifier characters next to it (so `${a}_${b}` cannot leave a bare `_` behind
/// that reads as a table) and it must survive the channel's lower-first casing unchanged, so the
/// veto below can find it by substring. No real table can be named this.
const PLACEHOLDER: &str = "_zzopTplHole_";

/// Extract `db-table` CONSUME entries from the SQL statement strings one TS/JS file contains.
/// Keyed `table:<name>` at extraction time (the join channel's canonical casing), anchored at the
/// line the string starts on. Empty for a test/spec file and for an unparseable one — the same
/// conventions `extract_db_table_consumes` applies.
pub fn extract_raw_sql_db_table_consumes(rel: &str, text: &str) -> Vec<IoConsume> {
    // A test fixture's SQL is not deployed DB coupling — skip before parsing, mirroring the sibling
    // db-table consume adapters.
    if zzop_core::is_test_file(rel) {
        return Vec::new();
    }
    let Some((cm, module)) = crate::parse_with_cm(rel, text) else {
        return Vec::new();
    };
    let cm_ref: &SourceMap = &cm;
    let mut collector = RawSqlCollector {
        cm: cm_ref,
        file: rel,
        out: Vec::new(),
    };
    module.visit_with(&mut collector);
    collector.out
}

struct RawSqlCollector<'a> {
    cm: &'a SourceMap,
    file: &'a str,
    out: Vec<IoConsume>,
}

impl RawSqlCollector<'_> {
    /// Runs one candidate string through the SQL extractor and records a consume per distinct table.
    fn collect(&mut self, sql: &str, span: Span) {
        let line = crate::line_of(self.cm, span.lo);
        for table in zzop_parser_sql::extract_statement_table_refs(sql) {
            if table.contains(PLACEHOLDER) {
                continue; // a name built out of an interpolation — never guessed
            }
            self.out.push(IoConsume {
                kind: "db-table".into(),
                key: Some(format!("table:{table}")),
                file: self.file.into(),
                line,
                raw: None,
                method: None,
                body: None,
                client: None,
                retry_configured: None,
            });
        }
    }
}

impl Visit for RawSqlCollector<'_> {
    fn visit_str(&mut self, s: &Str) {
        // A string holding a lone surrogate has no `&str` view; treating it as empty is right — such a
        // literal is not SQL. Same accessor the sibling adapters use.
        self.collect(s.value.as_str().unwrap_or_default(), s.span);
    }

    fn visit_tpl(&mut self, t: &Tpl) {
        // Quasis in source order, joined by the sentinel: a placeholder can neither vanish (which
        // would fuse two literal fragments into one fake identifier) nor supply an identifier.
        let joined = t
            .quasis
            .iter()
            .map(|q| q.raw.to_string())
            .collect::<Vec<_>>()
            .join(PLACEHOLDER);
        self.collect(&joined, t.span);
        t.visit_children_with(self); // nested templates inside the interpolated expressions
    }
}

#[cfg(test)]
mod tests;
