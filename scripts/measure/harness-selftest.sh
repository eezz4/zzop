#!/usr/bin/env bash
# harness-selftest.sh — damage the measurement harness ON PURPOSE, one way at a time, and require
# every one of its validation branches to ABORT. Preflight for `detection-gate.sh`.
#
# It covers BOTH halves of that gate, because the gate is only as honest as its quieter half:
#   PHASE 1  snapshot.mjs   vs `selftest-stub.rs`             — a binary that misbehaves.
#   PHASE 2  benchmark.mjs  vs `selftest-benchmark-cases.mjs` — a run directory, a ground truth and a
#                                                               coverage floor that are damaged.
# Phase 2 was added 2026-07-29. Until then benchmark.mjs — the SCORER, whose exit code IS the
# detection gate's verdict — had exactly one path any test had ever taken: the happy one. Its `fail(`
# branches for a corrupted run directory, an absent meta.json and an unparseable answer key had never
# been seen fire. Four of them turned out not to be `fail(` branches at all (see PHASE 2's own header).
#
# ## Why this file exists
# `selftest-stub.rs` has sat in the tree since 2026-07-26 carrying snapshot.mjs's most expensive
# sentence — "a guard nobody has seen go red is not known to work" — and until this script NOTHING
# RAN IT. Its build-and-run loop lived in a comment as a manual procedure: the file is outside
# `crates/` and `packages/` so `cargo test --workspace` never saw it, no Cargo.toml declares it as a
# `[[test]]` or `[[bin]]`, and `detection-gate.sh` invoked only snapshot.mjs and benchmark.mjs. The
# one place in this repo that states the rule most loudly was the rule's own violation.
# Measured 2026-07-29 before wiring: all seven branches DID fire when run by hand. The defect was the
# wiring, not the guards — so this script changes who runs them, not what they assert.
#
# ## Why this is not scripts/check-*.sh
# Same reason `detection-gate.sh` is not (see its header): `check-guards-wired.sh` would then require
# it in `.githooks/pre-commit` too, and the cost is entirely PROCESS SPAWNS — a rustc compile (~1s)
# plus one Node spawn per mode, now 48 of them rather than seven. Measured 2026-07-29 end to end on a
# Windows working tree: 102s, of which 40s is phase 2's loop, because a cold Node spawn costs ~1s
# there; the same set was ~4s when it was phase 1 alone on CI. That is a CI job's budget to protect a
# measurement harness that changes rarely, and not a pre-commit hook's — so it rides the job that
# already runs snapshot.mjs and preflights the very scripts whose silence that job's verdict is.
#
# It needs NO --release build, which is what separates it from the gate that calls it: run it
# standalone while editing snapshot.mjs or benchmark.mjs.
#
#   bash scripts/measure/harness-selftest.sh          # honors $RUSTC if rustc is not on PATH
#
# ## What counts as passing, per mode (both phases)
#   * a NONZERO exit,
#   * an abort message naming THAT mode's specific defect — `HARNESS ABORT` in phase 1, `BENCHMARK
#     ABORT` in phase 2. A mode that aborts for the WRONG reason is a failure, because otherwise one
#     over-eager branch could stand in for six dead ones. This is not hypothetical: while phase 2 was
#     being written, `write-derives-zero` started aborting on a NEWLY ADDED consistency check instead
#     of the ratchet branch it exists to prove, and the needles caught it,
#   * and nothing damaged left behind — phase 1: no directory under --runs (contract 1: a partial run
#     must never be diffable and its label must stay free for the retry); phase 2: no regenerated
#     ground truth and no lowered floor, because a guard that refuses AFTER writing refused nothing.
# A phase 1 mode that produces a snapshot, or a phase 2 mode that produces a score, is a regression.
#
# ## Still unproven by this script
# Contract 1's OTHER half — "an existing label is REFUSED" — needs no stub and is not exercised here;
# it is the branch that destroyed 22 baseline files mid-audit. Recorded rather than left implied.
# Phase 2 proves the scorer's REFUSALS, never its arithmetic beyond the one known-good case: that a
# real corpus scores 143/143 is the detection gate's own job, and it is a different question.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

