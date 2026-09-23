#!/usr/bin/env bash
# detection-gate-staleness.sh — print what the detection gate last ANSWERED, and how far behind that
# answer is. Runs on every commit, armed or not.
#
# ## The defect this exists for
#
# MEASURED, and it is the open half of the entry this closes: `scripts/measure/detection-gate.sh` was
# red from 9b89e62 (2026-08-21) for roughly ninety commits and nobody noticed. The three causes were
# unrelated and every one of them was the rule being RIGHT and `cases/EXPECTED.jsonc` being stale.
#
# `scripts/detection-gate-if-touched.sh` (2026-08-29) closed the first half: a commit that stages a
# path able to move a finding now runs the gate. What it cannot close is the half that made the eight
# days INVISIBLE rather than merely uncaught — a red run left nothing behind. The verdict existed in
# one terminal and nowhere else, so the next commit printed "detection gate not armed", exited 0, and
# so did every commit after it. On screen, silence after a red and silence after no run are identical.
#
# That is a failure of the SIGNAL, not of the thing it measures. This script and the stamp written by
# detection-gate.sh are the two halves of the signal: the gate records what it answered (including
# `regressed`), and this reads that record out loud at every commit.
#
# ## Why it discloses and does not refuse
#
# Two reasons, both with precedent in this tree rather than taste.
#
#   1. `scripts/unseen-commits.sh` faced the identical choice and wrote down why it prints a number:
#      a per-commit refusal for something a commit cannot fix "would be skipped by habit within a day,
#      and a skipped gate reports the same green as a passing one". A remembered red is exactly that
#      shape — you cannot clear it by declining to commit, you clear it by running the gate.
#   2. Refusal is already owned, and owned better, one layer up. When the commit stages a path that
#      can move a finding, detection-gate-if-touched.sh RUNS the gate and blocks on the live verdict.
#      A second refuser reading a cached one would block on yesterday's answer.
#
# So: exit 0 always. The output is the product.
#
# ## What it deliberately does not say
#
# It never prints "green". A `clean` stamp says the gate agreed with the key AT THAT SHA, for the
# paths that sha carried — not that detection is fine now, and not that the working tree is clean.
# Both caveats are printed rather than assumed, for the reason detection-gate-if-touched.sh states in
# its own header: the honest one-line summary of a green gate run is never "detection is fine".
#
# usage:
#   bash scripts/detection-gate-staleness.sh    # from .githooks/pre-commit, or by hand
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

# Shared with detection-gate-if-touched.sh so the "how many commits could have moved a finding"
# count below is about the same path set that arms the gate. See the lib header for why one copy.
. ./scripts/lib/detection-trigger.sh

STAMP=".zzop/detection-gate-last-run"

# The floor case, and it is reported as a floor rather than as a zero. `unseen-commits.sh` states the
# rule this follows: with nothing to read, the honest answer is "cannot tell", never 0 — a silent 0
# reads as "everything has been scored", which is the opposite of the truth. `.zzop/` is gitignored
# derived state, so a fresh clone, a cleaned tree, or a checkout that has never run the gate all land
# here, and all three mean the same thing: no measurement exists.
if [ ! -s "$STAMP" ]; then
  echo "pre-commit: the detection gate has recorded NO run in this clone ($STAMP is missing or empty)."
  echo "  That is not 'clean' -- it is 'no measurement'. cases/EXPECTED.jsonc has not been compared to"
  echo "  anything here. Run:  bash scripts/measure/detection-gate.sh"
  exit 0
fi

# Read the four fields written by detection-gate.sh's gate_stamp(). `read` from a file rather than a
# pipeline so this cannot be the mute-failure shape check-shell-mute-floor.sh exists for: there is no
# command substitution here whose nonzero status could kill the script before it can explain itself.
verdict=""
sha=""
epoch=""
iso=""
read -r verdict sha epoch iso _rest < "$STAMP" || true

# A stamp this script cannot parse is a stamp that says nothing, and saying nothing is the state this
# whole file exists to remove -- so it is reported as loudly as a red would be, not skipped.
case "$verdict" in
  started | clean | regressed) ;;
  *)
    echo "pre-commit: $STAMP is present but unreadable (first field: '${verdict:-<empty>}')."
    echo "  Treat that as 'no measurement', not as clean. Re-run: bash scripts/measure/detection-gate.sh"
    exit 0
    ;;
