use super::{hits, scan, TempDir};

// --- redis-counter-get-set ---

#[test]
fn read_then_increment_and_set_counter_is_flagged() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/hitCounter.ts",
        "import { redis } from \"./redis\";\nexport async function bumpHitCount(key: string) {\n  const n = await redis.get(key);\n  await redis.set(key, Number(n) + 1);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "redis-counter-get-set");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
}

#[test]
fn read_then_parseint_plus_one_set_counter_is_flagged() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/viewCounter.ts",
        "import { redis } from \"./redis\";\nexport async function bumpViewCount(key: string) {\n  const raw = await redis.get(key);\n  await redis.set(key, parseInt(raw as string) + 1);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "redis-counter-get-set");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 4);
}

// FP-adversarial NEGATIVE (pin): `read` and `arith-set` BOTH still match in this span (a shadow/debug
// counter mirrored via read-then-arithmetic-set), but the function also calls atomic `.incr(` for the real
// counter — the `atomic` absent-veto pattern matches anywhere in the same span, so the whole finding is
// vetoed. This pins that `incr` genuinely suppresses via the `absent` list, not merely because `arith-set`
// never matched.
#[test]
fn arith_set_shape_alongside_a_real_incr_call_is_vetoed_by_absent() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/hitCounterAtomic.ts",
        "import { redis } from \"./redis\";\nexport async function bumpHitCount(key: string) {\n  const n = await redis.get(key);\n  await redis.set(\"debug:\" + key, Number(n) + 1);\n  await redis.incr(key);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "redis-counter-get-set").is_empty(),
        "{:?}",
        out.findings
    );
}

// FP-adversarial NEGATIVE (pin), literal shape from the rule spec: the counter is bumped with atomic
// `.incr(` alone, no read-modify-write `.set(` in sight at all — `arith-set` never matches, so there is no
// candidate finding in the first place.
#[test]
fn plain_incr_with_no_arithmetic_set_at_all_is_not_flagged() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/hitCounterPlainIncr.ts",
        "import { redis } from \"./redis\";\nexport async function bumpHitCount(key: string) {\n  await redis.incr(key);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "redis-counter-get-set").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn redis_counter_ok_marker_above_the_set_call_suppresses_the_finding() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/hitCounterSuppressed.ts",
        "import { redis } from \"./redis\";\nexport async function bumpHitCount(key: string) {\n  const n = await redis.get(key);\n  // redis-counter-ok: single-writer cron job, no concurrent access possible\n  await redis.set(key, Number(n) + 1);\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "redis-counter-get-set").is_empty(),
        "{:?}",
        out.findings
    );
}

// Regression (opus review F2): the `arith-set` pattern's `[+\-]\s*1` matched the `-1` inside a string
// key literal like `'user-1'` on an un-masked line. `strip_string_literals: true` now masks string
// interiors before matching, so a plain hyphen-suffixed-key cache set is not read as a counter update.
#[test]
fn hyphen_suffixed_string_key_cache_set_is_not_a_counter_and_is_not_flagged() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/userCache.ts",
        "import { redis } from \"./redis\";\ndeclare function serialize(v: unknown): string;\nexport async function cacheUser(v: unknown) {\n  const prev = await redis.get(\"user-1\");\n  await redis.set(\"user-1\", serialize(v));\n  return prev;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "redis-counter-get-set").is_empty(),
        "{:?}",
        out.findings
    );
}