stub_src="scripts/measure/selftest-stub.rs"
snapshot="scripts/measure/snapshot.mjs"
bench_cases="scripts/measure/selftest-benchmark-cases.mjs"
benchmark="scripts/measure/benchmark.mjs"
# snapshot.mjs only checks that --config EXISTS; none of these modes reach a parse of it (the stub
# never looks at its argv). Pointing at the real corpus config keeps the invocation honest anyway.
config="cases/zzop.config.jsonc"

for p in "$stub_src" "$snapshot" "$bench_cases" "$benchmark" "$config"; do
  if [ ! -e "$p" ]; then
    echo "harness-selftest: missing $p — this script proves nothing without it." >&2
    exit 1
  fi
done

rustc_bin="${RUSTC:-rustc}"
if ! command -v "$rustc_bin" >/dev/null 2>&1; then
  echo "harness-selftest: '$rustc_bin' not found. The stub is a standalone single file compiled on" >&2
  echo "  demand (it is deliberately not a workspace member). Set \$RUSTC or put rustc on PATH." >&2
  exit 1
fi

# mode -> substrings that MUST all appear in that mode's abort output. Quoted heredoc: backticks and
# single quotes below are literal. `tr -d` because a Windows working tree checks this file out CRLF
# and a trailing \r would silently make every needle unmatchable.
table="$(tr -d '\r' <<'TBL'
empty|EMPTY stdout
initonly|no `tools/call` reply
garbage|is not JSON
rpcerror|JSON-RPC error
iserror|tool reported isError
wrongpayload|missing key 'sources'
fail|exited 2|does not speak MCP
TBL
)"

# The subject set is derived from the STUB, not hardcoded here: a mode added to selftest-stub.rs
# without an expectation row (or a row whose mode the stub no longer knows) fails right here rather
# than quietly shrinking what this script covers.
stub_arms="$(sed -n 's/^[[:space:]]*"\([a-z]*\)" =>.*/\1/p' "$stub_src" | tr -d '\r' | sort)"
table_modes="$(printf '%s\n' "$table" | cut -d'|' -f1 | sort)"

if [ -z "$stub_arms" ]; then
  echo "harness-selftest: FAILED -- enumerated ZERO stub modes from $stub_src. The match arm pattern" >&2
  echo "  stopped matching, so this script would 'pass' having proven nothing. An empty subject set is" >&2
  echo "  a broken runner, never a clean result." >&2
  exit 1
fi

if [ "$stub_arms" != "$table_modes" ]; then
  echo "harness-selftest: the stub's modes and this script's expectation table disagree." >&2
  echo "  stub:  $(echo $stub_arms)" >&2
  echo "  table: $(echo $table_modes)" >&2
  echo "  Every stub mode needs a row naming the abort it must produce." >&2
  exit 1
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# The `.exe` is written into the name on BOTH platforms rather than probed for afterwards. rustc does
# NOT append an extension when `-o` is explicit — measured 2026-07-29 on Windows, it produced a file
# literally named `stub` — and Windows `CreateProcessW` appends `.exe` to any image name that has no
# extension, so Node's spawnSync turned that file into ENOENT while MSYS bash's `[ -f ]` cheerfully
# said it was there. The earlier code probed for `stub.exe` first precisely to dodge that shell/Node
# disagreement, but it was dodging a file rustc never writes: every mode failed with a spawn ENOENT
# instead of its own defect, which this script correctly called a failure and no platform could pass.
# `stub.exe` is a perfectly ordinary filename on Linux, so naming it once removes the probe, the
# two-path uncertainty, and the assumption underneath both.
stub="$work/stub.exe"
"$rustc_bin" -O "$stub_src" -o "$stub"
if [ ! -f "$stub" ]; then
  echo "harness-selftest: rustc reported success but produced no binary at $stub" >&2
  exit 1
fi

runs="$work/runs"
failed=0
checked=0

echo "harness-selftest: PHASE 1 — proving snapshot.mjs aborts on $(printf '%s\n' "$table" | wc -l | tr -d ' ') deliberate defects"

