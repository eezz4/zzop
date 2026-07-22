use super::{hits, scan, TempDir};

// --- redis-lock-no-ttl ---

#[test]
fn set_nx_lock_acquire_with_no_ttl_is_flagged() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/jobLockNoTtl.ts",
        "import { redis } from \"./redis\";\nexport async function acquireJobLock() {\n  await redis.set(\"lock:job\", 1, \"NX\");\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "redis-lock-no-ttl");
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
    let h = hits(&out, "redis-lock-no-ttl");
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
    assert!(
        hits(&out, "redis-lock-no-ttl").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn lock_ttl_ok_marker_above_the_call_suppresses_the_finding() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/jobLockSuppressed.ts",
        "import { redis } from \"./redis\";\nexport async function acquireJobLock() {\n  // lock-ttl-ok: TTL applied via a separate PEXPIRE call right after, in a wrapper not on this line\n  await redis.set(\"lock:job\", 1, \"NX\");\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "redis-lock-no-ttl").is_empty(),
        "{:?}",
        out.findings
    );
}
