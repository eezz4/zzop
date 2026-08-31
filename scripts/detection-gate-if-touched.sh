#!/usr/bin/env bash
# detection-gate-if-touched.sh — run the labeled detection gate when, and only when, the commit being
# made can move what the analyzer finds.
#
# ## The hole this closes
# `scripts/measure/detection-gate.sh` is the only thing in this repository that compares real findings
# to the adjudicated answer key in `cases/EXPECTED.jsonc`. Until now it was reachable from exactly one
# place: `scripts/ci-local.sh`, which the batch protocol runs at most once per batch and after the work
# is done. `.githooks/pre-commit` ran 39 guards and none of them opens `cases/`. So a commit that
# changes a rule could not be refuted by the one gate written to refute it.
#
# MEASURED: the gate was red from 9b89e62 (2026-08-21) for roughly ninety commits, and three unrelated
# commits each broke exactly one expectation without anyone noticing:
#
#   9b89e62  rules/native/rules-schema/src/usage.rs began skipping relation navigators
#   0a536ba  a DSL matcher's `trigger` moved from `pagination` to `find-many`
#   36ef989  a native constant (LOOKUP_FIELD_MAX) was removed
#
# In all three the rule was right and the key was stale, and c7c52b3 re-adjudicated the key. This
# script exists so the NEXT one is caught by the commit that causes it.
#
# ## What this costs, measured rather than estimated
# Over 3e0c3ea..84f8ece (96 commits) the trigger below fires on 63 of them -- 65.6%, not the "one
# commit in five" the option was priced at when it was chosen. That earlier figure counted commits
# that touched a DSL matcher FIELD, which is the population of the fingerprint design, not of this
# path trigger. So the honest price is: about two commits in three pay the gate's wall clock on top of
# the guards. `cases/` accounts for 2 of the 63; dropping it would save 2.1% and reopen the hole
# described beside TRIGGER_RE. Narrowing by file kind was measured too and yields nothing: zero of the
# 63 fire on test or markdown files alone.
#
# ## Why a path trigger and not a fingerprint
# The cheaper design considered was a hash column over the `matcher` object of the 76 DSL-pack rules
# `EXPECTED.jsonc` names. It catches 0a536ba and 36ef989 and is structurally blind to 9b89e62, which is
# native Rust and owns no matcher json. A version constant is worse still: 36ef989 bumped one, 9b89e62
# bumped nothing. Two out of three, sold as a gate, is how a green signal becomes the reason nobody
# looks. A path trigger sees all three because all three touch `rules/`.
#
# ## WHAT THIS IS BLIND TO — read before quoting a green run
# The trigger is a path prefix, so its blind spot is exactly the paths it does not name:
#
#   * `crates/` — the engine, the IR, the cache and the summary layer. A change there can move every
#     finding in the corpus and this script will NOT arm. That is a deliberate cost decision, not an
#     oversight: `crates/` is the majority of this repository's commits, and arming on it is the
#     "unconditional" option, which prices every commit at the gate's wall clock. CI still scores every
#     push (the `detection-benchmark` job runs the same gate unconditionally), so the exposure is one
#     push, not ninety commits.
#   * `packages/` — the CLI and MCP surfaces. Same reasoning; they dispatch into crates/.
#   * Anything reached only at runtime — a config default, an environment variable.
#
# The honest one-line summary of a green run here is "the gate agrees with the key for the paths this
# commit staged", never "detection is fine".
#
# ## Which bytes decide: the INDEX
# The 39 guards in .githooks/pre-commit read the WORKING TREE on purpose. This one reads the INDEX
# (`git diff --cached`), for the same reason check-vendor-token-literals.sh grew an index pass: the
# subject is what the COMMIT carries. `git commit -a` and `git commit <paths>` both build a temporary
# index and export $GIT_INDEX_FILE, which `git diff --cached` honors, so the trigger judges the right
# bytes under all three spellings.
#
# Note the asymmetry that follows, and it is real: the gate BUILDS AND SCORES THE WORKING TREE. If
# rules/ is dirty but unstaged, the trigger stays quiet while the score would have included those
# edits. The reverse — staged rules/ plus unrelated dirty work — arms the gate and scores the dirt.
# Neither is fixable here; the gate compiles a tree, and there is only one tree. Staging is the axis
# that matches the question "does this COMMIT invalidate the key", so that is the axis used.
#
# usage:
#   bash scripts/detection-gate-if-touched.sh     # from .githooks/pre-commit, or by hand
#   ZZOP_SKIP_DETECTION_GATE=1 git commit ...     # arm, announce, and do not run
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

# The trigger set, spelled once. `cases/` is in it because the answer key and its fixture trees live
# there: c7c52b3 changed ONLY cases/EXPECTED.jsonc, and under a rules-and-parser-only trigger the
# commit that repairs the key would be the one commit that never verifies the repair.
TRIGGER_RE='^(rules|parser|cases)/'
SKIP_VAR=ZZOP_SKIP_DETECTION_GATE

# `|| true` is load-bearing and not hygiene: grep exits 1 on no match, which is an ORDINARY outcome
# here (measured: a third of commits stage nothing under these prefixes), and under `set -e` that
# status would kill the hook on the assignment with zero bytes of output.
touched="$(git diff --cached --name-only | grep -E "$TRIGGER_RE" || true)"

