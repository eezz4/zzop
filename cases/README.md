# cases/ — the labeled detection benchmark

A synthetic, labeled multi-repo corpus for measuring zzop's detection **recall/precision** — a mini
OWASP-Benchmark / NIST-Juliet for this engine. Bad patterns are planted on purpose and mapped to their
expected rule ids in `EXPECTED.jsonc`.

**Committed and tracked**, along with the scoring harness at `scripts/measure/` — data and script
both ship. It lived at the gitignored `corpus/benchmark/` until 2026-07-26, on a rule ("measurement
material is local-only") that had quietly stopped fitting it: every one of these ~150 files is
synthetic code we wrote, with no third-party source and no embedded checkout, so nothing prevented
shipping it. The line is now authorship — what we wrote lives under `examples/` and is committed;
`corpus/oss/`, which is other people's repositories cloned locally, stays gitignored.

Reading it is a legitimate second use: each module is a minimal `bad`/`good` pair for exactly one
rule, so the corpus doubles as specification-by-example of what a rule does and does not catch.

Analysis caches (`.zzop/`) are still ignored wherever they appear here — pure derived state.

**One fixture is missing from a fresh clone**, and it is a real hole rather than a detail.
`trees/api-be/services/be-security.hardcoded-secret.ts` is named individually in `.gitignore`: the
`security/vendor-token-committed` rule only fires on a *contiguous* live vendor-token literal, so its
fixture must contain the exact string shape GitHub push protection rejects — and
`scripts/check-vendor-token-literals.sh`, which has no escape hatch on purpose, blocks it one step
earlier at commit time. Splitting the literal was measured, not assumed, and silences both rules that
fixture carries. So scoring this corpus **from inside this repository** reports 2 false negatives on
that one line and `benchmark.mjs` exits nonzero, which it now prints the reason for (it reads the
`untracked` list in `EXPECTED.jsonc`).

**Recreating the file at that path does not restore the score**, measured 2026-07-27: the analyzer
honors `.gitignore` (`crates/engine/src/pipeline/walking.rs`, ancestor ignores included), so the
fixture stays invisible whether or not it is on disk. The same corpus copied outside the repo, with
the fixture present, scores 143/143 with zero FP — that is how the expectation is kept honest. See
`.gitignore` for the full reasoning and the two ways out. The rule itself is not unmeasured —
`rules/dsl/security/vendor_token_committed.rs` covers it with split literals written to a temp
directory at test time.

## Run

The corpus is measured through the shipped `zzop-mcp` binary. There is no Node addon and no JS
analysis path any more (the old `run.mjs` / `measure.mjs` / `gen-expected.mjs` loaded
`packages/native` and `packages/cli/lib`, both of which were deleted; they were removed rather than
left as a broken entry point).

```bash
cargo build -p zzop-mcp --release

# 1. snapshot both axes (per-tree analyze + the cross-layer join), fully validated
node scripts/measure/snapshot.mjs --label bench-<what-changed> \
     --bin ./target/release/zzop-mcp --config cases/zzop.config.jsonc

# 2. score it against the ground truth
node scripts/measure/benchmark.mjs --run scratchpad/runs/bench-<what-changed> \
     --expected cases/EXPECTED.jsonc

# ... plus --dump to list every finding (use this to confirm each `good` example is silent)
```

Two snapshots can also be compared against each other with `scripts/measure/diff.mjs`, which reads
the anchor set difference rather than count deltas.

## Ground truth

`EXPECTED.jsonc` maps `"<sourceId>/<tree-relative path>:<line>"` to the rule ids expected to fire
there, plus a `benign` array of control paths where ANY finding counts as a false positive, plus a
`gap` object — same key shape — for shapes that *should* fire once a known capability lands and do not
today (see "Negative cases" below).

The `<sourceId>/` prefix is load-bearing. Keys used to be tree-relative only, and against the current
corpus that spelling collapses 12 distinct locations in different trees onto the same key (several
trees have an `index.ts:5`), which turns a miss in one tree into a hit from another.
`benchmark.mjs` refuses to score a legacy-format file rather than print a recall number that means
nothing.

Regenerate with `--write-expected`, but only after confirming via `--dump` that every `good` example
is silent: regeneration locks whatever fired as the truth, false positives included.

## Layout — one module per rule

```
cases/
├─ zzop.config.jsonc   # trees (multi-source): web-fe, api-be, …
├─ EXPECTED.jsonc      # "<sourceId>/<path>:<line>" -> [ruleId]
└─ trees/
   ├─ web-fe/
   │  ├─ index.ts               # barrel: namespace-imports every rule module (keeps them alive)
   │  └─ rules/<pack>.<rule>.tsx # ONE module per rule
   └─ api-be/
      ├─ index.ts
      ├─ api/<pack>.<rule>.ts    # rules gated to an `api/` path (file_pattern) live here
      └─ services/<pack>.<rule>.ts
```

**Module convention.** Each `<pack>.<rule>` module exports a **`bad`** (the violation — must fire) and a
**`good`** (the correct form — must NOT fire). The good example lives in the same file, so precision is
tested by it staying silent: a `good`-line finding shows up as an unexpected FP. The barrel
namespace-imports every module so nothing reads as a dead-candidate/dead-export (except the intentional
`dead.orphan` module, which the barrel skips).

**Path gating matters.** Many rules have a `file_pattern`/`require_file` gate (e.g. `sql/nplus1` needs an
`api/` path; `be-reliability/fetch-no-timeout` needs an express/`.listen(`/`.prepare(` signal in the file).
Place the module where its rule can fire — check the rule's matcher in `rules/dsl/<pack>/<pack>.json`.

**A tree-level `zzop.config.jsonc` changes the measurement.** `analyze_repo` auto-discovers a config
INSIDE a tree, while the join uses this directory's config. If the two disagree, `snapshot.mjs`
aborts with an axis-scope-divergence message rather than let a scope change read as a rule change.

## Adding a rule

1. Create `trees/<repo>/…/<pack>.<rule>.ts` with a `bad` + `good`, placed to satisfy the rule's gate.
2. Add its `import * as … from './…'` line to that tree's `index.ts` barrel.
3. Snapshot, then run `benchmark.mjs --dump` to see what fired and where.
4. Add `"<sourceId>/…/<pack>.<rule>.ts:<badLine>": ["<ruleId>"]` to `EXPECTED.jsonc`. Re-score —
   the target on THIS fixture set is zero FP (a FP = over-report, or a `good` that accidentally trips
   another rule; fix the `good`) and zero FN (a FN = engine miss or bad label/placement). A shape that
   is correct but that the engine is *documented as refusing* goes in the `gap` object instead of being
   written as an expectation the gate can never meet — see **negative cases** below for what earns a
   place there.

Surprises worth engine follow-up are engine work, not silent EXPECTED edits.

**Adding a backend tree used to have a hard ceiling — it is gone.** `bucket_keys.rs` capped each
cross-layer bucket's distinct-key list at 20, and `snapshot.mjs` aborted on truncation, so a corpus that
outgrew the cap produced NO score rather than a degraded one. This corpus hit it: a 2026-07-29 batch
adding trees pushed `unconsumedProvides` past 20 and scored zero lines. The cap, its `bucketKeysTruncated`
disclosure and the `--tolerate-bucket-key-cap` hatch were all deleted that day (user decision — see
`bucket_keys.rs`' module doc); bucket key lists are complete now and a corpus may grow freely.

The workaround that batch used is still worth keeping, on its own merits rather than as ceiling relief:
both backends added below are genuinely consumed (each front end has a `server/loader.ts` reaching the
same routes with full literal paths, the ordinary SSR shape). That makes each pair show a joining call and
a non-joining call side by side in one app, which is a better fixture than either alone.

## Two negative-control axes

1. **Per-module `good` exports** — a control for THAT MODULE'S rule, at the same anchor granularity as
   the `bad` it sits next to. Another rule firing on a `good` line can be independently correct.
2. **The `decoy` tree** — whole-file controls listed in `EXPECTED.jsonc`'s `benign` array, where **any**
   finding is a false positive. See `trees/decoy/README.md`.

Axis 2 exists because axis 1 only ever asks "does the rule stay quiet on the line next to the defect".
The question a real repo asks is "does it stay quiet across a file of ordinary code that *resembles*
what it looks for", and until 2026-07-26 the corpus did not ask it at all (`benign` was empty).

**A decoy is only worth something if the rule actually evaluated it.** Rules are gated by
`file_pattern` / `file_exclude_pattern` / `require_file`; a decoy that fails a gate was never scanned,
and counting its silence inflates precision for free. Two consequences, both load-bearing:

* every decoy states in its header which gate it satisfies and how (and `sql/nplus1` lives under `api/`,
  `secret-env-in-fe` under `web/`, `console-in-be` under `api/`, because those rules' path gates say so);
* in-scope-ness is **verified empirically, not by reading** — mutate each decoy with the real defect,
  re-run, confirm the target rule fires, restore. That pass caught two decoys whose matcher never
  engaged at all (`json-parse-no-try` parsed a plain string the trigger does not match;
  `interval-no-clear`'s veto was satisfied by a `declare` line), which reading had not caught.

## Attribute injection (Mode B adapter overlays)

`zzop-attributes.json` / `zzop-attributes-web.json` inject `env-config-module` and `auth-guarded`
attributes. Each is declared **in both places** — the in-tree `zzop.config.jsonc` (which `analyze_repo`
auto-discovers) and the outer config's `trees[].overlays` (which the join reads). Declaring in only one
makes `snapshot.mjs` abort with axis-scope divergence; both directions have been hit for real.

Some rules can ONLY be cleared this way: `http/protected-path-no-auth-evidence` reads `attr_absent: "auth-guarded"` and never
looks at the handler identifier, so before the overlay existed it fired on its own `good` export and was
the benchmark's single standing false positive.

## Negative cases — shapes that are supposed to fail

Until 2026-07-29 this corpus scored `TP 143 FN 0 FP 0` — every expectation met, nothing unexpected
fired. Over the same period only 2 of 6 front-end trees in the real dogfood corpus joined to a backend
at all. Both numbers were accurate, and the gap between them was the whole problem: **every front-end
consume here was a literal path**, so the answer key was planted exclusively inside zzop's capability
boundary. A perfect score on it measured the inside of that boundary, not the capability. Verified the
same day: no tree contained a cross-file `baseURL` assignment.

Two pairs now sit deliberately outside it. Each is a front end whose call sites carry only the path
SUFFIX, paired with a backend that genuinely provides the full route — so the **correct** answer is a
join and the **current** answer is a miss:

| pair | the shape | why zzop cannot resolve it |
| --- | --- | --- |
| `trees/fe-axios` × `trees/be-articles` | `axios.defaults.baseURL = settings.baseApiUrl` — a cross-file constant | `parser/parser-typescript/src/adapters/client_base.rs` recognizes a string **literal** only, and names this exact expression as the shape it refuses (never-guess IO convention) |
| `trees/fe-gensdk` × `trees/be-invoices` | a vendored swagger `HttpClient` whose `baseUrl` comes from the environment | `…/adapters/client_base_generated.rs` refuses a non-literal base the same way |

Neither refusal is a bug: guessing a base would mis-key every consume in the tree. What was missing was
any measurement of the resulting hole — and, once measured, a repair that did not require rewriting the
source. Both arrived on 2026-07-29: **`trees[].topology.clientBase`**, where the author STATES the base
the client carries, is declared on each front end in `cases/zzop.config.jsonc`. The three reads per pair
now join and the POST becomes the real `cross-layer/method-mismatch` it always was.

So what these pairs measure changed on purpose, from *how big is the hole* to *does the declared repair
work end to end*. The hole has not moved: delete either declaration and every expectation on those two
trees reverts. And each front end still calls the same four routes with full literal paths from
`server/loader.ts` — the Next-style split of a browser client with an implicit base and server-side
rendering without one. Those eight calls join today and are the standing check that the declared prefix
is applied **idempotently**: a blind prepend would re-key them to `/api/api/…`, and this gate would
report eight lost joins.

**How an unmet expectation is spelled: the `gap` disposition.** For three days these two were written
as **ordinary expectations**, and the gate was therefore permanently RED — `TP 147 FN 2 FP 0`, exit 1,
for something nobody intended to fix that week. That is intolerable as a steady state: it blocks every
push and it teaches everyone to ignore the gate. Writing them as `benign` was never the alternative —
"no finding here is correct" would freeze today's incapacity as the right answer forever, which is the
exact reason they were added.

Since 2026-07-29 `benchmark.mjs` has a third disposition, `"gap"`, a top-level object in
`EXPECTED.jsonc` using the same `"<sourceId>/<path>:<line>": [ruleId]` shape as an expectation. It means
*this should fire once a known capability lands, and today it does not*:

| | in the score | exit |
| --- | --- | --- |
| expectation, absent | FN | nonzero |
| `benign` control, present | FP | nonzero |
| **`gap`, absent** | **nothing — untouched recall and precision** | **zero** |
| **`gap`, present** | **nothing — it is not an FP either** | **nonzero, naming what to promote** |

The last row is the point. A gap that fires is *progress*, and it is refused rather than absorbed: the
run prints the full score and then exits nonzero demanding the entry be promoted by hand into an
ordinary expectation. Absorbed silently, the corpus would go on reporting `GAP 0/2` while the engine had
already closed both — an unnoticed capability gain is how a benchmark stops measuring. The count prints
on its own line in **every** run (`GAP 0/2 closed`), green or red, so the size of the acknowledged hole
is a number on screen rather than a paragraph here.

`scripts/detection-expected-baseline.txt` carries a **third column** for gaps. It needs one more than
the other two do: an expectation or a control defends itself in the score, but a gap is exit-zero while
open, so deleting one leaves the score *byte-identical*. Measured before the column existed — removing a
gap from the ground truth scored `TP 2 FN 0 FP 0`, recall 100.0%, precision 100.0%, exit 0.

`gap` is not a suppression list. An entry earns its place by naming the capability, the code that
refuses it today, and what would close it; a false negative with no named cause belongs in the score.

**The mechanism has since run once, end to end.** The two gapped entries were the
`cross-layer/method-mismatch` at each pair's POST: each backend provides its fourth route as GET only,
so with the base resolved each is a real method mismatch anchored at the consume. The run that shipped
`clientBase` printed `GAP 2/2 closed` and **aborted** — it produced a full score and then refused to
report it, naming the two lines to promote. Both anchors already carried a met
`cross-layer/unprovided-mutation-call` (which co-fires with method-mismatch per that rule's own module
doc), so the promotion was purely additive and the floor only grew.

The same commit **retired** the two `cross-layer/prefix-drift` entries, and that half was not automatic.
They were the degrade path and true positives while the base was unresolved: three same-method reads per
pair landed as `prefix`-dimension route-near-misses aggregating into one finding naming the missing
`/api`. Resolved, the three reads join and there is no drift left to name. `--update-baseline` grows
only, so retiring an expectation is a hand edit of `scripts/detection-expected-baseline.txt` — the
ratchet does not let a detection be given up without an author.

**There is deliberately no inverse of `gap`** — no "will stop firing" disposition for that degrade path.
It was considered and declined on 2026-07-29, and the retirement above is what it would have covered.
(1) Such entries are *scored* while they last; nothing about them is mislabeled in the meantime, so the
marker would buy only pre-authorization of a future deletion — a two-line hand edit, once, riding in the
same commit as the fix that causes it, which is precisely the diff-with-an-author the ratchet exists to
produce. (2) The scorer cannot tell *why* a true positive disappeared: "the base resolved" and "the
near-miss aggregation regressed" look identical at the score. `gap` is safe in the mirror position only
because it moves toward **more** signal and **stops** the run to demand adjudication; its inverse would
have to let a disappearance be *silent*, converting the ratchet's fail-closed edge into a fail-open one
on the very axis (recall) it exists to protect.

## Status

Every labeled expectation is met with nothing unexpected fired: the gate reads `FN 0 FP 0`, recall and
precision 100.0%, and **exits 0**. The counts themselves are not written here — `scripts/measure/detection-gate.sh`
prints them, and `scripts/detection-expected-baseline.txt` is the per-tree floor they are checked
against in both directions. Any FN at all is a regression, and so is `GAP` moving in either direction:
upward means a gap closed and must be promoted by hand, downward means an entry was deleted, which the
floor's third column refuses.

**That is a statement about this fixture set, not about zzop.** These defects were planted here to be
found, by the same people who wrote the rules that find them — so a clean score means "the corpus is
still an accurate model of what the engine does today", which is what it is for: it turns a rule
change from an opinion into a before/after set difference. It says nothing about what fraction of
real defects in your repo zzop catches, and no number derived from a synthetic corpus can. Each
rule's own catalog entry states its measured blind spots; those are the honest limits.

It is adjudicated by hand — see `EXPECTED.jsonc`'s header for the three admissible bases and for why
`--write-expected` against a fresh binary must never be committed.
