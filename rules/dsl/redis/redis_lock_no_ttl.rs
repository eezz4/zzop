use super::{assert_disqualifier_summary_precedes_imperative, hits, scan, TempDir};

// --- lock-no-ttl ---

#[test]
fn set_nx_lock_acquire_with_no_ttl_is_flagged() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/jobLockNoTtl.ts",
        "import { redis } from \"./redis\";\nexport async function acquireJobLock() {\n  await redis.set(\"lock:job\", 1, \"NX\");\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "lock-no-ttl");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn setnx_call_with_no_ttl_is_flagged() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/jobLockSetnx.ts",
        "import { redis } from \"./redis\";\nexport async function acquireJobLock() {\n  await redis.setnx(\"lock:job\", 1);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "lock-no-ttl");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

// FP-adversarial NEGATIVE (pin): the same `SET ... NX` acquire, but with an `EX` expiry on the same call —
// the `exclude_pattern` vetoes it since the lock self-clears.
#[test]
fn set_nx_lock_acquire_with_ex_ttl_is_not_flagged() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/jobLockWithTtl.ts",
        "import { redis } from \"./redis\";\nexport async function acquireJobLock() {\n  await redis.set(\"lock:job\", 1, \"NX\", \"EX\", 30);\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "lock-no-ttl").is_empty(), "{:?}", out.findings);
}

#[test]
fn lock_ttl_ok_marker_above_the_call_suppresses_the_finding() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/jobLockSuppressed.ts",
        "import { redis } from \"./redis\";\nexport async function acquireJobLock() {\n  // zzop-lock-no-ttl-ok: TTL applied via a separate PEXPIRE call right after, in a wrapper not on this line\n  await redis.set(\"lock:job\", 1, \"NX\");\n}\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "lock-no-ttl").is_empty(), "{:?}", out.findings);
}

/// §27 pin (2026-08-27). This rule's remedy destroyed the property the rule exists to protect, and
/// with a worked example: it read "Always pair `NX` with a TTL (e.g. `SET key value NX EX 30`)".
///
/// A TTL is a deadline on the critical section, not a cleanup knob. Set below the duration of the work
/// it guards — and `30` is a number the message supplied with no knowledge of that work — the key
/// expires while the first holder is still running, a second caller acquires the same lock, and the
/// mutual exclusion this finding was raised to protect is gone. It fails silently on top of that,
/// because the first holder's unconditional `DEL` on the way out then removes the SECOND holder's lock.
/// That is the same class its pack-mate `lock-get-then-set` was repaired for on 2026-08-26: a redis
/// remedy that hands the reader a race.
///
/// The literal `EX 30` is gone rather than reworded — a concrete number is what a reader copies. Both
/// exits are now named, because "do not guess" is not a remedy: a bounded worst case sets the TTL from
/// that worst case, and an unbounded one renews a lease while the holder runs. The fenced release
/// (compare-and-delete against a token only this holder knows) closes the silent half.
///
/// Detection is untouched — the assertions on count and line below are the pre-edit ones, and the `EX`
/// veto test above still proves the matcher reads `EX` exactly as it did.
#[test]
fn lock_no_ttl_message_lands_the_short_ttl_race_before_the_ttl_imperative() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/jobLockNoTtl.ts",
        "import { redis } from \"./redis\";\nexport async function acquireJobLock() {\n  await redis.set(\"lock:job\", 1, \"NX\");\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "lock-no-ttl");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
    let msg = &h[0].message;
    assert!(
        !msg.contains("NX EX 30"),
        "lock-no-ttl: the copyable literal TTL is back. A number chosen without knowing the critical \
         section is the defect this pin exists for: {msg}"
    );
    assert_disqualifier_summary_precedes_imperative(
        "lock-no-ttl",
        msg,
        "THE TTL IS A DEADLINE ON THE WORK THIS LOCK GUARDS",
        "Pair `NX` with a TTL taken from the worst case",
        "deletes the SECOND holder's lock",
    );
    for needle in [
        "two of them run at once",
        "lease/watchdog",
        "compare-and-delete",
    ] {
        assert!(
            msg.contains(needle),
            "lock-no-ttl: the landing lost {needle:?}: {msg}"
        );
    }
}
