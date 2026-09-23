#!/usr/bin/env bash
# detection-gate.sh — score the labeled detection benchmark and FAIL when it regresses.
#
# This is the CI gate that `cases/` was committed for (2026-07-26) and that nothing ran until
# 2026-07-28. Until now `snapshot.mjs` and `benchmark.mjs` both declared themselves "NOT A GUARD, AND
# NOT WIRED INTO CI" in their own headers, and they were right: a 298-line adjudicated ground truth sat
# in the tree and no run compared anything to it. A benchmark nobody scores is a document, not a gate.
#
# ## Why this is not `scripts/check-*.sh`
# `check-guards-wired.sh` requires every `scripts/check-*.sh` to run in BOTH `.githooks/pre-commit` and
# `ci.yml`. This gate needs a RELEASE build of `zzop-mcp` plus a two-axis run over 15 trees — minutes,
# not the seconds a pre-commit hook may take. Naming it `check-*` would either lie to that meta-guard or
# make every commit unbearable. It is a CI job instead, and it stays out of the guard glob on purpose.
#
# ## Why this takes NO ARGUMENTS — it builds the binary it scores
# Until 2026-07-29 the binary path was `$1`, and the only thing asserted about it was `-x`. So the gate
# scored whatever happened to be lying in `target/release/`. MEASURED: against a 0.25.0 source tree, a
# 0.24.0 binary left over from the previous release scored `TP 143 FN 0 FP 0`, recall 100.0%,
# precision 100.0%, exit 0 — a full green run reporting on code that was never compiled.
#
# That is the worst possible place for it. The `detection-benchmark` CI job only runs AFTER a push, so
# this local invocation is the sole opportunity a developer has to see a detection regression before
# pushing one; and the paragraph directly above says this gate needs a release build. Keeping that
# build OUTSIDE the script is what opened the hole. It is inside now, which makes that paragraph true
# rather than merely explanatory.
#
# The hole was the PARAMETER, not a missing check, so the fix REMOVES rather than adds. The two
# checks considered were both worse than deletion: comparing version strings only catches release
# boundaries and misses every staleness within a cycle (which is most of them), and comparing mtimes
# goes falsely RED in CI because `Swatinem/rust-cache` rewrites mtimes when it restores. Freshness is
# cargo's question and cargo answers it exactly; a shell script re-deciding it would be a second,
# worse owner. Note that a version assertion here would now be tautological — the artifact comes from
# this tree's own Cargo.toml by construction — which is the sign the argument was the whole defect.
#
# `snapshot.mjs` was already recording `serverInfo` (CARGO_PKG_VERSION-derived), `binarySha256`,
# `binaryMtime` and `binarySize` in every meta.json, and `diff.mjs` prints all four without asserting
# on any. The evidence was collected the whole time. That is worth knowing when reading a past run's
# meta.json — but it is not a reason to add a fifth assertion to a script that no longer accepts an
# untrusted binary in the first place.
#
# ## Why the corpus is COPIED out of the repo first
# One fixture cannot be committed: `security/vendor-token-committed` only fires on a CONTIGUOUS
# `sk_live_`-shaped literal, which is exactly what GitHub push protection and
# `check-vendor-token-literals.sh` (no escape hatch, by design) both refuse. So the file is gitignored,
# and — measured 2026-07-27, recorded in `.gitignore` — the ANALYZER HONORS .gitignore (ancestor ignores
# included), which means recreating it at its own path changes nothing. It stays invisible.
#
# The `.gitignore` note left this as an open question for whoever wired the gate: synthesize the file
# OUTSIDE the ignored path, or drop the expectation and accept the coverage loss. This script takes the
# first, because the second silently narrows what the benchmark can claim — and because the outcome was
# already measured: "the same corpus copied outside the repo, fixture present, scores 143/143 with zero
# FP". Copying to a temp dir puts the tree beyond every ancestor .gitignore, so the synthesized fixture
# is actually walked.
#
# The generator below writes a contiguous literal while its OWN source never contains one: the vendor
# prefix and the token body are separate shell strings, concatenated at write time. That is the same
# trick `concat!` gives Rust fixtures, and it is why this file passes the vendor-token guard it exists
# to exercise.
#
# usage: bash scripts/measure/detection-gate.sh          (takes no arguments — see above)
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

