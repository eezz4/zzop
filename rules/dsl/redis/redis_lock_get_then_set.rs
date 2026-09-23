use super::{
    assert_disqualifier_clause_precedes_imperative,
    assert_disqualifier_summary_precedes_imperative, hits, scan, TempDir,
};

// --- lock-get-then-set ---

#[test]
fn check_then_set_lock_is_flagged() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/jobLock.ts",
        "import { redis } from \"./redis\";\nexport async function runJob() {\n  if (!(await redis.get(\"lock:job\"))) {\n    await redis.set(\"lock:job\", 1);\n  }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "lock-get-then-set");
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
    let h = hits(&out, "lock-get-then-set");
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
        hits(&out, "lock-get-then-set").is_empty(),
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
        hits(&out, "lock-get-then-set").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn redis_lock_atomic_ok_marker_above_the_set_call_suppresses_the_finding() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/jobLockSuppressed.ts",
        "import { redis } from \"./redis\";\nexport async function runJob() {\n  if (!(await redis.get(\"lock:job\"))) {\n    // zzop-lock-get-then-set-ok: acquire is delegated to a vetted redlock wrapper not visible to regex\n    await redis.set(\"lock:job\", 1);\n  }\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "lock-get-then-set").is_empty(),
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
        hits(&out, "lock-get-then-set").is_empty(),
        "{:?}",
        out.findings
    );
}

// ORDER-GATE pin: seals the `after: read` gate — a `.set(` that PRECEDES the only read is a lock
// RELEASE-then-verify, not the "get then set" acquire race the id names. Before the gate this fired.
#[test]
fn a_set_that_precedes_the_only_read_is_not_flagged() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/release.ts",
        "import { redis } from \"./redis\";\nexport async function releaseLock() {\n  await redis.set(\"lock:job\", 0);\n  const held = await redis.get(\"lock:job\");\n  return held;\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "lock-get-then-set").is_empty(),
        "{:?}",
        out.findings
    );
}

// --- the MUTUALLY DESTRUCTIVE prescription this rule used to deliver on a counter's line (2026-08-26) ---
//
// An outside auditor read cal.com's `packages/features/api-keys-legacy/api-keys/lib/autoLock.ts:80` and got
// BOTH `redis/counter-get-set` and this rule on that ONE line, with opposite remedies: `INCR` from the
// sibling, `SET key value NX` from here. The line is
// `await redis.set(lockKey, (currentCount + 1).toString())` under a key literally spelled `...".count"`,
// read back at :61-62 through `parseInt` and compared at :69 against a threshold of 5. It is a counter.
// Following `NX` there makes every write after the first a no-op, the stored value freezes at 1,
// `currentCount + 1 >= autolockThreshold` is false forever, and the rate-limit auto-lock is silently off —
// node-redis returns `null` from the refused `SET` and :80 discards it, so nothing errors and nothing logs.
//
// TWO repairs, and the second is the one that survives if the first is ever narrowed:
//   (a) a per-trigger veto (`trigger_call_exclude_pattern`) whose pattern is BYTE-IDENTICAL to the sibling's
//       `arith-set` trigger, so this rule declines exactly the `.set(` calls the sibling anchors on. It is
//       NOT an `absent` entry: `absent` is scoped to the whole body span and would silence a genuine mutex
//       acquire merely because a counter is bumped somewhere else in the same function — pinned below.
//   (b) the clause, because the veto only reads THIS call's own parentheses: a value computed into a
//       variable one line earlier is out of its reach, and the sibling cannot see that shape either, so the
//       reader arrives here and needs the counter-indication ahead of the imperative (rule-quality.md 27).

// (a) — the cal.com shape, reduced. Anti-vacuity: the silence is only correct because the SIBLING reports
// the same line, so this asserts the handoff rather than the absence.
#[test]
fn an_arithmetic_set_is_declined_here_and_reported_by_the_counter_sibling() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/autoLock.ts",
        "import { redis } from \"./redis\";\nexport async function handleAutoLock(identifier: string) {\n  const lockKey = `autolock:${identifier}.count`;\n  const count = await redis.get(lockKey);\n  const currentCount = count ? parseInt(count.toString(), 10) : 0;\n  await redis.set(lockKey, (currentCount + 1).toString());\n}\n",
    );
    let out = scan(&dir);
    assert!(
        hits(&out, "lock-get-then-set").is_empty(),
        "the NX prescription reached a counter's line: {:?}",
        out.findings
    );
    let sibling = hits(&out, "counter-get-set");
    assert_eq!(
        sibling.len(),
        1,
        "the veto is only sound while the sibling covers what it declines — coverage went to zero. {:?}",
        out.findings
    );
    assert_eq!(sibling[0].line, 6);
}