esac

# Age in whole days. `date -u +%s` only -- `date -d` is a GNU extension and this script's main caller
# is `.githooks/pre-commit`, which git runs with the system PATH: on macOS that is the BSD userland,
# where `date -d` does not exist. That is why detection-gate.sh stores the epoch instead of leaving
# the reader to parse the ISO field. Arithmetic is shell builtin, so nothing here can fail on a
# non-numeric field except by producing a wrong number -- which the guard below rejects.
now="$(date -u +%s)"
days="?"
case "$epoch" in
  '' | *[!0-9]*) ;;
  *) days="$(((now - epoch) / 86400))" ;;
esac

# How many commits, and how many of those could have moved a finding. The second number is the one
# that matters: "the gate last scored 40 commits ago" is mush if none of the 40 touched a rule, and it
# is the ninety-commit shape if most of them did.
#
# `git cat-file -e` first: the stamped sha can be absent from this clone after a rebase, an amend that
# rewrote it, or a branch switch. "Cannot tell how far behind" is a real answer and gets said; a count
# against a missing object would be an invented one.
commits="?"
armed_commits="?"
if git cat-file -e "${sha}^{commit}" 2> /dev/null; then
  set +e
  commits="$(git rev-list --count "$sha..HEAD" 2> /dev/null)"
  rc=$?
  set -e
  [ "$rc" -eq 0 ] && [ -n "$commits" ] || commits="?"

  if [ "$commits" != "?" ]; then
    # One `git log` walk, parsed by awk, rather than a `git show` per commit: the range is routinely
    # dozens of commits and this runs on every commit made in the repository.
    #
    # No early exit in the awk program and no `head` in the pipeline. awk closing the pipe early sends
    # `git log` SIGPIPE, and under `set -o pipefail` that failure becomes the status of the whole
    # pipeline -- check-shell-pipe-sigpipe.sh exists because this repository has already lost a guard
    # verdict to exactly that. awk reads every line.
    set +e
    armed_commits="$(git log --format='%x01' --name-only "$sha..HEAD" 2> /dev/null |
      awk -v re="$DETECTION_TRIGGER_RE" '
        /^\001$/ { if (hit) n++; hit = 0; next }
        $0 ~ re  { hit = 1 }
        END      { if (hit) n++; print n + 0 }
      ')"
    rc=$?
    set -e
    [ "$rc" -eq 0 ] && [ -n "$armed_commits" ] || armed_commits="?"
  fi
fi

short="$(git rev-parse --short "$sha" 2> /dev/null || printf '%s' "$sha")"

behind="$commits commit(s) ago"
[ "$commits" = "0" ] && behind="at HEAD"
[ "$commits" = "?" ] && behind="an unknown distance back (that sha is not in this clone)"

case "$verdict" in
  regressed)
    echo
    echo "pre-commit: the LAST detection gate run FAILED -- $short, $behind, ${days}d ago ($iso)."
    echo "  cases/EXPECTED.jsonc did not match what the analyzer found, and NOTHING since has scored it."
    echo "  This line is here because that state once lasted ninety commits without being printed once."
    echo "  Re-run it and read the FN/FP anchors:  bash scripts/measure/detection-gate.sh"
    echo
    ;;
  started)
    echo
    echo "pre-commit: the LAST detection gate run reported NO verdict -- $short, $behind, ${days}d ago."
    echo "  It started and did not reach the scorer: killed, a failed harness selftest, a failed release"
    echo "  build -- or it is running right now in another shell. Either way the key went uncompared."
    echo "  Run:  bash scripts/measure/detection-gate.sh"
    echo
    ;;
  clean)
    echo "pre-commit: detection gate last scored $short ($behind, ${days}d ago) and the key held."
    if [ "$armed_commits" != "?" ] && [ "$armed_commits" != "0" ]; then
      echo "  Since then $armed_commits of those commits touched a path that can move a finding, and none of"
      echo "  them has been scored against the key. Run: bash scripts/measure/detection-gate.sh"
    fi
    echo "  Not a claim that detection is fine: a stamp covers the sha it names, not this working tree,"
    echo "  and not the paths that run never reached."
    ;;
esac

exit 0
