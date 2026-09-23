#!/usr/bin/env bash
# check-ci-job-timeouts.sh — every CI job must declare a `timeout-minutes` ceiling.
#
# ## The defect this exists for (2026-09-06, review ledger V27)
#
# The workflow had no `timeout-minutes` anywhere, so every job inherited GitHub's default of 360
# minutes. A hang — the failure shape an external review named for the parser property tests, which
# assert "does not panic" and therefore cannot see "does not terminate" — would have burned six
# runner-hours per job and then reported `cancelled`, which reads like infrastructure flaking rather
# than a defect in this repo.
#
# The ceiling is not a performance budget and must not be tuned like one. It is the difference between
# a red run with a job name on it and six hours of nothing. Each declaration in `ci.yml` carries the
# measurement it was sized against, generously, so raising one is a decision someone makes on purpose.
#
# ## Why this is a text scan and not a YAML parse
#
# The fleet has no YAML dependency and adding one to assert the presence of a key would be a heavier
# tool than the question. A job header in this file is `^  <name>:$` and a job-level key is exactly
# four spaces deep — a shape the repo already relies on (`check-guards-wired.sh` reads the same file
# the same way). The cost is stated rather than hidden: a job written with different indentation would
# be invisible here, which is why the guard prints the roster it found rather than only its verdict.
# A reader who does not see their job in that list has found this guard's blind spot, not a pass.
#
# ## Population: EVERY workflow, not just ci.yml (2026-09-13, review ledger V187)
#
# This guard read `ci.yml` alone while printing "every job has a hang ceiling". At that moment
# `prebuild.yml` had SEVEN untimed jobs and `pages.yml` one -- and prebuild is the lane that builds
# and publishes the bytes a user installs, so the job that can hang for six hours unnamed was the one
# with the most at stake. The sentence was true of the file it read and false of the repo.
#
# The roster is now the directory: every `.github/workflows/*.yml` with a top-level `jobs:` block.
# Adding a workflow therefore cannot escape this guard by existing, which is the failure mode a
# hardcoded filename guarantees.
#
# ## The one exception, and why it is not a hole
#
# A job that calls a REUSABLE workflow (`uses: ./.github/workflows/....yml` at job level) cannot
# carry `timeout-minutes` -- GitHub rejects the key there. Its ceiling lives in the called
# workflow's own jobs, which this guard checks directly. Such jobs are SKIPPED and named in the
# output, never silently excluded: an exception nobody can see is the shape review ledger V177 was
# about. If the called workflow's jobs are untimed, this guard fails on THEM.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

shopt -s nullglob
WORKFLOWS=(.github/workflows/*.yml)
if [ "${#WORKFLOWS[@]}" -eq 0 ]; then
  echo "check-ci-job-timeouts: no workflow files under .github/workflows -- re-anchor this guard." >&2
  echo "  A guard that reads an empty roster reports green over a repo it never looked at." >&2
  exit 1
fi

missing=""
found=""
skipped=""

for WF in "${WORKFLOWS[@]}"; do
  # `jobs:` opens the only block whose two-space keys are job names; `on:`/`env:` keys above it are not.
  jobs_line="$(awk '/^jobs:$/ { print NR; exit }' "$WF")"
  # A workflow with no top-level `jobs:` is not a shape error -- reusable-workflow fragments and
  # composite actions live here too. Only a workflow that HAS jobs owes ceilings.
  [ -n "$jobs_line" ] || continue

  current=""
  has_timeout=0
  is_call=0
  wf_found=""

  close_job() {
    [ -n "$current" ] || return 0
    if [ "$is_call" -eq 1 ]; then
      skipped="$skipped $(basename "$WF")/$current"
    elif [ "$has_timeout" -eq 0 ]; then
      missing="$missing $(basename "$WF")/$current"
    fi
  }

  # `tr -d` strips CR once for the whole stream rather than per line. This file is CRLF in a Windows
  # checkout, and the first draft of this guard matched nothing there and landed in the "found no jobs"
  # branch below -- which is the only reason that branch exists, and the reason it is an ERROR and not a
  # pass. A guard that reads a file SHAPE has to assume the file arrives in the other line ending.
  while IFS= read -r line; do
    case "$line" in
    "  "[a-z]*":")
      close_job
      current="${line#  }"
      current="${current%:}"
      wf_found="$wf_found $current"
      has_timeout=0
      is_call=0
      ;;
    "    timeout-minutes: "*)
      has_timeout=1
      ;;
    "    uses: "*)
      # A job calling a REUSABLE workflow cannot carry timeout-minutes -- GitHub rejects the key.
      # Its ceiling lives in the called workflow's own jobs, which this guard reads directly.
      is_call=1
      ;;
    esac
  done < <(tail -n "+$jobs_line" "$WF" | tr -d '\r')
  close_job

  if [ -z "$wf_found" ]; then
    echo "check-ci-job-timeouts: $WF has a 'jobs:' block but no jobs under it -- the shape this" >&2
    echo "  guard reads has changed, and a guard that sees nothing reports green. Re-anchor it." >&2
    exit 1
  fi
  found="$found $(basename "$WF"):$wf_found"
done

if [ -z "$found" ]; then
  echo "check-ci-job-timeouts: found no jobs at all across ${#WORKFLOWS[@]} workflow file(s) -- the" >&2
  echo "  shape this guard reads has changed, and a guard that sees nothing reports green." >&2
  exit 1
fi

echo "check-ci-job-timeouts: jobs read ->$found"
[ -n "$skipped" ] && echo "check-ci-job-timeouts: reusable-workflow call(s), ceiling lives in the callee ->$skipped"

if [ -n "$missing" ]; then
  echo "" >&2
  echo "check-ci-job-timeouts: no timeout-minutes on:$missing" >&2
  echo "" >&2
  echo "Without one a job inherits GitHub's 360-minute default, so a hang burns six runner-hours and" >&2
  echo "reports 'cancelled' instead of naming what hung. Add a job-level ceiling sized generously" >&2
  echo "against a measurement, with that measurement written beside it -- the siblings show the shape." >&2
  exit 1
fi

echo "check-ci-job-timeouts: every job in every workflow has a hang ceiling."