// (a), the scope pin that says why this is not an `absent` entry: the veto reads ONE call's parentheses, so
// a counter bump does not disarm the rule for the rest of the function. The mutex acquire two lines down
// still fires, and becomes the anchor because a vetoed trigger is not a hit at all.
//
// The acquired value is spelled `"owner-1"` ON PURPOSE, and that is the second thing this pin holds. The
// veto reads UNMASKED source — this rule cannot set `strip_string_literals`, since its own `lockish`
// pattern has to see quoted key literals — so a veto pattern copied BYTE-FOR-BYTE from the sibling's
// `arith-set` trigger reads the `-1` inside that string as arithmetic and silently drops a real lock
// finding. Measured: with the copied pattern this test reported 0 findings where it now reports 1. The
// shipped pattern is a strict TIGHTENING of the sibling's instead (same line only, and the arithmetic must
// follow a comma with no quote between), so everything it declines the sibling still matches.
#[test]
fn a_counter_bump_elsewhere_in_the_function_does_not_silence_a_real_mutex_acquire() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/acquire.ts",
        "import { redis } from \"./redis\";\nexport async function acquire(job: string) {\n  const lockKey = `lock:${job}`;\n  const held = await redis.get(lockKey);\n  const tries = await redis.get(`${lockKey}.count`);\n  await redis.set(`${lockKey}.count`, (Number(tries) + 1).toString());\n  if (!held) {\n    await redis.set(lockKey, \"owner-1\");\n  }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "lock-get-then-set");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(
        h[0].line, 8,
        "the anchor must be the MUTEX write, not the counter bump at line 6: {:?}",
        out.findings
    );
    let sibling = hits(&out, "counter-get-set");
    assert_eq!(sibling.len(), 1, "{:?}", out.findings);
    assert_eq!(sibling[0].line, 6);
}

// (b) — the population the veto CANNOT reach, which is the population this message is written for
// (rule-quality.md 30): the arithmetic sits on an EARLIER line and only a variable is passed in, so the
// sibling is silent too and this rule is the reader's only warning.
#[test]
fn a_value_computed_one_line_earlier_is_out_of_the_vetos_reach_and_carries_the_clause() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/autoLockVar.ts",
        "import { redis } from \"./redis\";\nexport async function bump(identifier: string) {\n  const lockKey = `autolock:${identifier}.count`;\n  const count = await redis.get(lockKey);\n  const next = parseInt(String(count), 10) + 1;\n  await redis.set(lockKey, String(next));\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "lock-get-then-set");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 6);
    assert!(
        hits(&out, "counter-get-set").is_empty(),
        "if the sibling ever grows to see this shape, the clause below can be reconsidered — until then \
         this rule is the only warning the reader gets. {:?}",
        out.findings
    );

    let m = &h[0].message;
    for needle in [
        // what the reader must recognise BEFORE acting
        "THIS KEY IS A COUNTER AND NOT A LOCK",
        // the mechanism, and why it is silent rather than loud
        "no-op",
        "returns `null` and this line discards it",
        // the third leg 27 demands: following the caveat has to end GREEN, so name the replacement
        "`INCR`/`INCRBY`/`HINCRBY`",
        // and the handoff, so a reader who sees both rules on one line knows which one owns the line
        "redis/counter-get-set",
    ] {
        assert!(
            m.contains(needle),
            "lock-get-then-set's remedy no longer carries its counter-indication — missing {needle:?}. \
             A reader whose key is a counter follows `SET ... NX` and freezes the value at the first \
             write, turning a rate-limit threshold off with no error and no log. In: {m}"
        );
    }

    // INVALIDATION PROBE: move the clause behind the imperative with every token above still present —
    // every `contains` assertion here stays green and this call goes red.
    assert_disqualifier_summary_precedes_imperative(
        "lock-get-then-set",
        m,
        "THIS KEY IS A COUNTER AND NOT A LOCK",
        "acquire the lock atomically instead with `SET key value NX`",
        "`NX` IS THE WRONG FIX",
    );
}

