//! The AVAILABILITY axis — the landing, exits and message bodies for the two rules whose prescription is
//! "add an index" (`fk-no-index`, both arms, and `orderby-unindexed`).
//!
//! A sibling of `landing.rs` rather than more constants inside it, for a reason that is not filing: every
//! LANDING in that module names a prescription that FAILS at deploy time (`ADD COLUMN`, `ADD CONSTRAINT`,
//! `SET NOT NULL`, `SET DATA TYPE`) or DESTROYS values (`DROP COLUMN`/`DROP TABLE`). `CREATE INDEX` does
//! neither — the statement succeeds, the rows survive, the code stays correct. An uncontaminated auditor
//! built a prescription-breaking finding out of this rule and then killed it himself on exactly that
//! ground: *"if that counts, every index rule in the world is one."* He was right, and the cost is still
//! real; it is just a different cost. What this axis prices is AVAILABILITY — a window in which writes to
//! one table wait — and mixing it into a module whose header promises "failure" is how the next author
//! reuses the wrong sentence.
//!
//! ⚠ Tone follows from that: PRICE the edit, do not frighten anyone out of it. `fk-no-index` is a rule an
//! auditor judged FIX on the merits (cal.com `Host.groupId`, where every sibling FK on the same model
//! already carries an `@@index` and the house convention is itself the evidence). These sentences exist so
//! a reader can tell a five-millisecond build from a deploy-length one, not so they skip the index.

/// How the two "add an index" prescriptions LAND — spliced byte-identically into `fk-no-index` (both arms)
/// and `orderby-unindexed`, AHEAD of each rule's imperative (rule-quality.md §27).
///
/// ONE constant for both because the dangerous property belongs to the DDL, not to what either rule
/// detects: whether the index is wanted for a `WHERE` or for an `ORDER BY`, `@@index([...])` makes Prisma's
/// diff emit a plain `CREATE INDEX` and Postgres builds it under a `SHARE` lock. The two rules diverge only
/// in the EXITs, the same split `landing::DATA_LOSS_LANDING` and `landing::COLUMN_TYPE_CHANGE_LANDING`
/// already use across three rules and two.
///
/// **What this sentence deliberately does NOT claim**: that Prisma cannot run `CREATE INDEX CONCURRENTLY`
/// at all. The rule next door already has an escape hatch resting on a false premise, and arriving first
/// did not make it true. Measured instead (2026-08-27, `corpus/cal.com/packages/prisma/migrations/`, 595
/// `.sql` files): **240** plain `CREATE INDEX` against **2** `CONCURRENTLY`, and both concurrent files hold
/// exactly ONE statement — which is the real constraint, and is what [`CONCURRENT_INDEX_EXIT`] states. The
/// claim made here is the one that survives: Prisma's diff never GENERATES the concurrent form, so a reader
/// who follows the imperative and ships what was generated takes the lock.
///
/// Field- and model-independent for the same reason its siblings are: names belong in the imperative, so
/// the position pins have ONE spelling to compare an index against.
pub(super) const INDEX_BUILD_LANDING: &str = "COUNT THE ROWS IN THIS TABLE FIRST: adding the index is one \
     line of schema plus a deploy-time cost that is not in that line. `prisma migrate` never generates the \
     concurrent form — `@@index([...])` diffs to a plain `CREATE INDEX`, and Postgres builds that under a \
     `SHARE` lock on the table: reads keep working, and every INSERT, UPDATE and DELETE against it WAITS \
     until the build finishes. `SELECT count(*)` against the DEPLOYED database is what tells you whether \
     that wait is milliseconds or your whole deploy window.";

/// The EXIT both index rules publish after [`INDEX_BUILD_LANDING`] — the large-table way out, shared
/// byte-identically for the reason the landing is: it is one procedure, and neither rule's subject changes
/// a word of it.
///
/// The two facts a reader cannot get from the schema are why this is prose and not "use CONCURRENTLY".
/// First, `CONCURRENTLY` is not free: it cannot run inside a transaction block, so it has to be alone in
/// its migration file — which is the shape both of cal.com's two actually have — and a build that fails
/// leaves an INVALID index that still costs every write while serving no query. Second, the multiplier is
/// measured rather than imagined: `20230410234751_add_foreign_key_indexes` puts **61** `CREATE INDEX`
/// statements in ONE file, which is precisely what acting on a page of these findings at once produces.
pub(super) const CONCURRENT_INDEX_EXIT: &str = "IF THAT COUNT IS LARGE: do not let the generated migration \
     build it. Hand-edit that migration to `CREATE INDEX CONCURRENTLY IF NOT EXISTS \"<name>\" ON \
     \"<Table>\" (\"<column>\")`, which takes no write lock — it cannot run inside a transaction block, so \
     it must be the ONLY statement in its migration file, and a build that fails leaves an INVALID index \
     behind that still slows every write and serves no query until you `DROP` it and redo it. And do not \
     batch them: plain `CREATE INDEX` statements sharing one migration are built one after another before \
     the deploy returns, so the write-blocked window is their SUM, not the longest of them.";

