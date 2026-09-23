//! The SILENT axis — the landings and message bodies for the two rules whose prescription costs nothing at
//! deploy time, nothing to the rows at the moment it lands, and produces no error anywhere.
//!
//! A third sibling of `landing.rs` and `index_build.rs` rather than more constants inside either, for the
//! reason `rule-quality.md` §37 gave when it split the availability axis out: a module header is a promise
//! the next author reuses. `landing.rs` promises a prescription that FAILS at deploy (`ADD COLUMN`,
//! `ADD CONSTRAINT`, `SET NOT NULL`, `SET DATA TYPE`) or DESTROYS values through DDL
//! (`DROP COLUMN`/`DROP TABLE`); `index_build.rs` promises a statement that SUCCEEDS while writes wait.
//! Neither describes what is here. Both prescriptions below emit no DDL at all — one hands a column to the
//! ORM's own writer, the other adds a clause to a query — and both are absorbed by the system without a
//! single failure. What they cost is delivered later, to a code path that stops getting what it used to.
//!
//! ⚠ The nearest miss is `landing::DATA_LOSS_LANDING`, which also names destroyed values.
//! Its whole mechanism is "`prisma migrate` diffs the schema and emits DDL only", and that sentence is
//! FALSE of [`ORM_TIMESTAMP_OWNERSHIP_LANDING`]: no migration runs at all there, which is exactly why the
//! loss does not show up in a migration review. Reusing it would send a reader to read the diff, and the
//! diff is where nothing is.
//!
//! Both constants are field- and model-independent on purpose — names belong in the imperative, so the
//! position pins have ONE spelling to compare an index against.

use super::sightline::query_call_site_sightline;

/// How the `stale-updated-at` prescription LANDS. Spliced AHEAD of that rule's imperative
/// (rule-quality.md §27).
///
/// This rule shipped 115 characters, an observation and no prescription at all — the shape
/// `rule-quality.md` §34 named: the implicit imperative (`add @updatedAt`) is perfectly unambiguous and
/// there was nowhere to put a qualifier, so leg 3 of a prescription-breaking finding stood by construction.
///
/// The mechanism is the part a reader cannot get from the schema. `@updatedAt` is not a default and not a
/// constraint; it is a client-side assignment Prisma performs on every update of the row. So a column that
/// some other writer had been filling — an `updated_at` copied in from an upstream system, a sync
/// watermark, a moderation or review timestamp — stops holding that value as ordinary traffic flows over
/// it. And unlike every landing in `landing.rs`, this one has no migration to inspect: adding the
/// attribute to an existing `DateTime` column changes no DDL, so the edit passes a migration review by
/// being absent from it.
pub(super) const ORM_TIMESTAMP_OWNERSHIP_LANDING: &str = "READ WHO WRITES THIS COLUMN TODAY BEFORE YOU \
     HAND IT TO THE ORM: `@updatedAt` is neither a default nor a constraint — it is an assignment Prisma \
     makes on EVERY update of the row, so after this edit the column records the last time Prisma wrote \
     the row and nothing else. If anything else was filling it (a timestamp imported from an upstream \
     system, a sync watermark, a moderation or review time), that value is replaced row by row as ordinary \
     traffic flows, with no error, no migration to review — adding the attribute to an existing `DateTime` \
     column emits no DDL at all — and no way to recompute what was there. Existing rows keep their current \
     value until something updates them, so the damage arrives gradually rather than at deploy.";

/// How the `soft-delete-bypass` prescription LANDS. Spliced AHEAD of that rule's imperative.
///
/// A separate constant from the one above and from anything in `landing.rs`, because the population it
/// warns about is defined by something this rule provably cannot see: it matches four query methods on a
/// model and reads the argument span, and it knows nothing about the function the call sits in. The call
/// sites whose entire purpose is to see deleted rows are therefore reported at exactly the same
/// confidence as the ones that should never see them, and the prescribed clause turns those into a query
/// that returns nothing — with no error at any layer, because "no rows" is a legal answer.
///
/// Deliberately does not claim how common those call sites are: unmeasured, and a message that teaches a
/// reader to doubt true findings costs more than the one it saves (rule-quality.md §30/§32). It states
/// what the rule reads, which is a fact about this code.
pub(super) const DELETED_ROW_READER_LANDING: &str = "CHECK WHAT THIS CALL SITE IS FOR BEFORE YOU FILTER \
     IT, BECAUSE THIS RULE DID NOT: it matched a query method on the model and read the arguments, and it \
     knows nothing about the function around them — so a call that EXISTS to see deleted rows is reported \
     here exactly like one that must never see them. Restore and undelete, a trash or archive listing, an \
     admin or audit view, a uniqueness check that still has to collide with a soft-deleted row, a \
     permanent-delete job, an export or a backfill are all in that first group. Adding the filter there \
     raises nothing: the query simply returns no rows, which is a legal answer, so the restore or the \
     collision check quietly stops working and the failure surfaces somewhere else.";

/// The whole `stale-updated-at` message. A builder here rather than a `format!` in `message.rs` for the
/// reason `landing::float_money_message` is one: `message.rs` sits at the 300-line cap, and a body that is
/// one clause of its own plus this module's sentence belongs beside it.
///
/// The imperative is CONDITIONED rather than merely preceded (§27's third form): the condition is the one
/// question the landing tells the reader to answer, so the two halves are one instruction.
pub(super) fn stale_updated_at_message(model: &str, field: &str) -> String {
    format!(
        "Model {model} field {field} looks like an updatedAt timestamp but lacks @updatedAt — it will not \
         auto-refresh on writes. {ORM_TIMESTAMP_OWNERSHIP_LANDING} IF NOTHING ELSE WRITES {field}: add \
         `@updatedAt` to it. If something does, leave the attribute off and keep setting the column \
         explicitly — a field the ORM owns and a field your code owns cannot be the same field."
    )
}

/// The whole `soft-delete-bypass` message — the sibling builder, same reason.
///
/// The existing disqualifier (a `$use` middleware or `$extends` extension injecting the filter globally,
/// which this lexical check cannot see) keeps its place AHEAD of the landing: whether the finding is true
/// at all is the question a reader answers first, and what acting on it costs is the second
/// (rule-quality.md §38).
pub(super) fn soft_delete_bypass_message(
    model: &str,
    field: &str,
    method: &str,
    disable_tail: &str,
) -> String {
    format!(
        "Model {model} has a soft-delete marker field ({field}) but this {method}() call has no `{field}` \
         filter in its arguments — it may return soft-deleted rows. Note: a Prisma middleware (`$use`) or \
         `$extends` client extension that injects this filter globally is invisible to this static check — \
         if your app relies on one, this rule will false-positive on every call site for the model. {} \
         {DELETED_ROW_READER_LANDING} IF THIS CALL SITE SHOULD ONLY EVER SEE LIVE ROWS: add `{field}: \
         null` (or your app's not-deleted convention) to the `where` clause. To silence the rule instead, \
         disable it {disable_tail} (this rule has no inline suppression marker).",
        query_call_site_sightline()
    )
}
