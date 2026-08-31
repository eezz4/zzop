//! The `LANDING` clauses — the sentence each of these rules puts AHEAD of its own imperative because
//! the prescription is the risk (rule-quality.md §27), and the EXITs that follow them. They live in
//! their own module so the one spelling of each stays the one spelling: the position pins in `tests.rs`
//! compare an index against these constants, and a second copy of any of these sentences would make
//! that comparison meaningless. How many there are is what a grep for the declarations says — this
//! header does not count them, because the last two arrivals each made a written count wrong. ⚠ Do not
//! put the declaration keyword in this paragraph to spell that grep out: the sentence that used to do
//! so was itself a hit, so the command it recommended returned one more than the file holds.
//!
//! WHAT BELONGS HERE, and what does not: every LANDING in this module names a prescription that FAILS
//! at deploy time (`ADD COLUMN`, `ADD CONSTRAINT`, `SET NOT NULL`, `SET DATA TYPE`) or DESTROYS values
//! (`DROP COLUMN`/`DROP TABLE`). The AVAILABILITY axis — a statement that SUCCEEDS while blocking
//! writes, which is what `CREATE INDEX` does — is a different noun and lives in the sibling module
//! `index_build`, not here. Apply `eec4eea`'s test before adding a sixth: is that constant's noun the
//! same as this rule's failure. ⚠ The nearest miss is the one to watch —
//! [`COLUMN_TYPE_CHANGE_LANDING`] also names a lock, and reusing it for an index would tell a reader
//! their build takes ACCESS EXCLUSIVE, rewrites the whole table, and can fail mid-deploy on a stored
//! value. All three are FALSE of `CREATE INDEX`.

/// How the `missing-timestamps` prescription LANDS — spliced byte-identically into every arm of that
/// rule, AHEAD of that arm's imperative (rule-quality.md §27). One constant rather than a sentence per
/// arm because the arm that fired on cal.com's `Payment` shipped no prose at all and so had nowhere to
/// put the warning; a shared constant makes "an arm without the clause" impossible to write by accident.
/// The mechanism is not the schema line: `@updatedAt` supplies no column default, so Prisma emits a
/// bare `NOT NULL` `ADD COLUMN` that Postgres rejects on a populated table, while `createdAt DateTime
/// @default(now())` is NOT in that class — which is why the both-missing arm splits the two names
/// rather than instructing on them together.
pub(super) const MIGRATION_LANDING: &str =
    "Adding this column is a migration, not a one-line schema edit: \
     `@updatedAt` carries no column default, so Prisma emits a bare `ADD COLUMN ... NOT NULL` and \
     `prisma migrate deploy` FAILS against any table that already holds rows. The two shapes that \
     deploy are a nullable column (`updatedAt DateTime? @updatedAt`), or a hand-edited migration \
     that supplies one: `ADD COLUMN \"updatedAt\" TIMESTAMP(3) NOT NULL DEFAULT NOW();`.";

/// How the `implicit-fk` prescription LANDS — spliced AHEAD of the imperative (rule-quality.md §27).
/// Prisma models a relation exactly one way, so a reader who acts on this finding writes
/// `@relation(fields: [...], references: [id])`, and that generates `ALTER TABLE ... ADD CONSTRAINT ...
/// FOREIGN KEY`, which Postgres validates against every row the table already holds. A column that never
/// had a constraint can hold anything: a sentinel written by the application, or an id minted by a system
/// this database does not own.
///
/// Field-independent on purpose — the field name belongs in the imperative, so this constant can be a
/// single spelling the position pin compares against (the sibling `MIGRATION_LANDING` above is the same
/// shape, for the same reason). What it deliberately does NOT describe is the case the GATE removes (a
/// declared literal `@default`): a message must describe the population it still reaches, not the one it
/// no longer has readers in (rule-quality.md §30).
pub(super) const FK_CONSTRAINT_LANDING: &str = "CHECK WHAT THIS COLUMN ALREADY HOLDS FIRST: modeling the \
     relation is a migration, not a one-line schema edit — Prisma emits `ADD CONSTRAINT ... FOREIGN \
     KEY`, and Postgres VALIDATES it against every row already in the table, so `prisma migrate \
     deploy` FAILS MID-DEPLOY on the first value with no parent row. A sentinel (`0`, `-1`, `''`) \
     written by application code, or an id issued by an external system that has no table here, has \
     no parent row by design — and the code writing those values breaks at runtime even if the \
     constraint lands on an empty table.";

