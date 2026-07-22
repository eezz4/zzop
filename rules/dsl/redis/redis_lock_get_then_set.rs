use super::{hits, scan, TempDir};

// --- redis-lock-get-then-set ---

#[test]
fn check_then_set_lock_is_flagged() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/jobLock.ts",
        "import { redis } from \"./redis\";\nexport async function runJob() {\n  if (!(await redis.get(\"lock:job\"))) {\n    await redis.set(\"lock:job\", 1);\n  }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "redis-lock-get-then-set");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
}

#[test]
fn exists_then_set_lock_is_flagged() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/jobLock2.ts",
        "import { redis } from \"./redis\";\nexport async function runJob() {\n  const held = await redis.exists(\"lock:job\");\n  if (!held) {\n    await redis.set(\"lock:job\", 1);\n  }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "redis-lock-get-then-set");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 5);
}

// FP-adversarial NEGATIVE 1 (pin): a cache-aside read-through with no `lock` token anywhere in the
// function — the `lockish` co-occurrence pattern never matches, so this ordinary cache pattern stays silent.
#[test]
fn cache_aside_read_through_with_no_lock_token_is_not_flagged() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/cache.ts",
        "import { redis } from \"./redis\";\ndeclare function loadFromDb(key: string): Promise<unknown>;\nexport async function getCached(key: string) {\n  const v = await redis.get(key);\n  if (!v) {\n    const data = await loadFromDb(key);\n    await redis.set(key, JSON.stringify(data));\n    return data;\n  }\n  return JSON.parse(v as string);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "redis-lock-get-then-set").is_empty(),
        "{:?}",
        out.findings
    );
}

// FP-adversarial NEGATIVE 2 (pin): the acquire is already atomic (`SET ... NX`) — the `atomic` absent-veto
// pattern matches, so the non-atomic-lock heuristic correctly stays silent.
#[test]
fn set_nx_atomic_lock_acquire_is_not_flagged() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/jobLockAtomic.ts",
        "import { redis } from \"./redis\";\nexport async function runJob() {\n  const acquired = await redis.get(\"lock:job\");\n  if (!acquired) {\n    await redis.set(\"lock:job\", 1, \"NX\");\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "redis-lock-get-then-set").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn redis_lock_atomic_ok_marker_above_the_set_call_suppresses_the_finding() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/jobLockSuppressed.ts",
        "import { redis } from \"./redis\";\nexport async function runJob() {\n  if (!(await redis.get(\"lock:job\"))) {\n    // redis-lock-atomic-ok: acquire is delegated to a vetted redlock wrapper not visible to regex\n    await redis.set(\"lock:job\", 1);\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "redis-lock-get-then-set").is_empty(),
        "{:?}",
        out.findings
    );
}

// Regression (opus review F3): the `lockish` token was the bare substring `(?i)lock`, which matches
// inside `block`/`blocklist`/`clock`/`deadlock`/`unlock`. A plain blocklist cache get/set is not a lock
// TOCTOU. Now anchored `(?i)\block` (word-boundary before), so `blocklist` no longer satisfies it.
#[test]
fn blocklist_cache_get_then_set_is_not_a_lock_and_is_not_flagged() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/blocklist.ts",
        "import { redis } from \"./redis\";\ndeclare function computeNext(cur: unknown): string;\nexport async function refreshBlocklist() {\n  const cur = await redis.get(\"blocklist\");\n  await redis.set(\"blocklist\", computeNext(cur));\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "redis-lock-get-then-set").is_empty(),
        "{:?}",
        out.findings
    );
}