# A path passed here is REFUSED rather than ignored. Ignoring it would leave the caller — an old CI
# step, a habit, a copied command line — believing their binary was the one scored.
if [ "$#" -ne 0 ]; then
  echo "detection-gate: this script takes NO arguments (got: $*)." >&2
  echo '  It builds what it scores (`cargo build -p zzop-mcp --release`). Accepting a path is precisely' >&2
  echo '  how a leftover 0.24.0 binary scored a green 143/143 against a 0.25.0 tree on 2026-07-29.' >&2
  exit 1
fi

# Path updated in the same commit that moves the benchmark to its own root directory.
cases_dir=cases
expected="$cases_dir/EXPECTED.jsonc"
for p in "$cases_dir" "$expected"; do
  if [ ! -e "$p" ]; then
    echo "detection-gate: missing $p — the labeled benchmark is what this gate scores." >&2
    exit 1
  fi
done

# --- THE VERDICT STAMP ---------------------------------------------------------------------------
#
# ## The defect this exists for, and it is not a rule
#
# MEASURED: this gate was red from 9b89e62 (2026-08-21) for roughly ninety commits and nobody knew.
# Three unrelated commits each took one expectation, the rule was right all three times, and the key
# was stale. `detection-gate-if-touched.sh` now runs this script from the commit that causes the next
# one — but that only closes the case where a commit BOTH stages a trigger path AND runs the gate to
# completion. What left the eight days invisible is narrower and outlives that fix: A RED RUN LEFT NO
# TRACE. When this script exits 1, the only place the verdict exists is the terminal it printed to.
# The next commit that stages nothing under the trigger paths says "detection gate not armed" and
# exits 0, and so does every commit after it. Silence after a red and silence after no run are the
# same bytes on screen.
#
# `scripts/ci-local.sh` already writes `.zzop/last-verified` (sha, time) after its whole mirror
# passes, and this repo's maintainer-side review meter reads it every turn. That stamp cannot
# carry this: it is written ONLY on success, so a red detection gate makes ci-local `fail` before the
# write and leaves the PREVIOUS sha in place. The reader then reports "stale", which is what it
# reports when nobody ran anything — the exact conflation that hid the ninety commits.
#
# So this stamp records the VERDICT, including the failing one, and is written on the way in as well
# as on the way out. Its reader is scripts/detection-gate-staleness.sh, which .githooks/pre-commit
# runs on EVERY commit, armed or not.
#
# ## Three states, and why `started` is one of them
#   started    — this script began and has not reached a verdict. Left behind by a kill, a failed
#                harness selftest, a failed release build, or a run still in progress. It is NOT
#                "aborted": from outside, a gate that is still compiling and a gate that died look
#                identical, and inventing a fourth state to pretend otherwise would be a claim this
#                script cannot make. What the reader needs is that it is not a verdict.
#   clean      — benchmark.mjs exited 0 against cases/EXPECTED.jsonc.
#   regressed  — benchmark.mjs exited nonzero: FN and/or FP against the key.
#
# The distinction between `started` and `regressed` is the same one this gate's own preflight was
# taught on 2026-08-29, when a missing `rustc` printed "re-adjudicate the answer key" over a PATH
# problem. An environment failure must not be recorded wearing a detection failure's clothes.
#
# ## Why .zzop/ and why it may vanish
# `**/.zzop/` is gitignored, so the stamp is local BY CONSTRUCTION and cannot travel to another
# checkout to be read as a claim about that tree — the same reasoning ci-local.sh's stamp states. The
# directory is derived state and anything may delete it; losing the stamp then reads as "never
# recorded", which is the loud direction, not the silent one.
#
# ## What it deliberately does NOT claim
# A stamp says a run happened at a sha and what it answered. It says nothing about uncommitted edits
# made after it, and nothing about the paths this run did not cover. The reader prints both caveats.
GATE_STAMP=".zzop/detection-gate-last-run"