/// How the `nullable-fk` prescription LANDS — spliced AHEAD of the imperative (rule-quality.md §27).
///
/// A FOURTH constant rather than a reuse of [`FK_CONSTRAINT_LANDING`], because the two rules prescribe
/// opposite edits against different DDL. `implicit-fk` asks a reader to MODEL a relation, which emits `ADD
/// CONSTRAINT ... FOREIGN KEY` and fails on a row whose value names no parent; this rule asks a reader to
/// decide whether the column should be REQUIRED, which emits `ALTER COLUMN ... SET NOT NULL` and fails on
/// a row holding NULL. Different statement, different failing predicate, and — decisively — the sibling's
/// own exit is "backfill the parentless values to NULL and make the column optional", which is the state
/// this reader is trying to LEAVE. Splicing it here would hand them a sentence recommending the opposite
/// of what they were just told to do.
///
/// §30 bites here the way it did on `implicit-fk`: this rule now has two gates, so the population left to
/// address is a column with a DECLARED `@relation` whose `onDelete` is not `SetNull`. The sentence
/// describes that column and not the ones the gates removed — a name-only `*Id` guess has no reader here
/// any more, and neither does a `SetNull` relation, whose author already answered this question.
///
/// Field- and model-independent for the same reason its siblings are: names belong in the imperative, so
/// the position pin has ONE spelling to compare an index against.
pub(super) const NOT_NULL_LANDING: &str = "CHECK WHETHER THIS COLUMN HOLDS NULLS TODAY FIRST — \
     `SELECT count(*) ... WHERE <column> IS NULL`, against the DEPLOYED database rather than a local \
     one. Answering \"optional was not intentional\" makes this a migration, not a one-line schema edit: \
     dropping the `?` makes Prisma emit `ALTER TABLE ... ALTER COLUMN ... SET NOT NULL`, which Postgres \
     VALIDATES against every row already stored, so `prisma migrate deploy` FAILS MID-DEPLOY on the \
     first NULL it meets.";

/// The EXIT `nullable-fk` publishes after [`NOT_NULL_LANDING`] — both ways out, because for this rule the
/// second one is frequently the right answer and the shipped message ("confirm the optional relation is
/// intentional") ended without naming either.
///
/// The zero-NULL branch is not "just drop the `?`": Prisma's IMPLICIT `onDelete` is `SetNull` for an
/// optional relation and `Restrict` for a required one, so a promotion with no explicit action changes
/// what a parent delete does at runtime, long after the migration succeeded. Measured on cal.com, 21 of
/// the 77 findings this message still reaches declare no `onDelete` at all and are exactly that case.
///
/// The leave-it-optional branch names the shape that makes optional CORRECT rather than merely tolerated —
/// two mutually exclusive parents on one model (`EventType.userId`/`EventType.teamId`), where every row
/// holds NULL in one of the two by construction and no backfill exists to write.
pub(super) const NULLABLE_FK_EXIT: &str = "IF THE COUNT IS ZERO: drop the `?`, and declare the \
     `onDelete` you want in the same edit — the implicit action is `SetNull` for an optional relation \
     and `Restrict` for a required one, so parent deletes that used to null this column start failing \
     instead. IF IT IS NOT ZERO: backfill those rows to a real parent id in a hand-written `UPDATE` \
     placed ahead of the `SET NOT NULL` (`prisma migrate` writes no backfill), or LEAVE IT OPTIONAL — a \
     model naming two mutually exclusive parents holds NULL in one of them on every row by design, and \
     there is nothing to backfill.";

