#!/usr/bin/env bash
# Machine seal for the pipefail+SIGPIPE guard-killer class (bit for real 2026-07-17): under
# `set -o pipefail`, `<producer> | grep -q` lets grep exit on the FIRST match; if the producer
# still has more than a pipe buffer (~64KB) left to write, it dies with SIGPIPE (exit 141) and
# the pipeline — despite a REAL match — evaluates as failure. In a guard that inverts the guard:
# check-parser-fingerprint-bump rejected a present [no-projection-change] marker because the
# commit-message blob it printf'd was 79KB, and sibling `... | grep -q || collect` sites had the
# opposite failure mode (a real mismatch could read as a match, silently passing drift).
#
# Sealed rule: no `| grep -q` (or -qxF/--quiet/--silent) pipeline in ANY of this repo's shell.
# Safe equivalents, all used by the 2026-07-17 sweep:
#   grep -q <pattern> <<< "$var"             # herestring — no writer process, nothing to SIGPIPE
#                                            # ...but ONLY while "$var" stays under 64 KiB: bash
#                                            # writes a herestring into a pipe before exec'ing the
#                                            # reader, so 65,536+ bytes deadlock the script dead
#                                            # silently (measured 2026-09-08, review ledger V109).
#                                            # For content that scales with the repo, use
#                                            # < <(printf '%s\n' "$var") — same SIGPIPE-free
#                                            # property, no ceiling. check-shell-herestring-scale.sh
#                                            # enforces that split.
#   grep -q <pattern> <file>                 # direct file input
#   <producer> | grep <pattern> >/dev/null   # grep consumes ALL input; producer never SIGPIPEs
#
# ## File scope: DERIVED from git, not a glob list (2026-07-29)
# This used to read three literal globs — `scripts/*.sh scripts/lib/*.sh .githooks/*` — while the seal
# above claimed the rule held in "ANY of this repo's shell". Those two statements were not the same
# statement, and the difference was measured: four tracked `.sh` files sat outside the globs and inside
# the hazard —
#     .claude-plugin/hooks/bootstrap.sh
#     docs/demo/break-a-route.sh
#     scripts/measure/detection-gate.sh      <- a gate CI actually invokes
#     scripts/measure/harness-selftest.sh
# — and a planted `printf ... | grep -q` in a fifth file under docs/ made this guard print
# "OK (no '| grep -q' pipelines in 26 files)". Exit 0, violation live, in a guard whose entire subject
# is verdicts that silently invert.
#
# The 2026-07-26 widening (scripts/lib + .githooks) is the same lesson arriving one iteration earlier
# and being answered the wrong way: by extending the list. A hand list inside the guard's own file
# cannot see the shipped set grow — its green means "the files I remembered are clean", never "this
# repo's shell is clean". So the subject set is now derived: everything git knows about matching
# `*.sh`, plus every file under `.githooks/` (hooks carry no extension). Tracked AND
# untracked-but-not-ignored, the same scope check-english-source.sh settled on: a fresh script must be
# caught before its first `git add`, not from the moment it becomes tracked.
#
# Deliberately out of scope, and these are exclusions of a HAZARD, not of a scanner — each is a claim
# that no `| grep -q` can exist there, not that looking would be inconvenient:
#   * `*.mjs` / `*.js` (JavaScript — no shell pipelines, and Node has no `set -o pipefail` semantics
#     to invert).
#   * ~~`.github/workflows/*.yml`~~ — IN SCOPE since 2026-08-08. The old exclusion rested on a factual
#     claim — "the guards those blocks invoke are themselves scanned, which is where the pipelines
#     actually live" — and that claim was false when written. `prebuild.yml`'s MCP-registry publish
#     step ran `printf '%s' "$out" | grep -q 'cannot publish duplicate version'` two lines under its
#     own `set -uo pipefail`: the sealed pattern, in a `run:` block, invoking no guard. Its failure
#     mode is this class at its worst — a chatty publisher makes the pipeline report failure on a REAL
#     match, so the duplicate-version forgiveness (added so a re-run could succeed) would refuse to
#     forgive exactly when re-run matters. Fixed to a herestring in the same commit that widened this.
#     The old exclusion's OTHER half — that a line scan reads YAML strings and folded scalars as code —
#     is still true, and is accepted deliberately: those misreads are FALSE POSITIVES, which are loud
#     and take one edit to resolve, whereas the blind spot they were avoiding is a silent inversion in
#     the exact class this guard exists to seal. Measured before widening: zero hits repo-wide, so the
#     trade cost nothing today. Comment lines are skipped as everywhere else, and YAML shares `#` with
#     shell, so a documented example in a workflow comment does not trip it (verified by planting one).
#   * `target/` and `node_modules/` path segments, matching scripts/lib/tracked-grep.sh's standard
#     exclusions: build output and vendored dependencies are not this repo's shell.
# A shell script with neither a `.sh` name nor a home in `.githooks/` is the one residual (a bare
# `#!/bin/sh` file named without an extension, outside the hooks dir). Nothing in the tree is spelled
# that way today; closing it would mean reading every tracked file's first line, which is a whole-repo
# read for a population of zero. Named here so it is a known residual and not a silent one.
#
# Detection scope: code lines only — full-line comments are skipped (the fixed scripts document
# the class in comments). Known-uncovered: a `grep -q` whose pipe input arrives via a variable
# holding the command (indirection no grep can see) — no scanned script writes that.
set -euo pipefail
cd "$(dirname "$0")/.."

