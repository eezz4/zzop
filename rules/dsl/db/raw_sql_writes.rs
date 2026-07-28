//! The RAW-SQL write arm of three `db` rules whose write vocabulary was ORM-only until now
//! (`multi-write-no-tx`, `write-in-loop-no-tx`, `empty-catch-and-write`).
//!
//! Background: the pack's write patterns all matched
//! `<prisma|db|tx|client|repo>.<model>.<create|update|…>(`, so a backend that talks to its database
//! through SQL strings (`env.DB.prepare("INSERT INTO …")` — Cloudflare Workers + D1, `pg`, `mysql2`,
//! `better-sqlite3`) tripped none of them. The GUARD side of the same rules already spoke SQL
//! (`'BEGIN'`/`'COMMIT'`/`'ROLLBACK'` in `manual-tx-no-rollback`, the `sql-begin` veto here), so what
//! shipped was a pack that could recognize a raw-SQL transaction but not a raw-SQL write.
//!
//! Two decisions worth stating because they are not symmetric with the ORM arm:
//!   * `.batch(` is a NEW veto on the two transaction-vetoed rules. On D1 a `batch([...])` IS the
//!     transaction — without this, every correctly-batched multi-write would be reported.
//!   * Statement heads are UPPERCASE-only and must start a string literal or a line, the same gate
//!     `parser/parser-sql/src/consume.rs` uses: `"Insert into the notes field"` is not SQL.
//!
//! Rules deliberately left ORM-only, so a later reader does not read their absence as an oversight:
//!   * `update-delete-no-where` — the `sql` pack's `delete-no-where`/`update-no-where` already cover
//!     the raw-SQL whole-table write; a second arm here would double-report it.
//!   * `unawaited-write` — a line-scan. The D1 idiom `await db.batch([db.prepare("INSERT …"), …])`
//!     puts un-awaited `INSERT` lines inside an argument array, so a raw arm there is a false-positive
//!     factory, not a detector.
//!   * `non-atomic-counter-update` — in raw SQL, `SET n = n + 1` is the FIX (it is atomic), not the
//!     defect the rule names.
//!   * `find-then-create-no-unique` / `check-then-act-in-loop` — the raw-SQL arm of that family ships
//!     as `sql/raw-sql-check-then-write` instead; arming these too would make the two co-fire.

use crate::{hits, scan, TempDir};

// --- multi-write-no-tx --------------------------------------------------------------------------

#[test]
fn two_raw_sql_write_families_with_no_transaction_are_flagged() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/transfer.ts",
        r#"declare const db: any;
export async function transfer(from: string, to: string) {
  await db.query("INSERT INTO ledger (account_id, delta) VALUES (?, ?)", [to, 10]);
  await db.query("UPDATE accounts SET balance = 0 WHERE id = ?", [from]);
}
"#,
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "multi-write-no-tx").len(),
        1,
        "{:?}",
        out.findings
    );
}

// D1's `batch([...])` IS the transaction — the veto that keeps the rule above off correct code.
#[test]
fn two_raw_sql_writes_inside_a_d1_batch_are_not_flagged() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/transferBatched.ts",
        r#"declare const env: any;
export async function transfer(from: string, to: string) {
  await env.DB.batch([
    env.DB.prepare("INSERT INTO ledger (account_id, delta) VALUES (?, ?)").bind(to, 10),
    env.DB.prepare("UPDATE accounts SET balance = 0 WHERE id = ?").bind(from)
  ]);
}
"#,
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "multi-write-no-tx").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- write-in-loop-no-tx ------------------------------------------------------------------------

#[test]
fn a_raw_sql_write_inside_a_loop_with_no_transaction_is_flagged() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/importRows.ts",
        r#"declare const db: any;
export async function importRows(rows: any[]) {
  for (const row of rows) {
    await db.query("INSERT INTO staged (id) VALUES (?)", [row.id]);
  }
}
"#,
    );
    let out = scan(&dir);
    let found = hits(&out, "write-in-loop-no-tx");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(found[0].line, 4, "{:?}", out.findings);
}

#[test]
fn a_raw_sql_write_outside_every_loop_is_not_flagged_by_the_loop_rule() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/importOne.ts",
        r#"declare const db: any;
export async function importOne(row: any) {
  await db.query("INSERT INTO staged (id) VALUES (?)", [row.id]);
}
"#,
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "write-in-loop-no-tx").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- empty-catch-and-write ----------------------------------------------------------------------

#[test]
fn a_raw_sql_write_alongside_an_empty_catch_is_flagged() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/swallow.ts",
        r#"declare const db: any;
export async function record(id: string) {
  try {
    await db.query("INSERT INTO events (id) VALUES (?)", [id]);
  } catch (e) {}
}
"#,
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "empty-catch-and-write").len(),
        1,
        "{:?}",
        out.findings
    );
}

// --- the case gate, shared by all three arms ----------------------------------------------------

// Lowercase SQL-shaped English prose is not a write. Both statement heads below are syntactically
// valid SQL; only their case says otherwise. If any raw arm ever gains `(?i)`, this goes red.
#[test]
fn lowercase_sql_shaped_prose_arms_none_of_the_raw_write_rules() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/copy.ts",
        r#"export async function labels(rows: any[]) {
  const a = "Insert into the notes field below";
  const b = "update accounts set by the admin";
  for (const row of rows) {
    console.log(a, b, row);
  }
  try {
    console.log(a);
  } catch (e) {}
  return [a, b];
}
"#,
    );
    let out = scan(&dir);
    for rule in [
        "multi-write-no-tx",
        "write-in-loop-no-tx",
        "empty-catch-and-write",
    ] {
        assert!(hits(&out, rule).is_empty(), "{rule}: {:?}", out.findings);
    }
}