/// How the two "this column is the wrong TYPE" prescriptions LAND — spliced byte-identically into
/// `float-money` and `temporal-as-string`, AHEAD of each rule's imperative (rule-quality.md §27).
///
/// ONE constant for both because the dangerous property is one property and it belongs to the DDL, not
/// to what either rule detects: a declared type change makes `prisma migrate` emit a bare `ALTER COLUMN
/// ... SET DATA TYPE` with no `USING` and no backfill, and Postgres answers it by taking an ACCESS
/// EXCLUSIVE lock and rewriting the whole table. That is identical whether the target type accepts the
/// old values (`Float` -> `Decimal`) or cannot be reached from them at all (`String` -> `DateTime`) —
/// the two rules diverge only in the EXIT, and the exit is each rule's own imperative, the same split
/// [`DATA_LOSS_LANDING`] already uses.
///
/// Deliberately silent on WHICH cast: "any value the cast cannot produce" is true of both, and naming
/// the two cases here would put each rule's other case in front of a reader it does not apply to.
/// Field- and model-independent for the same reason its siblings are — names belong in the imperative,
/// so the position pin has ONE spelling to compare an index against.
pub(super) const COLUMN_TYPE_CHANGE_LANDING: &str = "CHANGING A COLUMN'S TYPE IS A MIGRATION AGAINST \
     THE ROWS THAT ALREADY EXIST, not a one-line schema edit: `prisma migrate` diffs the schema and \
     emits a bare `ALTER TABLE ... ALTER COLUMN ... SET DATA TYPE` — no `USING`, no backfill — and \
     Postgres answers it by taking an ACCESS EXCLUSIVE lock and REWRITING the entire table, so every \
     read and write against it blocks until the rewrite finishes, while any stored value the cast \
     cannot produce fails the statement mid-deploy. `SELECT count(*)` against the DEPLOYED database is \
     what tells you whether that is a moment or an outage.";

/// The EXIT `float-money` publishes after [`COLUMN_TYPE_CHANGE_LANDING`]. The two ways out split on
/// table size, as the landing's `count(*)` says — but NOT on whether the statement can die, which is
/// what this constant claimed until 2026-08-31 ("float to numeric is an implicit cast, so the
/// statement itself cannot fail on a value") and what a release audit refuted twice over:
///
/// - `float8 -> numeric` is an ASSIGNMENT cast in `pg_cast` (`castcontext = 'a'`), not an implicit one.
///   It needs no `USING` only because `ALTER COLUMN ... TYPE` supplies assignment context itself, which
///   is a different reason with a different scope — and it is the fact that separates this exit from
///   [`DATETIME_MIGRATION_EXIT`], where no cast exists at all.
/// - The prescription was the DANGEROUS half. `numeric(p, s)` bounds the value: any stored amount whose
///   integer part exceeds `p - s` digits aborts the whole `ALTER` with `numeric field overflow`, so
///   `@db.Decimal(10, 2)` plus ONE legacy outlier is exactly the mid-deploy statement death
///   [`COLUMN_TYPE_CHANGE_LANDING`] warns about one sentence earlier. Prisma's UNANNOTATED `Decimal`
///   maps to `DECIMAL(65,30)` on Postgres, so the annotation this exit used to prescribe is the narrow
///   choice and the bare type is the wide one. The reader was told to measure `count(*)` and nothing
///   about magnitude, which is the axis that actually decides this.
///
/// So the exit now names the failure it cannot rule out and the query that rules it out, instead of
/// promising it away. It stays silent on `Infinity`/`NaN` (rejected by `numeric` before PG 14): a
/// float8 money column holding one is not a population this rule's reader has, and the magnitude
/// query below is what they do have.
///
/// The last sentence is the part no reader can get from the schema: `Decimal` makes future writes exact
/// and does nothing whatsoever for the values already stored, which were rounded at the moment they were
/// written. A reader who believes the type change repaired them has a ledger that still does not balance
/// and now trusts it.
pub(super) const DECIMAL_MIGRATION_EXIT: &str = "IF THAT COUNT IS SMALL: change the field to `Decimal` \
     and ship the generated migration as-is — float8 to numeric is an ASSIGNMENT cast, which `ALTER \
     COLUMN ... TYPE` takes with no `USING`. IT CAN STILL DIE ON A VALUE, and what decides that is \
     the precision, not the row count: `@db.Decimal(p, s)` caps the column at `p - s` digits ahead of \
     the point and aborts the whole ALTER with `numeric field overflow` on the first stored amount \
     above it, so one legacy outlier ends the deploy. `SELECT max(abs(<column>))` is what picks `p`; \
     a bare `Decimal` is `DECIMAL(65,30)` on Postgres, the WIDE choice rather than the careless one. \
     IF IT IS NOT: add a second `Decimal` field, \
     backfill it in batches, cut writes over to it, and drop the float in a later release, which trades \
     one long lock for a dual-write window. Either way the amounts already stored were rounded when \
     they were WRITTEN and converting the column cannot make them exact again — reconcile the converted \
     values against whatever issued those amounts instead of reading the type change as a repair.";