/// The exit only `fk-no-index` publishes — the one its sibling structurally cannot have, and the reason
/// this rule does not simply share `orderby-unindexed`'s pair (rule-quality.md §30: a message describes the
/// population it still reaches).
///
/// `orderby-unindexed` is anchored at a query call site that ALREADY sorts on the column, so "check that
/// something reads this column" is a question its own finding has answered; splicing this there would hand
/// a reader a sentence undoing what they were just told — the trap `landing::NOT_NULL_LANDING` avoids by
/// not reusing `landing::FK_CONSTRAINT_LANDING`. `fk-no-index` has the opposite standing: it reads the
/// field NAME and no query at all. An uncontaminated auditor judged cal.com's `User.defaultScheduleId`
/// IGNORE on exactly this ground — all 22 use sites select it, write it, or pass it as a value, and nothing
/// filters on it — which makes the prescribed index pure write cost, on this tree's largest table.
///
/// Placed AHEAD of the landing rather than after the exits: if nothing reads the column, the size of the
/// table is irrelevant and neither exit applies. A reader who meets this last has already priced a build
/// they should not run.
pub(super) const FK_INDEX_NO_READER_EXIT: &str = "AND CHECK THAT SOMETHING ACTUALLY FILTERS ON THIS COLUMN, \
     because this rule did not: it reads the field's NAME, never your queries. A `*Id` column that code \
     only ever selects, writes, or passes around as a value is not a search key, and an index on it is pure \
     write cost — every insert and update to this table maintains a btree no query reads. If nothing filters \
     or joins on it alone, the right edit is NONE, and this finding is naming a convention rather than a \
     defect.";

/// The whole `fk-no-index` message, both arms. A builder here rather than a `format!` in `message.rs` for
/// the reason `landing::float_money_message` is one: `message.rs` sits at the 300-line cap, and a body that
/// is one clause of its own plus three of this module's sentences belongs beside them.
///
/// Both arms carry the same three sentences in the same order, because both arms prescribe the same edit.
/// The arms differ only in the observation and in the small-table imperative, which is where the field name
/// lives — the composite arm asks for a SECOND index rather than a first, and says so, because a reader who
/// reads "add `@@index([x])`" against a model that already lists a composite containing `x` will otherwise
/// assume the tool has miscounted.
pub(super) fn fk_no_index_message(
    model: &str,
    field: &str,
    params: Option<&serde_json::Value>,
) -> String {
    let get = |key: &str| params.and_then(|p| p.get(key));
    let coverage = get("coverage").and_then(|v| v.as_str()).unwrap_or("none");
    if coverage == "non-leading" {
        let cols = get("compositeCols")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        format!(
            "Model {model} field {field} is a non-leading member of the composite ({cols}) \
             @@index/@@unique — it is only covered for queries that ALSO constrain the leading \
             column(s) of that composite, not for queries filtering on {field} alone. \
             {FK_INDEX_NO_READER_EXIT} {INDEX_BUILD_LANDING} IF SOMETHING DOES FILTER ON {field} ALONE \
             AND THAT COUNT IS SMALL: add a SECOND index, `@@index([{field}])`, and ship the generated \
             migration as-is — the composite stays where it is, and the two serve different queries. \
             {CONCURRENT_INDEX_EXIT}"
        )
    } else {
        format!(
            "Model {model} field {field} looks like a foreign key but has no @@index/@@unique — queries \
             filtering on it will scan the table. {FK_INDEX_NO_READER_EXIT} {INDEX_BUILD_LANDING} IF \
             SOMETHING DOES FILTER ON IT AND THAT COUNT IS SMALL: add `@@index([{field}])` and ship the \
             generated migration as-is. {CONCURRENT_INDEX_EXIT}"
        )
    }
}