# 🔴 No self-exemption (removed 2026-09-13, review ledger V188). This guard used to skip its own
# file, reasoning that it "documents the class in code-shaped prose". Measured: the file contains the
# token 13 times and the guard is GREEN over itself without the skip — the needle matches a PIPELINE,
# and every occurrence here is inside a pattern string or a comment. So the exemption protected
# nothing and only stood ready to excuse a real one.
#
# If the needle is ever loosened enough to match this file's prose, this guard will say so loudly
# instead of quietly not looking. That is the trade taken on purpose: a red that names the problem
# beats a silence nobody can audit.

# One `git ls-files` pair, one `grep -v`, one `awk` over the whole list — four process spawns total,
# not two per file. That is not micro-optimisation: the pre-commit fleet is dominated by process
# spawns on Windows msys (~0.7s each), and the per-file loop this replaces paid a `basename` plus an
# `awk` for every scanned file.
#
# `--others --exclude-standard` never lists a git-ignored path, so `target/`/`node_modules/` can only
# arrive here by being TRACKED; the filter below is the belt for that case (same reasoning as
# scripts/lib/tracked-grep.sh's standard exclusions).
# The population is `scripts/lib/shell-subjects.sh`'s, not this file's. It used to be spelled here,
# and the two sibling shell guards each spelled their own — three lists, three different answers, and
# the gap between them hid a live here-string in a workflow (see that file's header).
. "$(dirname "$0")/lib/shell-subjects.sh"
files="$(shell_subject_files)"

# ## Empty-enumeration floor
# Declared BEFORE awk runs, and that ordering is the point rather than a style choice: `awk 'prog'`
# with no file operands READS STDIN, so an empty list would not report an empty scan — it would hang
# (or, at EOF, certify silence). check-policy-census.sh's header records the same failure from the
# same cause, and check-docs-link-graph.sh shipped it. A derived enumeration that comes back empty
# must abort loudly; a scan root pointing at nothing prints the same "clean" as a genuinely clean tree.
scanned=0
files_arr=()
if [ -n "$files" ]; then
  while IFS= read -r f; do
    [ -f "$f" ] || continue          # index may list a file deleted in the working tree
    files_arr+=("$f")
    scanned=$((scanned + 1))
  done < <(printf '%s\n' "$files")
fi

# NO EMPTY FLOOR HERE ANY MORE, and its absence is deliberate (2026-09-13, ledger V212).
# `shell_subject_files` aborts on a per-pathspec zero before this file sees a list, so the floor that
# stood here could not fire — and it went on printing a `git ls-files` command this guard no longer
# runs. A floor that cannot fire is not caution, it is a second owner of the population's integrity,
# stating it in terms that stopped being true. The floor lives with the list.

fail=0
hits="$(awk '
  /^[[:space:]]*#/ { next }
  # Three widenings, all measured green before them (2026-09-13, ledger V209):
  #   `command grep -q` / `\grep -q` — a wrapper or a quoting escape in front of the name;
  #   `grep -e foo -q`                — the quiet flag AFTER an operand, which the old needle required
  #                                      to come first;
  #   `grep --quiet` spelled anywhere in the argument run.
  # The subject is "a reader that exits early", and none of those three change that.
  /\|[[:space:]]*(command[[:space:]]+|\\)?grep([[:space:]]+([^|;&]*))?[[:space:]](-[A-Za-z]*q[A-Za-z]*|--quiet|--silent)([[:space:]]|$)/ {
    print FILENAME ":" FNR ": " $0
  }
' "${files_arr[@]}")"
if [ -n "$hits" ]; then
  printf '%s\n' "$hits" >&2
  fail=1
fi

if [ "$fail" -ne 0 ]; then
  echo "check-shell-pipe-sigpipe: FAILED — '| grep -q' under pipefail SIGPIPEs the producer on large input," >&2
  echo "  flipping the pipeline's verdict. Use a herestring (grep -q ... <<< \"\$var\"), direct file input," >&2
  echo "  or '| grep ... >/dev/null' (grep then consumes all input). See this script's header." >&2
  exit 1
fi
echo "check-shell-pipe-sigpipe: OK (no '| grep -q' pipelines in $scanned files -- the shared shell population, scripts/lib/shell-subjects.sh)"
