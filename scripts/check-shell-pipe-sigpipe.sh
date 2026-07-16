#!/usr/bin/env bash
# Machine seal for the pipefail+SIGPIPE guard-killer class (bit for real 2026-07-17): under
# `set -o pipefail`, `<producer> | grep -q` lets grep exit on the FIRST match; if the producer
# still has more than a pipe buffer (~64KB) left to write, it dies with SIGPIPE (exit 141) and
# the pipeline — despite a REAL match — evaluates as failure. In a guard that inverts the guard:
# check-parser-fingerprint-bump rejected a present [no-projection-change] marker because the
# commit-message blob it printf'd was 79KB, and sibling `... | grep -q || collect` sites had the
# opposite failure mode (a real mismatch could read as a match, silently passing drift).
#
# Sealed rule: no `| grep -q` (or -qxF/--quiet/--silent) pipeline anywhere in scripts/*.sh.
# Safe equivalents, all used by the 2026-07-17 sweep:
#   grep -q <pattern> <<< "$var"             # herestring — no writer process, nothing to SIGPIPE
#   grep -q <pattern> <file>                 # direct file input
#   <producer> | grep <pattern> >/dev/null   # grep consumes ALL input; producer never SIGPIPEs
#
# Detection scope: code lines only — full-line comments are skipped (the fixed scripts document
# the class in comments). Known-uncovered: a `grep -q` whose pipe input arrives via a variable
# holding the command (indirection no grep can see) — no scanned script writes that.
set -euo pipefail
cd "$(dirname "$0")/.."

self="$(basename "${BASH_SOURCE[0]}")"
fail=0
for f in scripts/*.sh; do
  [ "$(basename "$f")" = "$self" ] && continue
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
echo "check-shell-pipe-sigpipe: OK (no '| grep -q' pipelines in scripts/*.sh)"
