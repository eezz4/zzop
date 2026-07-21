#!/usr/bin/env bash
# guards-wired meta-guard — fails when a scripts/check-*.sh guard is not wired into BOTH
# .githooks/pre-commit and .github/workflows/ci.yml's `guards` job. Nothing else in this repo
# mechanically enforces that a newly authored guard actually runs anywhere; without this, a guard
# can be written, committed, and quietly never invoked again.
#
# Single hardcoded exception: check-parser-fingerprint-bump.sh is RANGE-based (it diffs a
# base..head commit range — see its own header comment) and structurally cannot run from
# pre-commit, which only ever sees the working tree, not a range (see .githooks/pre-commit's own
# "Scope" comment). It is wired into .githooks/pre-push instead, plus its own CI step in
# ci.yml — both of those are what this guard checks for it, in place of pre-commit.
#
# This script wires itself the same way its siblings are wired (see .githooks/pre-commit and
# ci.yml, both updated alongside this file) rather than special-casing itself out of the
# requirement — a meta-guard that isn't itself wired in is exactly the failure mode it exists to
# catch, so it does not exempt its own name from the loop below.
#
# Also asserts scripts/lib/tracked-grep.sh exists: several check-*.sh guards source it for their
# TRACKED-file enumeration (see its own header comment), so a missing lib silently breaks every
# guard that depends on it at the `. ./scripts/lib/tracked-grep.sh` line -- worth a fast, specific
# failure here rather than each guard's own less obvious "command not found: tracked_files_matching".
# It is NOT itself wired into pre-commit/CI/pre-push the way a scripts/check-*.sh guard is: it lives
# under scripts/lib/, has no independent exit status of its own, and is only ever sourced by a real
# guard -- the `git ls-files -z -- 'scripts/check-*.sh'` glob below does not (and must not) match it.
#
# No deps beyond git + grep. Exit 1 on any violation, listing the exact (guard, missing-location)
# pairs.
set -euo pipefail
cd "$(dirname "$0")/.."

PRE_COMMIT=.githooks/pre-commit
PRE_PUSH=.githooks/pre-push
CI=.github/workflows/ci.yml
TRACKED_GREP_LIB=scripts/lib/tracked-grep.sh

RANGE_BASED_EXCEPTION="check-parser-fingerprint-bump"

missing=0
count=0

if [ ! -f "$TRACKED_GREP_LIB" ]; then
  echo "check-guards-wired: $TRACKED_GREP_LIB -- missing. Several isolation/scope guards source it" >&2
  echo "  for their TRACKED-file enumeration; without it they fail at the '. ./$TRACKED_GREP_LIB' line." >&2
  missing=1
fi

while IFS= read -r -d '' f; do
  base="$(basename "$f" .sh)"
  count=$((count + 1))

  if [ "$base" = "$RANGE_BASED_EXCEPTION" ]; then
    if ! grep -qF "scripts/${base}.sh" "$PRE_PUSH"; then
      echo "check-guards-wired: ($base, $PRE_PUSH) -- range-based guard not wired into pre-push"
      missing=1
    fi
    if ! grep -qF "scripts/${base}.sh" "$CI"; then
      echo "check-guards-wired: ($base, $CI) -- range-based guard not wired into CI"
      missing=1
    fi
    continue
  fi

  if ! grep -qE "^[[:space:]]*${base}[[:space:]]*$" "$PRE_COMMIT"; then
    echo "check-guards-wired: ($base, $PRE_COMMIT) -- not wired into pre-commit's GUARDS array"
    missing=1
  fi

  if ! grep -qF "scripts/${base}.sh" "$CI"; then
    echo "check-guards-wired: ($base, $CI) -- not wired into CI's guards job"
    missing=1
  fi
done < <(git ls-files -z -- 'scripts/check-*.sh')

if [ "$missing" -ne 0 ]; then
  echo
  echo "check-guards-wired: every scripts/check-*.sh must run in BOTH .githooks/pre-commit and"
  echo ".github/workflows/ci.yml's guards job. The one hardcoded exception is"
  echo "check-parser-fingerprint-bump.sh, which is range-based and runs from .githooks/pre-push"
  echo "plus its own CI step instead of pre-commit."
  exit 1
fi

echo "check-guards-wired: clean ($count guards checked)."
