#!/usr/bin/env bash
# check-version-relative-prose.sh — guards the "published prose makes a claim ABOUT the version, and
# nothing re-reads it when the version moves" defect class.
#
# WHY THIS EXISTS, measured: on 2026-08-11, one release after v0.30.0 shipped, three published
# sentences were false and all 31 guards were green. README.md said each release carries a SHA256SUMS
# asset "from the next release onward — releases up to and including v0.29.1 do not have one"; v0.30.0
# has one (`gh release view v0.30.0`). VERSIONING.md said v0.29.1 was "every version installable
# today". docs/ARCHITECTURE.md said the cache split "holds from the next release onward". The
# VERSIONING.md paragraph even carried its own expiry note — "written this way rather than in the
# present tense because the fix landed after the v0.29.1 tag" — so the author KNEW it needed rewriting
# at the next release and no machine held it. `check-release-version-propagation.sh` is the same fix
# one axis over (its subject is *.json version fields and release-asset URLs); markdown was outside it.
#
# 1.0 is why this is a guard and not a runbook line: it is the one release whose entire prose surface
# is keyed to the version ("pre-1.0", "0.x makes no promise"), and one copy of that surface —
# docs/NORMALIZED_AST.md — is `include_str!`-baked into the binary, so correcting it after the tag
# costs a 1.0.1.
#
# ── What it checks, three axes ────────────────────────────────────────────────────────────────────
#
# 1. ANCHORED CLAIMS MUST BE RE-READ WHEN THE VERSION MOVES. A sentence that names an explicit
#    `vX.Y.Z` boundary ("from v0.30.0 onward", "up to and including v0.29.1") is GOOD prose — it is
#    falsifiable and does not rot silently. But it still has to be re-read when the workspace version
#    changes, because the boundary it names may now be the wrong one. The registry records the version
#    those sentences were last vetted at; a workspace version that differs reds every entry. The
#    per-file line COUNT is registered too, so a new claim added to a vetted file also reds.
#
# 2. UNANCHORED DEICTICS ARE BANNED OUTRIGHT. "the next release", "installable today", "the current
#    release" name no reference frame at all, so no machine can decide whether they are true and no
#    human notices when they stop being. This axis converts an undecidable question ("is this number
#    right?") into a decidable one ("does this claim name its own reference frame?"). Exemptions carry
#    a mandatory reason AND are liveness-checked: an exemption whose text no longer appears is itself
#    a failure, so the list cannot outlive what it excuses.
#
# 3. STABILITY TOKENS EXPIRE AT 1.0. `pre-1.0` and `0.x` are claims about the version, true today and
#    false the instant MAJOR reaches 1. At 0.x this axis is SKIPPED, and it says so out loud — a
#    silent skip and a real pass print the same thing otherwise, which is this repo's most-repeated
#    failure shape.
#
# ── What it deliberately does NOT check ───────────────────────────────────────────────────────────
#
# Bare numbers in prose ("all 27 guards", "144 rules"). That was considered and rejected with a
# measurement: `scripts/check-overclaim-prose.sh:62` says "until then a 'never misses' in any of them
# left all 27 guards green" against a fleet of 31 — it LOOKS like a violation and is not, because
# "until then" anchors it to the day it was true. A claim-shape rule would have cried wolf on a
# correct sentence on its first run, and a guard that cries wolf gets an escape hatch. The anchoring
# rule above is the decidable half of that idea, and it is the only half that can be invalidation-tested.
#
# Registry: scripts/version-relative-prose.txt (format documented in that file's own header).

set -uo pipefail
cd "$(dirname "$0")/.."

. ./scripts/lib/tracked-grep.sh

REGISTRY="scripts/version-relative-prose.txt"
FAIL=0

version="$(grep -m1 '^version = "' Cargo.toml | sed 's/^version = "\(.*\)"/\1/')"
if [ -z "$version" ]; then
  echo "check-version-relative-prose: cannot read [workspace.package] version from Cargo.toml" >&2
  exit 1
fi
major="${version%%.*}"

if [ ! -f "$REGISTRY" ]; then
  echo "check-version-relative-prose: missing registry $REGISTRY" >&2
  exit 1
fi

# The prose surface. Same enumeration every sibling prose guard uses, so a new page is covered without
# editing this file.
mapfile -t PAGES < <(git ls-files -- '*.md' '*.html' | grep -vE '(^|/)(target|node_modules|\.claude)/' || true)
if [ "${#PAGES[@]}" -eq 0 ]; then
  echo "check-version-relative-prose: scanned 0 pages — the subject set collapsed, refusing to report clean" >&2
  exit 1
fi

ANCHORED='(v[0-9]+\.[0-9]+\.[0-9]+ onward|up to and including v[0-9]+\.[0-9]+\.[0-9]+|as of v[0-9]+\.[0-9]+\.[0-9]+|since v[0-9]+\.[0-9]+\.[0-9]+|from v[0-9]+\.[0-9]+\.[0-9]+)'
DEICTIC='(the next release|installable today|the current release|todays release|as of the latest release)'
STABILITY='(pre-1\.0|`0\.x`)'

# ── axis 1 ────────────────────────────────────────────────────────────────────────────────────────
# Same CR strip as the loop below, and for the same measured reason - see its comment. Written as a
# tr on the STREAM with an OCTAL escape rather than a backslash-r inside the awk program: three
# attempts to spell that escape through a shell-inside-node quoting chain produced first a literal CR
# byte in this script and then an empty regex, and both ran clean while stripping nothing.
vetted="$(tr -d '\015' < "$REGISTRY" | awk '$1 == "VETTED" { print $2 }')"
if [ -z "$vetted" ]; then
  echo "check-version-relative-prose: $REGISTRY has no VETTED line" >&2
  exit 1