while IFS= read -r row; do
  [ -n "$row" ] || continue
  mode="${row%%|*}"
  needles="${row#*|}"
  label="selftest-$mode"
  checked=$((checked + 1))

  set +e
  out="$(ZZOP_STUB="$mode" node "$snapshot" \
    --label "$label" --bin "$stub" --config "$config" --runs "$runs" 2>&1)"
  rc=$?
  set -e

  problems=""
  if [ "$rc" -eq 0 ]; then
    problems="${problems}
    exited 0 — it WROTE A SNAPSHOT from a binary that does not measure anything."
  fi
  if ! grep -Fq -- "HARNESS ABORT" <<< "$out"; then
    problems="${problems}
    no 'HARNESS ABORT' in the output — it failed for some other reason, or silently."
  fi

  IFS='|' read -r -a needle_arr <<< "$needles"
  for n in "${needle_arr[@]}"; do
    if ! grep -Fq -- "$n" <<< "$out"; then
      problems="${problems}
    abort did not name the defect: expected to see \"$n\"."
    fi
  done

  if [ -e "$runs/$label" ]; then
    problems="${problems}
    left $runs/$label behind — a partial run must remove itself and free its label."
  fi

  if [ -n "$problems" ]; then
    failed=1
    echo "  FAIL  $mode$problems" >&2
    echo "    --- what snapshot.mjs actually said (exit $rc) ---" >&2
    printf '%s\n' "$out" | sed 's/^/    /' >&2
  else
    echo "  ok    $mode -> $(printf '%s' "$needles" | tr '|' ',')"
  fi
done <<< "$table"

# Nothing above should have survived: every mode aborts, and every abort removes its own directory.
if [ -d "$runs" ] && [ -n "$(ls -A "$runs")" ]; then
  failed=1
  echo "harness-selftest: FAILED -- snapshots left under $runs after all modes aborted:" >&2
  ls -A "$runs" >&2
fi


