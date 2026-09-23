#!/usr/bin/env bash
# require-gnu.sh — abort with the RIGHT diagnosis when a guard's GNU-only utility is not GNU.
#
# ## The defect this exists for (2026-09-13, review ledger V184)
#
# Three guards pass `-d '\n'` to `xargs`. That option is GNU-only; BSD `xargs` (the default on macOS)
# rejects it. The repo has always assumed GNU utilities and says so in its build skill — that is not
# the defect. The defect is what each guard says when the assumption is violated, because none of
# them blames the utility:
#
#   check-max-file-lines.sh      reads the pipeline status and reports "wc failed reading one or more
#                                tracked .rs files (deleted or became unreadable mid-scan)". A reader
#                                goes hunting for a deleted file that does not exist. Observed live.
#   check-cli-flags-documented.sh  swallows stderr with `2>/dev/null` and counts the empty output, so
#                                it silently answers hits=0. Measured on the same input: BSD 0, GNU 1.
#                                This one does not even go red — it goes WRONG.
#   check-docs-rule-ids.sh       ends in `|| true`, so the file list is empty and its population floor
#                                fires with "ZERO files to scan ... broken scan".
#
# Three guards, three diagnoses, none of them the truth. That is the shape this repo keeps finding:
# a failure reported as something it is not costs more than a failure reported loudly, because the
# reader spends the debugging budget in the wrong place. "The lesson is wiring, not knowledge" —
# knowing the repo needs GNU utils does not help at the moment a guard blames your working tree.
#
# Cheap by construction: one `xargs` invocation over an empty input, no subshell fan-out, run once per
# guard rather than per file.

# Abort unless `xargs` understands `-d`. Callers source this file and call it before their first use.
require_gnu_xargs() {
  if printf '' | xargs -d '\n' true 2> /dev/null; then
    return 0
  fi
  local who="${1:-this guard}"
  # printf, not echo: some shells expand backslash escapes in echo, and this message has to SHOW the
  # option spelling rather than act on it. The first draft printed a real newline mid-sentence.
  printf '%s\n' "$who: this script needs GNU xargs (it passes the -d option), and the xargs on PATH is not GNU." >&2
  printf '%s\n' "  $(command -v xargs 2> /dev/null || echo 'xargs') rejected it." >&2
  echo "" >&2
  echo "  This is an ENVIRONMENT failure, not a finding about the repo. Without this check the same" >&2
  echo "  condition surfaces as a claim about your files -- a deleted source, an empty scan, or a" >&2
  echo "  silently wrong count -- and the debugging goes to the wrong place." >&2
  echo "" >&2
  echo "  macOS: 'brew install findutils' and put its gnubin first on PATH. The prelude that does" >&2
  echo "  this for the whole fleet is in this repo's build skill; CI runs Ubuntu and is unaffected." >&2
  exit 1
}