fi

# `tr -d '\r'` is load-bearing, not hygiene. This tree is CRLF: the registry read as LF while it was
# an uncommitted working-tree file, then git converted it on checkout and the last field of every line
# gained a trailing CR — so `1` compared unequal to `1` and the guard reported all three files as
# drifted with the counts printed IDENTICAL on both sides. Measured here on 2026-08-11, minutes after
# this guard first went green; `scripts/measure/plant-revert.mjs` opens with the same warning.
declare -A REGISTERED=()
while read -r kind file count _rest; do
  [ "$kind" = "claim" ] || continue
  REGISTERED["$file"]="$count"
done < <(tr -d '\r' < "$REGISTRY")

anchored_total=0
declare -A ACTUAL=()
for f in "${PAGES[@]}"; do
  n="$(grep -ciE "$ANCHORED" "$f" 2>/dev/null || true)"
  [ "${n:-0}" -gt 0 ] || continue
  ACTUAL["$f"]="$n"
  anchored_total=$((anchored_total + n))
done

if [ "$anchored_total" -eq 0 ]; then
  echo "check-version-relative-prose: 0 version-anchored claims found across ${#PAGES[@]} pages —" >&2
  echo "  either the needle broke or every boundary sentence was deleted. Refusing to report clean" >&2
  echo "  on an empty subject set; if the prose genuinely lost them, delete this axis deliberately." >&2
  exit 1
fi

if [ "$version" != "$vetted" ]; then
  echo "check-version-relative-prose: workspace version is $version, registry vetted at $vetted." >&2
  echo "  Re-read these $anchored_total version-boundary sentence(s) against $version, then set VETTED:" >&2
  for f in "${!ACTUAL[@]}"; do
    grep -niE "$ANCHORED" "$f" | sed "s|^|    $f:|" >&2
  done
  FAIL=1
fi

for f in "${!ACTUAL[@]}"; do
  if [ -z "${REGISTERED[$f]+x}" ]; then
    echo "check-version-relative-prose: $f makes a version-boundary claim and is not in $REGISTRY" >&2
    grep -niE "$ANCHORED" "$f" | sed "s|^|    $f:|" >&2
    FAIL=1
  elif [ "${REGISTERED[$f]}" != "${ACTUAL[$f]}" ]; then
    echo "check-version-relative-prose: $f has ${ACTUAL[$f]} boundary claim line(s), registry says ${REGISTERED[$f]}" >&2
    FAIL=1
  fi
done
for f in "${!REGISTERED[@]}"; do
  if [ -z "${ACTUAL[$f]+x}" ]; then
    echo "check-version-relative-prose: $REGISTRY registers $f, which no longer makes any boundary claim" >&2
    echo "  (a stale registry entry vouches for prose that is gone — remove the line)" >&2
    FAIL=1
  fi
done

# ── axis 2 ────────────────────────────────────────────────────────────────────────────────────────
declare -A EXEMPT_REASON=()
while IFS= read -r line; do
  case "$line" in
    exempt\ *) ;;
    *) continue ;;
  esac
  path="$(printf '%s' "$line" | awk '{ print $2 }')"
  reason="$(printf '%s' "$line" | cut -d' ' -f3-)"
  if [ -z "$reason" ]; then
    echo "check-version-relative-prose: exemption for $path carries no reason" >&2
    FAIL=1
  fi
  EXEMPT_REASON["$path"]="$reason"
done < <(tr -d '' < "$REGISTRY")

deictic_hits=0
for f in "${PAGES[@]}"; do
  grep -qiE "$DEICTIC" "$f" 2>/dev/null || continue
  deictic_hits=$((deictic_hits + 1))
  if [ -n "${EXEMPT_REASON[$f]+x}" ]; then
    continue
  fi
  echo "check-version-relative-prose: $f uses a deictic that names no reference frame:" >&2
  grep -niE "$DEICTIC" "$f" | sed "s|^|    $f:|" >&2
  echo "  Rewrite it to name the version it means (\"from v$version onward\"), or register an exemption with a reason." >&2
  FAIL=1
done
for path in "${!EXEMPT_REASON[@]}"; do
  if ! grep -qiE "$DEICTIC" "$path" 2>/dev/null; then
    echo "check-version-relative-prose: $REGISTRY exempts $path, which no longer contains a deictic" >&2
    echo "  (an exemption that outlives what it excuses is a hole nobody is watching — remove the line)" >&2
    FAIL=1
  fi
done

# ── axis 3 ────────────────────────────────────────────────────────────────────────────────────────
if [ "$major" -ge 1 ]; then
  stability_branch="ENFORCED (workspace MAJOR=$major)"
  for f in "${PAGES[@]}"; do
    grep -qE "$STABILITY" "$f" 2>/dev/null || continue
    echo "check-version-relative-prose: $f still claims pre-1.0/0.x status at version $version:" >&2
    grep -nE "$STABILITY" "$f" | sed "s|^|    $f:|" >&2
    FAIL=1
  done
else
  stability_branch="SKIPPED (workspace MAJOR=$major — pre-1.0/0.x claims are true today)"
fi

if [ "$FAIL" -ne 0 ]; then
  echo "check-version-relative-prose: violations found." >&2
  exit 1
fi

echo "check-version-relative-prose: clean (${#PAGES[@]} pages; $anchored_total version-anchored claim line(s) vetted at $vetted; $deictic_hits deictic file(s), all registered; stability axis $stability_branch)"