/// The EXIT `temporal-as-string` publishes after [`COLUMN_TYPE_CHANGE_LANDING`] — the sibling of
/// [`DECIMAL_MIGRATION_EXIT`], and a separate constant rather than a shared template because the two
/// differ in the one fact that decides the whole edit: here the cast the landing describes DOES NOT
/// EXIST. Postgres has no implicit text-to-timestamp conversion, so the generated statement is rejected
/// while it is still being planned, before a single row is read — a failure mode that reaches the reader
/// earlier and more loudly than `float-money`'s, and one that no amount of `count(*)` predicts.
///
/// Second failure behind the first: once a hand-written `USING` makes the statement legal, it runs and
/// aborts on the first value that does not parse. Naming both in order is the point — a reader who fixes
/// only the syntax meets the data failure on the next deploy attempt instead.
pub(super) const DATETIME_MIGRATION_EXIT: &str = "AND FOR THIS PAIR THE CAST DOES NOT EXIST: Postgres \
     has no implicit text-to-timestamp conversion, so the statement Prisma generates is rejected while \
     it is still being planned, before any row is read. Change the field to `DateTime` and hand-edit \
     that migration to carry the conversion (`USING <column>::timestamptz`, or a `to_timestamp(...)` \
     naming the format your strings actually use) — it then runs, and aborts on the FIRST stored value \
     that does not parse, so select for those before you deploy rather than after. IF SOME OF THEM \
     CANNOT BE PARSED AT ALL: add a second `DateTime` field, backfill what parses, and leave the \
     original strings in place for the rest — a cast that drops them is a data loss the migration will \
     not announce.";

/// The whole `float-money` message. A builder here rather than a `format!` in `message.rs` for the
/// reason [`FIELD_RETIREMENT_EXIT`] is a constant here: `message.rs` sits at the 300-line cap, and a body
/// that is one clause of its own plus two of this module's sentences belongs beside them.
pub(super) fn float_money_message(model: &str, field: &str, ty: &str) -> String {
    format!(
        "Model {model} field {field} stores a monetary value as a lossy float type ({ty}) — binary \
         floating point cannot hold most decimal fractions exactly, so the amounts stored and the sums \
         taken over them drift. {COLUMN_TYPE_CHANGE_LANDING} {DECIMAL_MIGRATION_EXIT}"
    )
}

/// The whole `temporal-as-string` message — [`float_money_message`]'s sibling, same reason.
pub(super) fn temporal_as_string_message(model: &str, field: &str) -> String {
    format!(
        "Model {model} field {field} stores a date/time value as String — string ordering is not \
         chronological ordering, and nothing rejects a value that is not a date at all. \
         {COLUMN_TYPE_CHANGE_LANDING} {DATETIME_MIGRATION_EXIT}"
    )
}