# Four space-separated fields: verdict, HEAD sha, epoch seconds, ISO-8601 UTC. Epoch is stored rather
# than derived from the ISO field by the reader because the reader runs from `.githooks/pre-commit`,
# whose PATH on macOS is the BSD userland: `date -d` does not exist there, and a reader that computed
# an age from the timestamp would be right on Linux and silently wrong here. Both fields are kept —
# the epoch is for arithmetic, the ISO is for the human reading the file with `cat`.
#
# It can never fail this gate. A stamp is a report ABOUT the run; a run that scored 292/292 and then
# could not write a file under .zzop/ has still scored 292/292, and turning that into a nonzero exit
# would be a new way for an environment problem to wear a detection failure's clothes — the very thing
# the state list above exists to prevent.
gate_stamp() { # <verdict>
  mkdir -p "$(dirname "$GATE_STAMP")" 2> /dev/null || return 0
  printf '%s %s %s %s\n' \
    "$1" \
    "$(git rev-parse HEAD 2> /dev/null || echo unknown)" \
    "$(date -u +%s)" \
    "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
    > "$GATE_STAMP" 2> /dev/null || return 0
  return 0
}
gate_stamp started

# PREFLIGHT — prove the harness can still go red before trusting a number it produces.
# Everything below depends on snapshot.mjs ABORTING rather than writing a zero, and the accident this
# whole harness was built around wrote 22 zero-byte files that read as "460 findings -> 0, all fixed".
# `harness-selftest.sh` covers BOTH halves of the harness and requires every validation branch to fire
# with its own message. The count is deliberately not written here — it moved from 7 to 30 in a single
# 2026-07-29 batch and a number in this comment would already have been false by the end of that day.
# Ask the script: `bash scripts/measure/harness-selftest.sh --list`.
#   * PHASE 1 — snapshot.mjs against `selftest-stub.rs`, a binary that misbehaves on purpose.
#   * PHASE 2 — benchmark.mjs, the SCORER, against a deliberately damaged run directory. Added after
#     two of its branches were measured absorbing a corrupted snapshot as an empty anchor list: the
#     score then reads as a sweep of false negatives and points the reader at the engine for damage
#     that is in the harness.
#
# It lives here, and not in `.githooks/pre-commit` as a `scripts/check-*.sh`, for the same budget
# reason this file does; but note the asymmetry: the selftest needs NO --release build, so it can be
# run standalone (`bash scripts/measure/harness-selftest.sh`) while editing snapshot.mjs or
# benchmark.mjs. This is
# simply the only automated place snapshot.mjs runs, so it is where the proof gets to run too.
# Before 2026-07-29 nothing ran it at all: the stub's build-and-run loop was a comment.
bash scripts/measure/harness-selftest.sh

# SECOND PREFLIGHT — is every fixture that names a rule actually SCORED by the key below?
#
# The selftest above proves the harness can still go red. This proves the CORPUS can: a fixture named
# after a rule but anchored nowhere in cases/EXPECTED.jsonc contributes nothing but silence, and
# silence is what a working rule and a rule that stopped seeing the file both produce. Any drift
# inside such a file scores as success — which is how `be-security.timing-unsafe-compare.ts` came to
# label, as its GOOD half, the exact call shape its own rule's message had begun warning throws
# (repaired in 4c504978). The script's header carries that case and carries the measurement of the
# sharper check that was tried first and does NOT carry.
#
# It runs HERE — after the selftest, before the release build — for the reason the selftest gives:
# it costs a node spawn and the build costs minutes, so a corpus that cannot be scored fails before
# anything pays for a compile.
#
# Its failure is reported under its OWN name and never as an FN/FP. That distinction is the lesson of
# 2026-08-29, when a missing `rustc` printed "re-adjudicate the answer key" over a PATH problem: an
# input failure wearing a detection failure's clothes sends the next reader to the escape hatch.
set +e
node scripts/measure/fixture-anchor-coverage.mjs
coverage_rc=$?
set -e
if [ "$coverage_rc" -ne 0 ]; then
  echo "detection-gate: the corpus coverage preflight FAILED (exit $coverage_rc)." >&2
  echo "  This is NOT a detection regression -- nothing was scored. The message above names the" >&2
  echo "  fixtures the key does not mention, or says the check could not measure at all." >&2
  exit "$coverage_rc"
fi

