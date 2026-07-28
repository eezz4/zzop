//! `raw-sql-check-then-write` — the RAW-SQL arm of the check-then-act family, sibling of the ORM-only
//! `race-condition-toctou` in the same pack (and of `db/find-then-create-no-unique`).
//!
//! Why a sibling rule and not a widened `race-condition-toctou`: that rule's `file_pattern` is a
//! house-convention PATH scope (`api/`, `*/routes/`, `*/controllers/`, `*handler.ts(x)`,
//! `*controller.ts(x)`) and its `require_file` is `.findOne|.findById|.findUnique` — an ORM vocabulary.
//! Raw-SQL backends (the Cloudflare Workers + D1 shape that motivated this) put the same race in
//! `src/createLedger.ts`, which fails BOTH gates; widening either one to reach it would silently
//! re-scope the ORM arm across every tree at the same time.
//!
//! Case-sensitivity is the precision gate, and it is the same one `parser/parser-sql/src/consume.rs`
//! settled on for raw-SQL table extraction: `"Select a date from the list"` is a syntactically valid
//! `SELECT <col> FROM <table> <alias>`, so shape analysis cannot separate it from prose — only case can.
//! The cost is an honest under-report of lowercase SQL, pinned below.

use crate::{hits, scan, TempDir};

// The measured anchor shape (an external dogfood corpus' `createLedger.ts`, reconstructed here): an
// optimistic-concurrency version check whose READ sits OUTSIDE the atomic write batch. Note the file
// path — `src/`, not `api/` — which is exactly what `race-condition-toctou` cannot scan.
#[test]
fn raw_sql_select_then_insert_with_the_read_outside_the_batch_is_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "src/createLedger.ts",
        r#"declare const env: any;
export async function postRevision(body: any) {
  const head = await env.DB.prepare(
    "SELECT revision_no FROM ledger_revisions WHERE ledger_id = ? ORDER BY revision_no DESC LIMIT 1"
  ).bind(body.id).first();
  if (head.revision_no !== body.baseRevisionNo) throw new Error("stale");
  await env.DB.batch([
    env.DB.prepare("INSERT INTO ledger_revisions (ledger_id, revision_no) VALUES (?, ?)").bind(body.id, 1)
  ]);
}
"#,
    );
    let out = scan(&dir);
    let raw = hits(&out, "raw-sql-check-then-write");
    assert_eq!(raw.len(), 1, "{:?}", out.findings);
    // The finding's line is the WRITE (the racing action), line 8 — the `INSERT INTO` literal.
    assert_eq!(raw[0].line, 8, "{:?}", out.findings);
    // Disjointness pin: the ORM sibling stays silent on raw SQL, so the two rules never double-report
    // the same defect. If someone widens `race-condition-toctou`'s vocabulary, this goes red first.
    assert!(
        hits(&out, "race-condition-toctou").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn raw_sql_select_then_update_set_is_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "src/reserveSeat.ts",
        r#"declare const db: any;
export async function reserve(id: string) {
  const row = await db.query("SELECT seats_taken FROM events WHERE id = ?", [id]);
  if (row.seats_taken >= 100) return null;
  return db.query("UPDATE events SET seats_taken = 5 WHERE id = ?", [id]);
}
"#,
    );
    let out = scan(&dir);
    assert_eq!(
        hits(&out, "raw-sql-check-then-write").len(),
        1,
        "{:?}",
        out.findings
    );
}

// ORDER-GATE pin — the same one `race-condition-toctou` carries: a write that PRECEDES the only read is
// not a check-then-act race, and the rule id asserts an order.
#[test]
fn a_raw_sql_write_that_precedes_the_only_read_is_not_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "src/appendAudit.ts",
        r#"declare const db: any;
export async function logThenLookUp(id: string) {
  await db.query("INSERT INTO audit_log (kind) VALUES (?)", ["read"]);
  const row = await db.query("SELECT revision_no FROM ledger_revisions WHERE id = ?", [id]);
  return row;
}
"#,
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "raw-sql-check-then-write").is_empty(),
        "{:?}",
        out.findings
    );
}

// CASE-SENSITIVITY pin — the whole precision argument in one fixture. Both literals are syntactically
// valid SQL statement heads; only their case says they are English. If either statement-head pattern
// ever gains `(?i)`, this goes red.
#[test]
fn lowercase_sql_shaped_english_prose_is_not_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "src/copy.ts",
        r#"export function labels() {
  const a = "Select a date from the list";
  const b = "Insert into the notes field below";
  return [a, b];
}
"#,
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "raw-sql-check-then-write").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn an_upsert_spelled_on_conflict_is_not_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "src/upsertMember.ts",
        r#"declare const db: any;
export async function join(id: string) {
  const row = await db.query("SELECT id FROM members WHERE id = ?", [id]);
  if (row) return row;
  return db.query("INSERT INTO members (id) VALUES (?) ON CONFLICT (id) DO NOTHING", [id]);
}
"#,
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "raw-sql-check-then-write").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn a_for_update_row_lock_is_not_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "src/lockSeat.ts",
        r#"declare const db: any;
export async function reserve(id: string) {
  const row = await db.query("SELECT seats_taken FROM events WHERE id = ? FOR UPDATE", [id]);
  if (row.seats_taken >= 100) return null;
  return db.query("INSERT INTO reservations (event_id) VALUES (?)", [id]);
}
"#,
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "raw-sql-check-then-write").is_empty(),
        "{:?}",
        out.findings
    );
}

// A read with no following write is not this rule's shape.
#[test]
fn a_raw_sql_read_with_no_write_is_not_flagged() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "src/getLedger.ts",
        r#"declare const db: any;
export async function get(id: string) {
  const row = await db.query("SELECT revision_no FROM ledger_revisions WHERE id = ?", [id]);
  return row;
}
"#,
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "raw-sql-check-then-write").is_empty(),
        "{:?}",
        out.findings
    );
}

// Suppression is anchored on the TRIGGER line (the write), same as the ORM sibling.
#[test]
fn the_ok_marker_directly_above_the_write_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-sql");
    dir.write(
        "src/markedLedger.ts",
        r#"declare const db: any;
export async function postRevision(id: string) {
  const row = await db.query("SELECT revision_no FROM ledger_revisions WHERE id = ?", [id]);
  if (!row) return null;
  // zzop-raw-sql-check-then-write-ok: single-writer admin path
  return db.query("INSERT INTO ledger_revisions (id) VALUES (?)", [id]);
}
"#,
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "raw-sql-check-then-write").is_empty(),
        "{:?}",
        out.findings
    );
}
