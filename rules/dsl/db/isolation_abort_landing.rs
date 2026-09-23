//! §37 LANDING for `db/money-tx-no-isolation-level` — raising the isolation level SUCCEEDS, and what
//! it costs is that the second writer stops waiting and starts failing.
//!
//! WHY THIS IS A LANDING AND NOT A DISQUALIFIER (`1.architecture/rules/rule-quality.md` §37). Nothing
//! about the edit is wrong: `Serializable`/`RepeatableRead` do exactly what the finding asks for. But
//! the two levels do not serialize by making the loser WAIT — they abort it with serialization failure
//! `40001`, surfaced by Prisma as `P2034`, and Prisma does not retry. So a payment handler that was
//! quietly racing starts throwing under concurrency, and a webhook sender that sees the 500 re-delivers
//! against a transaction that may already have committed. The reader who applies the remedy without a
//! bounded retry has traded a silent lost update for a loud double charge.
//!
//! WHY IT TAKES A NEW CONSTANT rather than a sibling's (§37's most-dangerous-reuse test). The nearest
//! spellings in this tree are about a lock being HELD (a window in which other writers queue) or a TTL
//! expiring; this one is about a transaction being KILLED. Nothing that describes waiting is true of an
//! abort, and the whole point of the sentence is that the reader expects waiting.
//!
//! THE OTHER AXIS IS ALREADY PINNED AND STAYS THERE. This rule's own §27-a verdict — "this is a
//! co-occurrence heuristic, not proof the write actually touches the money field" — lives in
//! `transactions.rs` on the disqualifier helper. The two claims are independent (§38): one says when
//! the report may be wrong, this one says what the correct edit costs when it is right.
//!
//! POSITION, not presence. The invalidation probe is to move this constant to the tail of the message:
//! every token stays present and spelled exactly once, and the pin must go red on ORDER alone.

use super::*;

/// The isolation-abort landing, spliced ahead of the "set an explicit isolation level" imperative.
const ISOLATION_ABORT_LANDING: &str = "RAISING THE LEVEL IS NOT FREE, AND THE COST LANDS ON THIS EXACT PATH: `Serializable`/`RepeatableRead` do not make the second writer wait, they ABORT one of the two with serialization failure `40001`, which Prisma surfaces as `P2034` and does NOT retry — so a payment handler that was quietly racing starts throwing under concurrency instead, and a webhook sender that sees the 500 re-delivers against a transaction that may already have committed.";

/// The landing is one constant, so the thing that can silently rot is a COPY that drifts. This asserts
/// the shipped pack carries it in this rule and NOWHERE else — a sibling that acquires an isolation
/// remedy later and paraphrases the cost fails here rather than shipping a second spelling.
#[test]
fn the_isolation_abort_landing_is_carried_by_exactly_one_rule() {
    let pack: serde_json::Value =
        serde_json::from_str(include_str!("db.json")).expect("db.json parses");
    let mut carriers: Vec<&str> = pack["rules"]
        .as_array()
        .expect("rules array")
        .iter()
        .filter(|r| {
            r["message"]
                .as_str()
                .is_some_and(|m| m.contains(ISOLATION_ABORT_LANDING))
        })
        .map(|r| r["id"].as_str().expect("rule id"))
        .collect();
    carriers.sort_unstable();
    assert_eq!(
        carriers,
        ["money-tx-no-isolation-level"],
        "the isolation-abort landing is carried by a different rule set than the one it was written for"
    );
}

/// The position pin, on a DELIVERED finding. Separate `#[test]` from this rule's §27-a pin in
/// `transactions.rs` on purpose: two claims, two functions, each readable on its own.
#[test]
fn money_tx_isolation_landing_precedes_the_imperative() {
    let dir = TempDir::new("zzop-db");
    dir.write(
        "src/service.ts",
        "declare const prisma: any;\ndeclare const amt: number;\nexport async function transferFunds(accountId: string) {\n  await prisma.$transaction(async (tx: any) => {\n    const acct = await tx.account.findUnique({ where: { id: accountId } });\n    await tx.account.update({ where: { id: accountId }, data: { balance: acct.balance - amt } });\n  });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "money-tx-no-isolation-level");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "money-tx-no-isolation-level",
        &h[0].message,
        ISOLATION_ABORT_LANDING,
        "Set an explicit isolation level TOGETHER WITH a bounded retry",
    );
}