/// How the three "take something out of the schema" prescriptions LAND — spliced byte-identically into
/// `god-model`, `unreferenced-field-name` and `unreferenced-model-name`, AHEAD of each rule's imperative
/// (rule-quality.md §27).
///
/// ONE constant for three rules because the dangerous property is one property, and it is not the one
/// the three rules detect: `prisma migrate` diffs the schema and emits DDL, never data movement. Whatever
/// leaves a model here becomes `DROP COLUMN`, and whatever leaves the schema becomes `DROP TABLE`, applied
/// to the rows that exist at deploy time with no backfill and no way back. That is as true of `god-model`,
/// where the DROP is a SIDE EFFECT of moving fields into a new model, as of the two `unreferenced-*` rules,
/// where the DROP is the whole edit — the two intents differ only in the EXIT, and the exit is each rule's
/// own imperative, which is per-rule anyway. Field- and model-independent for the same reason
/// [`FK_CONSTRAINT_LANDING`] is: names belong in the imperative, so the position pins have ONE spelling to
/// compare an index against.
///
/// §30 does not bite here the way it did on `implicit-fk`: none of these three rules has a gate, so the
/// population this sentence must describe is EVERY firing. There is no gated case to leave out, and no
/// reader it fails to reach.
///
/// This is the axis rule-quality.md §34 named and could not close — the two `unreferenced-*` rules already
/// carried ~700 characters of EVIDENCE disclosure ("check the tree actually contains the consuming code
/// before acting") and still said nothing about what the edit costs if the evidence is right. Disclosure
/// about whether the finding is TRUE is not disclosure about what ACTING on it destroys.
pub(super) const DATA_LOSS_LANDING: &str = "CHECK WHAT THIS ALREADY HOLDS FIRST: a migration under \
     `prisma/migrations/` that added it is evidence production has been writing to it, and `SELECT \
     count(*)` against the deployed database is the only answer to how much. Taking something out of a \
     Prisma schema is not a one-line edit — `prisma migrate` diffs the schema and emits DDL only: a field \
     that leaves a model becomes `DROP COLUMN`, a model that leaves the schema becomes `DROP TABLE`, \
     neither is given a backfill, and no `prisma migrate` command puts the values back afterwards.";

/// The EXIT `unreferenced-field-name` publishes after [`DATA_LOSS_LANDING`] — a const rather than an
/// inline string for two reasons: the position pin needs one spelling to find, and `message.rs` is at the
/// 300-line cap, so prose that carries no interpolation belongs beside the clause it follows.
///
/// Three ways out, and following any one of them ends green (rule-quality.md, "a prescription must be
/// followable to a green state"): dump it, stage it, or leave it. The staged form is the one a reader
/// with a rollback window actually wants — a column removed from the schema in the same release that
/// stops writing it cannot be rolled back into, because the rollback restores code that reads a column
/// the migration already dropped.
pub(super) const FIELD_RETIREMENT_EXIT: &str = "IF THE CONSUMING CODE REALLY IS ABSENT: dump the \
     column before you touch the schema (`COPY (SELECT ...) TO ...`, or a `pg_dump` restricted to this \
     table), then retire it over TWO releases — stop writing it now, and remove it from the schema only \
     after a release has shipped with nothing reading it, so a rollback in between still finds its \
     values. If you are not willing to keep that dump, leave the field declared: an unused column costs \
     storage and nothing else, and the values in it cannot be recomputed.";

/// The EXIT `unreferenced-model-name` publishes — [`FIELD_RETIREMENT_EXIT`] at table granularity. Two
/// constants rather than one parameterised sentence because the two differ in more than a noun (`DROP
/// COLUMN` against `DROP TABLE`, values against rows), and a shared template that has to be read twice
/// to see which half applies is how the wrong half ends up in front of a reader.
pub(super) const MODEL_RETIREMENT_EXIT: &str = "IF THE CONSUMING CODE REALLY IS ABSENT: dump the table \
     before you touch the schema (a `pg_dump` restricted to it is enough), then retire it over TWO \
     releases — stop writing to it now, and remove the model only after a release has shipped with \
     nothing reading it, so a rollback in between still finds its rows. If you are not willing to keep \
     that dump, leave the model declared: an unused table costs storage and nothing else, and the rows \
     in it cannot be recomputed.";