// --- leg 3 on the COUNTER arm: the two-call prescription now states its own cost (2026-09-11) ---
//
// rule-quality.md 27 leg 3 asks whether a message states the COST of the edit it prescribes. On the
// counter arm this rule said "Increment it atomically instead — `INCR`/`INCRBY`/`HINCRBY` ... and keep
// the window with a separate `EXPIRE` on the same key", which is the next STEP of the edit and not its
// cost. `MULTI`, Lua and `EX`/`PX` were all present in the message and all of them sat inside the
// "WHEN THE KEY REALLY IS A MUTEX" arm — measured 2026-09-11, the two-call sentence at byte 1938 and
// that arm's heading at byte 3406 — so a reader on the counter arm never reached them.
//
// The batch that declined to make this rule a second carrier recorded "the sibling already warns", and
// the sibling does: `redis/counter-get-set` spells the interrupted-window failure out. But the sibling
// is SILENT on the population this arm exists for. Its trigger requires the arithmetic to sit inside
// the `.set(` call's own parentheses, so a value computed on an earlier line
// (`const next = parseInt(count) + 1`) reaches this rule alone —
// `a_value_computed_one_line_earlier_is_out_of_the_vetos_reach_and_carries_the_clause` above pins
// exactly that, asserting the sibling reports NOTHING there. For that reader this message is the only
// warning, and until now it prescribed two calls without saying what happens between them.

/// Same fixture population as the pin above (the sibling is silent; this rule is the only warning), and
/// it asserts the COST rather than the next step — a TTL of `-1`, what that does to a lockout window,
/// and the two guarded spellings that close the gap.
///
/// INVALIDATION: delete the `THAT IS TWO ROUND TRIPS` sentence from the message and this goes red while
/// every other pin in this file stays green — run it before believing the green.
#[test]
fn the_counter_arm_states_the_cost_of_its_own_two_call_prescription() {
    let dir = TempDir::new("zzop-redis");
    dir.write(
        "src/autoLockCost.ts",
        "import { redis } from \"./redis\";\nexport async function bumpCost(identifier: string) {\n  const lockKey = `autolock:${identifier}.count`;\n  const count = await redis.get(lockKey);\n  const next = parseInt(String(count), 10) + 1;\n  await redis.set(lockKey, String(next));\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "lock-get-then-set");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert!(
        hits(&out, "counter-get-set").is_empty(),
        "the sibling must still be silent here — the whole point of this pin is that this message is \
         the reader's only warning on this shape. {:?}",
        out.findings
    );
    let m = &h[0].message;

    for needle in [
        // the cost, named as a cost
        "THAT IS TWO ROUND TRIPS AND THE GAP BETWEEN THEM IS THE COST OF THIS EDIT",
        // the concrete end state, which is what the reader has to be able to recognise
        "a TTL of `-1`",
        // the consequence in the product, not in Redis
        "stays locked until somebody deletes the key by hand",
        // and the two guarded spellings, so following the caveat ends GREEN rather than stuck
        "issue the `EXPIRE` only when `INCR` RETURNS 1",
        "a `MULTI` transaction or a one-line Lua script",
    ] {
        assert!(
            m.contains(needle),
            "lock-get-then-set's counter arm lost the cost of its own prescription — missing \
             {needle:?}. A reader who follows `INCR` + a separate `EXPIRE` and is interrupted between \
             them leaves a lockout counter with no expiry, and the locked account never unlocks. \
             In: {m}"
        );
    }

    // POSITION, through the shared helper so the subject is readable off the source: the cost has to
    // sit with the prescription it belongs to, which means AHEAD of the mutex arm's own imperative.
    // Every alternative spelling down there is for a different key, and a reader on the counter arm
    // stops before that heading — which is the whole defect this pin was written for.
    //
    // INVALIDATION PROBE: move the cost sentence behind `acquire the lock atomically instead` with
    // every token above still present — all the `contains` assertions stay green and this goes red.
    assert_disqualifier_clause_precedes_imperative(
        "lock-get-then-set",
        m,
        "THAT IS TWO ROUND TRIPS AND THE GAP BETWEEN THEM IS THE COST OF THIS EDIT",
        "acquire the lock atomically instead with `SET key value NX`",
    );
}