if [ -z "$touched" ]; then
  echo "pre-commit: detection gate not armed (no staged path under rules/, parser/ or cases/)."
  exit 0
fi

n="$(printf '%s\n' "$touched" | grep -c . || true)"

echo
echo "pre-commit: detection gate ARMED -- $n staged path(s) under rules/, parser/ or cases/:"
# `awk` rather than `head`: under `set -o pipefail` a `head` that closes the pipe early can SIGPIPE its
# producer, and this repo has already lost one guard verdict that way (see check-shell-pipe-sigpipe.sh).
# awk reads every line and prints the first eight, so there is no early close.
printf '%s\n' "$touched" | awk 'NR <= 8 { print "    " $0 }'
if [ "$n" -gt 8 ]; then
  echo "    ... and $((n - 8)) more"
fi
echo "  These are the paths that can change what the analyzer finds, so cases/EXPECTED.jsonc may no"
echo "  longer be true. scripts/measure/detection-gate.sh builds zzop-mcp --release and scores the"
echo "  labeled corpus against it."
echo "  MEASURED wall clock: 200-290s, plus 2-3 min more when the release build is cold. NOT hung."
echo "  To commit without it: $SKIP_VAR=1 git commit ...   (the key then goes unchecked)"
echo

if [ -n "${!SKIP_VAR:-}" ]; then
  echo "pre-commit: detection gate SKIPPED by $SKIP_VAR -- cases/EXPECTED.jsonc was NOT scored"
  echo "  against this commit. If a rule moved, the key is now stale and nothing here says so."
  exit 0
fi

# PREFLIGHT: a missing toolchain is not a detection failure, and must not be reported as one.
#
# MEASURED 2026-08-29, on this gate's FIRST live firing: the hook armed correctly on six staged
# rules/ paths, ran, and died after 2s with `harness-selftest: 'rustc' not found`. The message
# printed underneath was the FN/FP one below -- so the very first real use told its reader to go
# re-adjudicate cases/EXPECTED.jsonc over a PATH problem. The next reach after that message is
# ZZOP_SKIP_DETECTION_GATE=1, which would have disarmed this gate on the day it landed.
#
# Why the hook and not the shell: git runs hooks with a PATH that does not include ~/.cargo/bin on
# this machine (verified: `command -v rustc` is empty in a plain shell here; the binaries live in
# $HOME/.cargo/bin). Every script in this repository that touches cargo prepends it by hand. The
# hook did not, because its 39 guards never needed a compiler -- this stage is the first that does.
#
# The remedy is scoped: prepend the standard rustup bin directory IF the tools are missing AND that
# directory has them. It is not an unconditional PATH edit, so a deliberately pinned toolchain
# already on PATH keeps winning.
if ! command -v cargo >/dev/null 2>&1 || ! command -v "${RUSTC:-rustc}" >/dev/null 2>&1; then
  if [ -x "$HOME/.cargo/bin/cargo" ] || [ -x "$HOME/.cargo/bin/cargo.exe" ]; then
    PATH="$HOME/.cargo/bin:$PATH"
    export PATH
  fi
fi

# Still missing after that? Then this gate CANNOT SCORE, and saying so is the whole point. Exiting 0
# here would be the silent green this file exists to remove; exiting with the FN/FP text below would
# misattribute the cause. So: block, with its own message and its own name for what happened.
missing=""
command -v cargo >/dev/null 2>&1 || missing="$missing cargo"
command -v "${RUSTC:-rustc}" >/dev/null 2>&1 || missing="$missing ${RUSTC:-rustc}"
if [ -n "$missing" ]; then
  echo
  echo "pre-commit: detection gate COULD NOT RUN -- missing:$missing"
  echo "  This is an ENVIRONMENT failure, not a detection failure. Nothing was scored, so this says"
  echo "  NOTHING about whether cases/EXPECTED.jsonc is still true for this commit."
  echo "  Put the Rust toolchain on PATH (usually \$HOME/.cargo/bin) and commit again, or set \$RUSTC."
  echo "  $SKIP_VAR=1 would let this commit through, but the key would then go unchecked -- which is"
  echo "  the state that left the gate red for ninety commits. Prefer fixing PATH."
  exit 1
fi
# NOT `if bash ...; then`: an `if` whose condition fails returns 0, so `$?` after the `fi` is the
# STATUS OF THE IF, not of the gate. Reading it there would report every failure as exit 0 -- the exact
# shape of silent green this file exists to remove.
start=$SECONDS
set +e
bash scripts/measure/detection-gate.sh
rc=$?
set -e

if [ "$rc" -eq 0 ]; then
  echo "pre-commit: detection gate clean in $((SECONDS - start))s."
  exit 0
fi

echo
echo "pre-commit: detection gate FAILED after $((SECONDS - start))s."
echo "  Read the FN/FP lines above with their anchors. Two outcomes are legitimate and they look the"
echo "  same from here: the rule regressed (fix the rule), or the rule improved and the key is stale"
echo "  (re-adjudicate cases/EXPECTED.jsonc and write the reason beside the row, as c7c52b3 did)."
echo "  If the lines above name no anchors at all, this is NOT one of those two outcomes -- the gate"
echo "  broke before it scored. Read its own last line; the preflight above covers only the toolchain."
echo "  Deciding which requires reading the fixture at the anchor. Do not silence this by editing the"
echo "  key to match the output."
exit "$rc"
