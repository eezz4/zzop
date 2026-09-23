//! §27/§37 LANDING for `db/write-in-loop-no-tx` — the remedy has two branches and the wrong one
//! re-sends work that has already left the process.
//!
//! WHY (`1.architecture/rules/rule-quality.md` §27 leg 3, third form). This message conditions its own
//! imperative rather than moving a clause behind it: the reader is told, before the verb, that the
//! remedy is conditional and what the wrong branch does. Where the loop body only touches the database,
//! wrapping it in `$transaction(...)` is right. Where it also sends mail, charges a card or publishes to
//! a queue, wrapping it is what breaks the code — the rollback deletes the record while the side effect
//! itself is already gone, and a cron that sends on one line and records on the next re-sends its whole
//! backlog on the following run. The per-iteration autocommit that the finding reports is, in that
//! branch, the correct at-least-once ordering.
//!
//! WHY THE HEADLINE SENTENCE IS THE CONSTANT and not the paragraph under it. The paragraph is what the
//! reader needs, but the paragraph sits on BOTH sides of the verb, so a constant cut to hold it would
//! contain the imperative it is supposed to precede and the position pin would be satisfied by
//! construction. The headline is the part that stands in front of the verb and it is a complete cost
//! claim on its own. The paragraph's own needles are pinned separately in `transactions.rs`, which is
//! also where this rule's inline §27-a order comparison lives — that comparison is why the pin below is
//! a SEPARATE `#[test]`: the coverage scanner does not count an inline comparison in a block that
//! also calls a shared helper. That used to make merging the two functions a silent deletion of this
//! rule's axis-A verdict; `delivered_pins` now refuses the shape, so the merge is a red test instead.
//!
//! WHY IT TAKES ITS OWN CONSTANT. The nearest sibling, `db/multi-write-no-tx`, has the same two-branch
//! shape and a different failure: there the transaction is held ACROSS an external round trip and dies
//! on the ORM's own time ceiling, rolling back a write that would otherwise have committed. Nothing
//! about replaying an already-delivered side effect is true of that, and the two messages spell their
//! conditions differently today, so there is no byte-identical sentence to share.
//!
//! POSITION, not presence. The invalidation probe is to move this constant to the tail of the message:
//! every token stays present and spelled exactly once, and the pin must go red on ORDER alone.

use super::*;

/// The autocommit-replay landing, spliced ahead of the `$transaction(...)` imperative.
///
/// `#[rustfmt::skip]` is LOAD-BEARING and not a style preference. The axis-B registry in
/// `message_order_verdicts.rs` reads a landing only from a BARE single-line
/// `const NAME: &str = "...";`, and this is the first landing in the tree short enough for rustfmt to
/// break after the `=` (4 + 92 + 2 fits in 100; every other landing's literal does not, which is why
/// none of them has needed this). Measured 2026-09-01: without the attribute, `cargo fmt --all` wrapped
/// this declaration, the constant left the registry, and `write-in-loop-no-tx` reported as a rule whose
/// landing pin names no registered constant. The guard caught it — but the same wrap on a rule with no
/// pin would have made a carrier read as a non-carrier and gone quiet.
#[rustfmt::skip]
const AUTOCOMMIT_REPLAY_LANDING: &str = "THE REMEDY IS CONDITIONAL, AND THE WRONG BRANCH RE-SENDS WORK THAT ALREADY LEFT THE PROCESS.";

/// One constant, one carrier — asserted against the shipped pack so a sibling that grows a conditional
/// remedy and pastes this sentence fails here rather than shipping a second spelling.
#[test]
fn the_autocommit_replay_landing_is_carried_by_exactly_one_rule() {
    let pack: serde_json::Value =
        serde_json::from_str(include_str!("db.json")).expect("db.json parses");
    let mut carriers: Vec<&str> = pack["rules"]
        .as_array()
        .expect("rules array")
        .iter()
        .filter(|r| {
            r["message"]
                .as_str()
                .is_some_and(|m| m.contains(AUTOCOMMIT_REPLAY_LANDING))
        })
        .map(|r| r["id"].as_str().expect("rule id"))
        .collect();
    carriers.sort_unstable();
    assert_eq!(
        carriers,
        ["write-in-loop-no-tx"],
        "the autocommit-replay landing is carried by a different rule set than the one it was written for"
    );
}

/// The position pin, on a DELIVERED finding. It MUST stay out of the `transactions.rs` block that
/// carries this rule's §27-a claim: that block proves its order with an inline offset comparison, and
/// the coverage scanner does not count an inline comparison in a function that also calls a shared
/// helper. `delivered_pins` asserts against that combination, so merging the two fails the guard by
/// name rather than deleting this rule's axis-A verdict quietly.
#[test]
fn write_in_loop_autocommit_landing_precedes_the_imperative() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare const users: { id: string }[];\nexport async function activateAll() {\n  for (const u of users) {\n    await prisma.account.update({ where: { id: u.id }, data: { active: true } });\n  }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "write-in-loop-no-tx");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "write-in-loop-no-tx",
        &h[0].message,
        AUTOCOMMIT_REPLAY_LANDING,
        "wrap the loop body in `$transaction(...)`",
    );
}