# =====================================================================================================
# PHASE 2 — benchmark.mjs, the SCORER
# =====================================================================================================
# Phase 1 proves the harness refuses to RECORD a broken measurement. This proves it refuses to SCORE
# one. The gate's verdict is benchmark.mjs's exit code, so a dead branch here is the shorter path to
# the same accident: a malfunction translated into a number, and a number into a green CI job.
#
# `selftest-benchmark-cases.mjs` builds a known-good scoring sandbox and damages it one way at a time
# (see its header for why the sandbox holds a COPY of benchmark.mjs — the coverage floor's path is
# derived from the script's own location and no flag moves it, so the floor can only be damaged safely
# by moving its neighbourhood, never the tracked file the real gate reads).
#
# ## The `good` case is not decoration
# It runs FIRST and must exit 0 with `TP 2  FN 0  FP 0`. A sandbox that is already red would make every
# damage below "fire" for a reason that has nothing to do with the damage — which is the same
# wrong-reason pass phase 1's per-mode needles exist to catch, one level up.
#
# ## What phase 2 found on the day it was written (2026-07-29)
# Four damages were NOT caught by a `fail(` branch at all, and two of those are the interesting kind:
#   * meta.json truncated, cross.json absent, a per-tree file absent, EXPECTED.jsonc truncated — all
#     exited nonzero, but through an uncaught Node throw (`SyntaxError`, `ENOENT`) with a stack trace
#     naming `node:fs` and nothing naming the run. Safe by accident: the exit code was right because
#     Node's default is right, not because this script's contract was enforced. Notably
#     `--write-expected` and `--update-baseline` both already caught a parse failure of the SAME file;
#     the score path, the mode CI runs, was the one that did not.
#   * a per-tree payload missing `findings.shown`, and a cross.json missing `crossLayerFindings` —
#     SILENTLY SWALLOWED. `a.findings.shown || []` and `(cross.crossLayerFindings || {}).shown || []`
#     absorbed the damage as an EMPTY anchor list, and an empty anchor list is not an error anywhere
#     downstream: it is a full sweep of false negatives. Measured, the score came back `TP 1  FN 1`,
#     red for the wrong reason and naming a rule that had fired perfectly. That cannot manufacture a
#     GREEN result (an FN always exits nonzero), but it manufactures a false DIAGNOSIS, and it points
#     the reader at the engine for damage that is in the harness.
# All six now abort by name. The rows below are what keeps them doing so.
#
# ## What is deliberately NOT covered, and why
# `missing --expected` and `--<name> needs a value` die on ABSENT ARGV: they cannot be reached with a
# value in hand and have no silent form, so a row for each would raise a count without raising
# confidence. Same for `--bootstrap-baseline` when the floor exists and `--update-baseline` when it
# does not — plain existsSync ergonomics on a hand-run mode. The ratchet's DIRECTION, which is the part
# that actually stops laundering, is covered three ways (ratchet-shrank, ratchet-grew, update-lowers).
bench_table="$(tr -d '\r' <<'TBL'
no-meta|no meta.json in
meta-corrupt|meta.json could not be parsed|truncated or hand-edited snapshot
axes-absent|did not run 'analyze_repo'|axes: []
meta-no-trees|carries no `trees`
axes-partial|did not run 'cross_repo'
cross-missing|is missing cross.json
tree-missing|is missing tree-alpha.json
tree-findings-empty|no `findings.shown` array for tree 'alpha'|full sweep of false negatives
cross-findings-empty|cross.json has no `crossLayerFindings.shown` array
expected-missing|no ground truth at
expected-corrupt|EXPECTED.jsonc could not be parsed|against an unparsed answer key is not a score
expected-empty|has no expectations
benign-ghost|benign control(s) in the ground truth name files that DO NOT EXIST|alpha/src/ghost.ts
legacy-keys|does not use the "<sourceId>/<path>:<line>" key format
registry-missing|no tree registry at
registry-corrupt|tree registry|could not be parsed
registry-no-trees|is not a usable trees[] list
registry-tree-removed|MISSING beta|never scored at all
baseline-missing|expectation ratchet: missing
baseline-badrow|unreadable row "alpha 2"
ratchet-shrank|claims LESS than the committed floor|SHRANK alpha.expectations: 2 -> 1
ratchet-grew|claims MORE than the committed floor|GREW  alpha.expectations: 2 -> 3
gap-fired|`gap` entries FIRED (1 of 1)|PROMOTE each line above by hand
gap-fired-write|refusing to regenerate: `gap` entries FIRED|promoted by no one
gap-ratchet-shrank|claims LESS than the committed floor|SHRANK alpha.gap: 1 -> 0
write-over-corrupt|refusing to regenerate
write-derives-zero|derived ZERO claims
update-lowers|refusing to LOWER the floor
TBL
)"

# Derived from the CASE FILE, exactly as phase 1 derives its subjects from the stub: a damage added
# there without a row naming the abort it must produce fails here rather than going unexercised.
bench_cases_modes="$(node "$bench_cases" --list | tr -d '\r' | sort)"
bench_table_modes="$(printf '%s\n' "$bench_table" | cut -d'|' -f1 | sort)"

if [ -z "$bench_cases_modes" ]; then
  echo "harness-selftest: FAILED -- enumerated ZERO damage modes from $bench_cases. An empty subject" >&2
  echo "  set is a broken runner, never a clean result." >&2
  exit 1
fi

if [ "$bench_cases_modes" != "$bench_table_modes" ]; then
  echo "harness-selftest: the damage cases and this script's expectation table disagree." >&2
  echo "  cases: $(echo $bench_cases_modes)" >&2
  echo "  table: $(echo $bench_table_modes)" >&2
  echo "  Every damage needs a row naming the abort it must produce." >&2
  exit 1
fi

bench_root="$work/bench"
bench_checked=0

echo "harness-selftest: PHASE 2 — proving benchmark.mjs aborts on $(printf '%s\n' "$bench_table" | wc -l | tr -d ' ') deliberate corruptions"

