#!/usr/bin/env bash
# plant-revert-recover.sh <backup-prefix> — the LAST-RESORT half of `plant-revert.mjs`.
#
# That module restores its targets from the bytes it read itself: in a `finally`, and on
# SIGINT/SIGTERM/SIGHUP. Neither reaches a process that is killed outright (SIGKILL, a `process.abort()`,
# a machine losing power), and a tracked file left mutated on someone's working tree is the worst thing
# this machinery could leave behind. So the module also drops, for every target it reserves, a pair:
#
#   <prefix>.<n>        the target's pre-plant bytes
#   <prefix>.<n>.path   the target's path
#
# and deletes that pair only once it has PROVEN that target back. Anything still here therefore means a
# planting process died mid-probe, and this script is what puts those files back.
#
# ## Why the paths are read from the sidecars and not spelled here
# One owner for "which files are probe targets". A list spelled in a shell wrapper is a second copy that
# goes stale the day a caller plants a different file — and it would go stale silently, because a
# recovery nobody needed today looks identical to a recovery that would not have worked.
#
# ## Why this is a script and not a function inside self-analysis-gate.sh
# It was inline there until 2026-08-02. `plant-revert-selftest.mjs` proves the abort path by killing a
# child with `process.abort()` and then recovering it, so a second caller exists — and two copies of a
# recovery loop is one copy more than this repo tolerates.
#
#   bash scripts/measure/plant-revert-recover.sh "$backup"
#
# Exits 0 whether or not there was anything to recover: nothing to recover is the normal case, and this
# runs from an EXIT trap where a nonzero status would mask the real verdict. What it recovered goes to
# stderr, loudly, because a recovery that happened is news.
set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "usage: bash scripts/measure/plant-revert-recover.sh <backup-prefix>" >&2
  echo "  <backup-prefix> is the same value passed to plant-revert.mjs as its \`backup\` option." >&2
  exit 2
fi

backup="$1"

# A Windows-style prefix is REFUSED rather than tried. Measured 2026-08-02 while writing the selftest:
# with `C:\Users\...\sidecar` the glob below matches nothing, the loop is a silent no-op, and this
# script exits 0 having recovered a file it left mutated — the "guard is green, nothing was verified"
# shape this whole module exists to close, in the one place that has no second line of defence. Callers
# on Windows already have a POSIX prefix in hand (`mktemp -d`); a caller that built one with
# `path.join` must hand over its forward-slash spelling.
case "$backup" in
  *\\*)
    echo "plant-revert-recover: the backup prefix contains a BACKSLASH: $backup" >&2
    echo "  Backslash paths do not survive this script's glob, so it would silently recover NOTHING" >&2
    echo "  and exit 0. Pass the same prefix with forward slashes." >&2
    exit 2
    ;;
esac

for path_file in "$backup".*.path; do
  # A glob that matches nothing expands to itself; both tests below then fail and the loop is a no-op.
  [ -f "$path_file" ] || continue
  bytes="${path_file%.path}"
  [ -f "$bytes" ] || continue
  target="$(cat "$path_file")"
  if [ -z "$target" ]; then
    echo "plant-revert-recover: $path_file is EMPTY — cannot tell which file $bytes belongs to." >&2
    echo "  Those bytes are the only copy of something's pre-plant state; do not delete them." >&2
    exit 1
  fi
  cp "$bytes" "$target"
  rm -f "$bytes" "$path_file"
  echo "plant-revert-recover: RECOVERED $target from its pre-plant bytes (the planting process died" >&2
  echo "  mid-probe, so neither its \`finally\` nor its signal handlers ran)." >&2
done
