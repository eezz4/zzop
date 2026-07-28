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
there, plus a `benign` array of control paths where ANY finding counts as a false positive.

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
   the target on THIS fixture set is zero FN (a FN = engine miss or bad label/placement) and zero
   FP (a FP = over-report, or a `good` that accidentally trips another rule; fix the `good`).

Surprises worth engine follow-up are engine work, not silent EXPECTED edits.

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

## Status

`EXPECTED.jsonc` is **current**: 15 trees, 143 findings, 143 labeled expectations, every expectation
met and nothing unexpected fired.

**That is a statement about this fixture set, not about zzop.** These defects were planted here to be
found, by the same people who wrote the rules that find them — so a clean score means "the corpus is
still an accurate model of what the engine does today", which is what it is for: it turns a rule
change from an opinion into a before/after set difference. It says nothing about what fraction of
real defects in your repo zzop catches, and no number derived from a synthetic corpus can. Each
rule's own catalog entry states its measured blind spots; those are the honest limits.

It is adjudicated by hand — see `EXPECTED.jsonc`'s header for the three admissible bases and for why
`--write-expected` against a fresh binary must never be committed.
