//! §37 LANDING for `redis/lock-no-ttl` — adding the TTL the finding asks for SUCCEEDS, and a TTL
//! shorter than the work it guards recreates the very failure the rule exists to prevent.
//!
//! WHY (`1.architecture/rules/rule-quality.md` §27 leg 3). A TTL is a deadline on the critical section,
//! not a cleanup knob. Set below the duration of the work, the key expires while the first holder is
//! still inside, a second caller acquires the same lock, and two of them run at once — silently, because
//! the first holder's unconditional `DEL` on the way out then deletes the SECOND holder's lock. The
//! message's exits follow from that: take the number from the worst case rather than a round figure,
//! renew from the holder (a lease/watchdog) where the worst case is unbounded, and release with a
//! compare-and-delete against a token only this holder knows.
//!
//! WHY IT TAKES ITS OWN CONSTANT (§37's most-dangerous-reuse test, and the nearest sibling is in this
//! same pack). `INCR_TTL_LOSS_LANDING` on `counter-get-set` is also a sentence about a TTL, and every
//! word of it is false here: that one says `INCR` does not CARRY the expiry a `SET` did and creates a
//! key with a TTL of -1, so the window stops resetting. This one is about a TTL that exists and is TOO
//! SHORT, where the damage is two concurrent holders rather than a window that never closes. Same noun
//! in the vocabulary, opposite failure.
//!
//! THE OTHER AXIS IS ALREADY PINNED AND STAYS THERE (§38). This rule's §27-a pin in
//! `redis_lock_no_ttl.rs` plants a fragment of this same sentence in front of the verb; the pin below is
//! a separate `#[test]` so the landing claim and the disqualifier claim stay separately readable, and so
//! the two helper families are counted on the axes they each speak to.
//!
//! POSITION, not presence. The invalidation probe is to move this constant to the tail of the message:
//! every token stays present and spelled exactly once, and the pin must go red on ORDER alone.

use super::{assert_landing_precedes_imperative, hits, scan, TempDir};

/// The lock-TTL-deadline landing, spliced ahead of the "pair `NX` with a TTL" imperative.
const LOCK_TTL_DEADLINE_LANDING: &str = "THE TTL IS A DEADLINE ON THE WORK THIS LOCK GUARDS, AND ONE SET SHORTER THAN THAT WORK RECREATES THE FAILURE THIS RULE EXISTS TO PREVENT: the key expires while the first holder is still inside the critical section, a second caller acquires the same lock, and two of them run at once — silently, because the first holder's unconditional `DEL` on the way out then deletes the SECOND holder's lock.";

/// One constant, one carrier — asserted against the shipped pack so the pack-mate that already
/// prescribes a separate `EXPIRE` cannot acquire this sentence as a second spelling.
#[test]
fn the_lock_ttl_deadline_landing_is_carried_by_exactly_one_rule() {
    let pack: serde_json::Value =
        serde_json::from_str(include_str!("redis.json")).expect("redis.json parses");
    let mut carriers: Vec<&str> = pack["rules"]
        .as_array()
        .expect("rules array")
        .iter()
        .filter(|r| {
            r["message"]
                .as_str()
                .is_some_and(|m| m.contains(LOCK_TTL_DEADLINE_LANDING))
        })
        .map(|r| r["id"].as_str().expect("rule id"))
        .collect();
    carriers.sort_unstable();
    assert_eq!(
        carriers,
        ["lock-no-ttl"],
        "the lock-TTL-deadline landing is carried by a different rule set than the one it was written for"
    );
}

/// The position pin, on a DELIVERED finding. Separate `#[test]` from this rule's §27-a pin in
/// `redis_lock_no_ttl.rs`, which plants a 50-byte fragment of this sentence as its summary.
#[test]
fn lock_no_ttl_landing_precedes_the_imperative() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/jobLockNoTtl.ts",
        "import { redis } from \"./redis\";\nexport async function acquireJobLock() {\n  await redis.set(\"lock:job\", 1, \"NX\");\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "lock-no-ttl");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "lock-no-ttl",
        &h[0].message,
        LOCK_TTL_DEADLINE_LANDING,
        "Pair `NX` with a TTL taken from the worst case",
    );
}
