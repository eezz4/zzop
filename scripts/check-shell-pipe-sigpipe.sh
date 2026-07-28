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
# ## File scope, widened 2026-07-26
# This used to read `scripts/*.sh` only, while the seal above claimed the rule held "anywhere". Two
# populations sat outside that glob and inside the hazard:
#   * scripts/lib/*.sh — sourced by five or more guards and made almost entirely of grep pipelines.
#     A `| grep -q` there SIGPIPEs on behalf of every caller at once, and the caller cannot see it.
#   * .githooks/* — pre-commit and pre-push, which run on every commit and every push. A flipped
#     verdict there does not fail one guard, it turns the whole local gate off or on wrongly.
# No live violation existed in either at the time of widening (measured), so this closes the gap
# between the seal and its enforcement rather than fixing a bug — but the seal was the thing that
# was wrong, and a claim wider than its check is how the next one gets missed. `.githooks/*` is
# enumerated as "every file in the directory", not a hardcoded pair, so a new hook is in scope the
# moment it exists; non-regular entries are skipped.
#
# Deliberately out of scope: scripts/measure/*.mjs (JavaScript — no shell pipelines, and Node has no
# `set -o pipefail` semantics to invert) and .github/workflows/*.yml (`run:` blocks are shell, but a
# line-based scan there would read YAML strings, folded scalars and comments as code; the guards those
# blocks invoke are themselves scanned, which is where the pipelines actually live). Both exclusions
# are for lack of a hazard, not for lack of a scanner.
#
# Detection scope: code lines only — full-line comments are skipped (the fixed scripts document
# the class in comments). Known-uncovered: a `grep -q` whose pipe input arrives via a variable
# holding the command (indirection no grep can see) — no scanned script writes that.
set -euo pipefail
cd "$(dirname "$0")/.."

self="$(basename "${BASH_SOURCE[0]}")"
fail=0
scanned=0
shopt -s nullglob
for f in scripts/*.sh scripts/lib/*.sh .githooks/*; do
  [ -f "$f" ] || continue
  [ "$(basename "$f")" = "$self" ] && continue
  scanned=$((scanned + 1))
  hits="$(awk '
    /^[[:space:]]*#/ { next }
    /\|[[:space:]]*grep([[:space:]]+(-[A-Za-z]+|--[a-z][a-z-]+))*[[:space:]]+(-[A-Za-z]*q[A-Za-z]*|--quiet|--silent)([[:space:]]|$)/ {
      print FILENAME ":" FNR ": " $0
    }
  ' "$f")"
  if [ -n "$hits" ]; then
    printf '%s\n' "$hits" >&2
    fail=1
  fi
done

if [ "$fail" -ne 0 ]; then
  echo "check-shell-pipe-sigpipe: FAILED — '| grep -q' under pipefail SIGPIPEs the producer on large input," >&2
  echo "  flipping the pipeline's verdict. Use a herestring (grep -q ... <<< \"\$var\"), direct file input," >&2
  echo "  or '| grep ... >/dev/null' (grep then consumes all input). See this script's header." >&2
  exit 1
fi
# Census + its assertion: the file list is three globs, and a glob that stops matching turns this
# guard into a no-op that still prints OK.
if [ "$scanned" -eq 0 ]; then
  echo "check-shell-pipe-sigpipe: scanned 0 files — the scripts/*.sh, scripts/lib/*.sh and .githooks/*" >&2
  echo "  globs matched nothing, so this guard vouched for nothing. Fix the enumeration." >&2
  exit 1
fi
echo "check-shell-pipe-sigpipe: OK (no '| grep -q' pipelines in $scanned files: scripts/*.sh, scripts/lib/*.sh, .githooks/*)"