# BUILD — see "Why this takes NO ARGUMENTS" above. Runs after the preflight because the preflight
# costs ~4s and this costs minutes: fail on a dead validation branch before paying for a release build.
#
# The artifact path is READ BACK FROM CARGO rather than spelled `target/release/zzop-mcp`. That
# spelling is wrong under $CARGO_TARGET_DIR, wrong under a `.cargo/config.toml` `build.target-dir`,
# and wrong on Windows (`.exe`) — three separate ways to reintroduce "score whatever sits at this
# path", which is the exact defect this change removes. `json-render-diagnostics` keeps compiler
# errors human-readable on stderr while the machine-readable artifact stream comes down stdout.
# `pipefail` is set, so a failed compile aborts the gate instead of scoring a stale artifact.
echo "detection-gate: building zzop-mcp --release (this gate builds what it scores)"
bin="$(cargo build -p zzop-mcp --release --message-format=json-render-diagnostics | node -e '
  let exe = null;
  for (const l of require("fs").readFileSync(0, "utf8").split("\n")) {
    if (!l.trim()) continue;
    let m;
    try { m = JSON.parse(l); } catch { continue; }
    // A lib or proc-macro unit reports executable:null; only the bin target names a file to run.
    if (m.reason === "compiler-artifact" && m.executable && (m.target && m.target.kind || []).includes("bin")) exe = m.executable;
  }
  if (!exe) {
    console.error("detection-gate: cargo emitted NO executable artifact for zzop-mcp. Refusing to score:");
    console.error("  there is no binary this run could honestly claim to have measured.");
    process.exit(1);
  }
  process.stdout.write(exe);
')"
if [ ! -x "$bin" ]; then
  echo "detection-gate: cargo named '$bin' but it is not executable." >&2
  exit 1
fi

work="$(mktemp -d)"
# `mktemp -d` lands outside the repository, which is the whole point (see the header): an ancestor
# .gitignore would otherwise keep the synthesized fixture invisible to the walker.
trap 'rm -rf "$work"' EXIT

mkdir -p "$work/corpus"
cp -R "$cases_dir/." "$work/corpus/"
# Any cache the local tree happens to carry must not ride along — a warm `.zzop/` from a different
# binary would be read as this run's own, and the first thing a stale cache does is look like a clean
# score.
find "$work/corpus" -type d -name '.zzop' -prune -exec rm -rf {} + 2>/dev/null || true

# The one fixture the tracked tree cannot hold. Prefix and body are separate strings HERE and one
# contiguous literal THERE — see the header for why both halves of that sentence matter.
secret_prefix='sk_live_'
secret_body='51a9Xk2mQd7RtY0pLvB3nZ8w'
fixture="$work/corpus/trees/api-be/services/be-security.hardcoded-secret.ts"
mkdir -p "$(dirname "$fixture")"
cat > "$fixture" <<EOF
// be-security/hardcoded-secret — bad: a secret-shaped literal assigned to a secret-named key. good:
// injected via a config object (no literal in source).
export const bad = { apiKey: '${secret_prefix}${secret_body}' };

export function good(cfg: { apiKey: string }) {
  return cfg.apiKey;
}
EOF

echo "detection-gate: scoring $cases_dir through $bin"
node scripts/measure/snapshot.mjs \
  --label ci \
  --bin "$bin" \
  --config "$work/corpus/zzop.config.jsonc" \
  --runs "$work/runs"

# `benchmark.mjs`'s exit code IS the verdict — it is nonzero on any FN or FP, and it prints each one
# with its anchor. No re-derivation of the score here: a second opinion about what counts as a
# regression is a second owner of the ground truth.
#
# `set +e; rc=$?; set -e` rather than `if node ...; then`: an `if` whose condition fails returns 0, so
# the status read after the `fi` would be the status of the IF and every failure would stamp `clean`.
# detection-gate-if-touched.sh carries the same note for the same reason — this is the second reader
# of an exit code in this pair and the trap is identical.
set +e
node scripts/measure/benchmark.mjs \
  --run "$work/runs/ci" \
  --expected "$expected"
bench_rc=$?
set -e

# The stamp is written from the SCORER's status and from nothing else. Everything above this line that
# can fail — the harness selftest, the release build, snapshot.mjs — leaves the `started` stamp in
# place, which is the honest record: those failures mean the key was never compared to anything.
if [ "$bench_rc" -eq 0 ]; then
  gate_stamp clean
else
  gate_stamp regressed
fi
exit "$bench_rc"
