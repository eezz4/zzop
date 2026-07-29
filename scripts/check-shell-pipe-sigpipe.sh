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
#   * `.github/workflows/*.yml` — `run:` blocks are shell, but a line-based scan there would read YAML
#     strings, folded scalars and comments as code; the guards those blocks invoke are themselves
#     scanned, which is where the pipelines actually live.
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

self="scripts/$(basename "${BASH_SOURCE[0]}")"

# One `git ls-files` pair, one `grep -v`, one `awk` over the whole list — four process spawns total,
# not two per file. That is not micro-optimisation: the pre-commit fleet is dominated by process
# spawns on Windows msys (~0.7s each), and the per-file loop this replaces paid a `basename` plus an
# `awk` for every scanned file.
#
# `--others --exclude-standard` never lists a git-ignored path, so `target/`/`node_modules/` can only
# arrive here by being TRACKED; the filter below is the belt for that case (same reasoning as
# scripts/lib/tracked-grep.sh's standard exclusions).
files="$(
  {
    git ls-files -- '*.sh' '.githooks/*'
    git ls-files --others --exclude-standard -- '*.sh' '.githooks/*'
  } | sort -u | grep -vE '(^|/)(target|node_modules)/' || true
)"

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
    [ "$f" = "$self" ] && continue   # this file documents the class in code-shaped prose
    files_arr+=("$f")
    scanned=$((scanned + 1))
  done <<< "$files"
fi

if [ "$scanned" -eq 0 ]; then
  echo "check-shell-pipe-sigpipe: FAILED -- enumerated ZERO shell files. 'git ls-files -- \"*.sh\"" >&2
  echo "  \".githooks/*\"' returned nothing, so this guard would have vouched for nothing. Either this" >&2
  echo "  is not a git work tree, or the repo genuinely has no shell left; neither is a clean run." >&2
  exit 1
fi

fail=0
hits="$(awk '
  /^[[:space:]]*#/ { next }
  /\|[[:space:]]*grep([[:space:]]+(-[A-Za-z]+|--[a-z][a-z-]+))*[[:space:]]+(-[A-Za-z]*q[A-Za-z]*|--quiet|--silent)([[:space:]]|$)/ {
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
echo "check-shell-pipe-sigpipe: OK (no '| grep -q' pipelines in $scanned shell files: every git-known *.sh plus .githooks/*)"
