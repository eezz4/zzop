//! zzop-parser-sql — SQL frontend, both directions of the `db-table` channel.
//!
//! **PROVIDE side** ([`extract_db_table_provides`], `extract`): line/regex-level extraction of
//! `CREATE TABLE` statements from `.sql` files.
//! **CONSUME side** ([`extract_statement_table_refs`], `consume`): the table names ONE SQL statement
//! string reads or writes. Input is a string another parser already holds — a TypeScript string
//! literal, say — so an ORM-less stack (Cloudflare D1, `better-sqlite3`, `pg`, `mysql2`) projects
//! db-table consumes without any rule re-lexing source text. SQL vocabulary lives here and only here;
//! the calling parser passes text and gets back canonical keys.
//!
//! Line/regex-level extraction of `CREATE TABLE` statements from
//! `.sql` files into `db-table` io PROVIDEs, so the core cross-layer linker can surface
//! `cross-layer/db-table-name-in-multiple-sources` (a table declared by a DDL migration and touched by application code
//! elsewhere) and dead-provide detection (`cross-layer/unconsumed-endpoint`-style) for a declared table
//! nothing reads. Join key: `(kind="db-table", key="table:<name>")` — the same generic contract shape
//! `zzop_parser_typescript::adapters::db_table_consume` already produces on the CONSUME side (see that
//! module's own doc); this crate is the first PROVIDE-side producer feeding the channel from raw SQL
//! migrations. See `extract`'s own module doc for the extraction rules, casing convention, and
//! limitations.
//!
//! Deliberately NO tree-sitter / `sqlparser` dependency: a full SQL grammar (dialect-specific
//! expressions, generated columns, partitioning clauses, ...) is out of scope for a v1 "does this table
//! exist somewhere in a DDL migration" signal — a line/regex-level scanner is the deliberate scope, same
//! spirit as `zzop_parser_prisma`'s own line-based PSL scanner.
//!
//! Cache key ingredient for `zzop-cache` (see `zzop_parser_typescript::PARSER_FINGERPRINT`'s doc for the
//! scheme this mirrors, and `zzop_parser_prisma::PARSER_FINGERPRINT`'s doc for the "no external version
//! pin to track" reasoning this crate shares — a local regex/line scanner, not a wrapped third-party
//! crate). Bump the trailing `/vN` counter whenever `extract_db_table_provides`'s projection logic changes
//! in a way that changes its output for the same SQL text.
//! - `+sql-quoted-dot-and-temp-table-v1`: `bare_table_name`'s dot-split is now quote-aware (a `.` inside
//!   a quote pair, e.g. `"my.table"`, no longer wrongly splits into a fake schema qualifier), and
//!   `create_table_re` now recognizes a `GLOBAL`/`LOCAL`/`TEMP`/`TEMPORARY`/`UNLOGGED` modifier between
//!   `CREATE` and `TABLE`.
//! - `+dml-table-refs-v1`: the `consume` module is new — `extract_statement_table_refs` projects the
//!   CONSUME side of the channel from a SQL statement string.
///
/// **This string is an ID, not a version — it no longer has to be bumped.** `crates/engine/build.rs`
/// hashes this crate's whole dependency closure into the cache key beside it, so a change to any
/// source here invalidates on its own. What is left is the part a person reads in a cache path or a
/// bug report: which frontend parsed the file. Change it when the FRONTEND changes; correctness no
/// longer depends on remembering.
pub const PARSER_FINGERPRINT: &str = "sql/0.21.0+dml-table-refs-v1";

mod consume;
mod extract;

pub use consume::extract_statement_table_refs;
pub use extract::extract_db_table_provides;