# The known-good anchor. If this is not green, nothing below means anything.
good_dir="$bench_root/good"
good_argv=()
while IFS= read -r a; do [ -n "$a" ] || continue; good_argv+=("$a"); done < <(node "$bench_cases" --mode good --out "$good_dir" | tr -d '\r')
set +e
good_out="$(node "$good_dir/measure/benchmark.mjs" "${good_argv[@]}" 2>&1)"
good_rc=$?
set -e
# `GAP 0/1 closed` is asserted alongside the score on purpose: it is the ONLY place the silent-gap
# exit-zero path is pinned. A `gap` entry that stays quiet changes no TP/FN/FP number by design, so
# without this needle the whole disposition could stop working and the undamaged sandbox would still
# read as clean — the disposition would have become a no-op that nothing notices.
if [ "$good_rc" -ne 0 ] || ! grep -Fq -- "TP 2   FN 0   FP 0" <<< "$good_out" \
   || ! grep -Fq -- "GAP 0/1 closed" <<< "$good_out"; then
  failed=1
  echo "  FAIL  good — the UNDAMAGED sandbox does not score clean (exit $good_rc), so every" >&2
  echo "        corruption below would 'abort' for a reason unrelated to its damage." >&2
  printf '%s\n' "$good_out" | sed 's/^/    /' >&2
else
  echo "  ok    good -> scores TP 2 FN 0 FP 0, exit 0"
fi

while IFS= read -r row; do
  [ -n "$row" ] || continue
  mode="${row%%|*}"
  needles="${row#*|}"
  case_dir="$bench_root/$mode"
  bench_checked=$((bench_checked + 1))

  run_argv=()
  while IFS= read -r a; do [ -n "$a" ] || continue; run_argv+=("$a"); done < <(node "$bench_cases" --mode "$mode" --out "$case_dir" | tr -d '\r')

  set +e
  out="$(node "$case_dir/measure/benchmark.mjs" "${run_argv[@]}" 2>&1)"
  rc=$?
  set -e

  problems=""
  if [ "$rc" -eq 0 ]; then
    problems="${problems}
    exited 0 — it SCORED a corrupted input. This is the manufactured-green case."
  fi
  if ! grep -Fq -- "BENCHMARK ABORT" <<< "$out"; then
    problems="${problems}
    no 'BENCHMARK ABORT' in the output — it died through an uncaught throw, or not at all."
  fi

  IFS='|' read -r -a needle_arr <<< "$needles"
  for n in "${needle_arr[@]}"; do
    if ! grep -Fq -- "$n" <<< "$out"; then
      problems="${problems}
    abort did not name the defect: expected to see \"$n\"."
    fi
  done

  # An abort must not have half-done the job first. The two regeneration modes must leave the ground
  # truth untouched, and the floor-lowering mode must leave the floor untouched — a guard that refuses
  # AFTER writing has refused nothing.
  case "$mode" in
    write-over-corrupt | write-derives-zero | gap-fired-write)
      if grep -Fq -- "AUTO-CALIBRATED from snapshot" "$case_dir/EXPECTED.jsonc" 2>/dev/null; then
        problems="${problems}
    it REGENERATED the ground truth before refusing — the abort came too late to protect anything."
      fi
      ;;
    update-lowers)
      if ! grep -Fq -- "alpha 2 1" "$case_dir/detection-expected-baseline.txt" 2>/dev/null; then
        problems="${problems}
    it LOWERED the floor before refusing — the ratchet's whole contract is that it does not."
      fi
      ;;
  esac

  if [ -n "$problems" ]; then
    failed=1
    echo "  FAIL  $mode$problems" >&2
    echo "    --- what benchmark.mjs actually said (exit $rc) ---" >&2
    printf '%s\n' "$out" | sed 's/^/    /' >&2
  else
    echo "  ok    $mode -> $(printf '%s' "$needles" | tr '|' ',')"
  fi
done <<< "$bench_table"

if [ "$failed" -ne 0 ]; then
  echo "" >&2
  echo "harness-selftest: FAILED. A validation branch in the measurement harness no longer fires." >&2
  echo "  Every measurement this repo takes runs through snapshot.mjs, and every detection verdict runs" >&2
  echo "  through benchmark.mjs. The failure mode both exist to stop is a harness going quiet — which" >&2
  echo "  reads as the best result of the batch, not as an error." >&2
  echo "  Fix the branch, do not relax this script." >&2
  exit 1
fi

echo "harness-selftest: OK (phase 1: $checked/$checked snapshot aborts, none left a snapshot behind;"
echo "                      phase 2: $bench_checked/$bench_checked scorer aborts, known-good still green)"
