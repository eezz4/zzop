#!/usr/bin/env bash
# Mechanical census of every policy-shaped constant under the crates that hold rule/extraction logic
# (crates/engine/src, crates/core/src, parser/*/src, rules/native/*/src). This is the "continuous
# drift review" mechanism (A5): the census tracks EXISTENCE (path:CONST_NAME), never values — values
# change legitimately and are not what this guard is for. Its job is to force a triage moment (tier
# T1/T2/T3, or "not policy") every time a *new* policy-shaped constant is introduced, by failing CI
# until the committed snapshot (scripts/policy-census.txt) is regenerated to include it.
#
# Regex is intentionally narrow: `^\s*(pub[(vis)] )?const NAME: (&[&str]|[&str; N]|usize|u32|i32|f64) = ...`.
# This is tighter than "every const" on purpose — it's scoped to the shapes that actually carry policy
# (string-list vocabularies and small numeric thresholds), which keeps the initial census under
# ~200 lines. Two closed blind spots (both 2026-07-13, v0.12.0 release audit): `[&str; N]` fixed
# arrays (HTTP_VERB_EXPORTS / PAGES_API_FALLBACK_VERBS / compose VERBS carried verb policy invisibly
# in that form) and scoped visibility (`pub(crate)`/`pub(super)`/`pub(in ...)` const — the audit's
# own WRITE_HTTP_METHODS unification escaped a `(pub )?`-only pattern on the visibility axis).
# If a future sweep needs to widen the type list further, re-run --update and re-check the line
# count; if it balloons, narrow back down and note that here.
#
# A THIRD blind spot was found and considered, NOT closed (2026-07-13, v0.13.0 release audit): a
# single `const NAME: &str = "literal"` (no `&[&str]`/array/braces) is still outside the type
# alternation — this is exactly what let the "nest-global-prefix" sentinel-kind string drift
# independently across producer/consumer/envelope sites with no shared symbol (C1) before that
# batch introduced `NEST_GLOBAL_PREFIX_KIND`, mirroring the existing `CLIENT_BASE_PREFIX_KIND`
# pattern. Measured before deciding: adding bare `&str` to the alternation would pull in ~32-35
# entries across the census dirs (module-level AND function-local `&str` consts alike — e.g. every
# `PARSER_FINGERPRINT`, plus incidental non-policy aliasing consts like a local
# `const SENTINEL_KIND: &str = ...` binding), which balloons well past the "modest" (~25) bar this
# census was designed to stay under. Decision: single `&str` consts remain OUT of the tracked
# pattern — the specific sentinel-kind-pair failure mode is considered closed a different way: both
# `nest-global-prefix` and `client-base-prefix` are now T1-shared single-source consts
# (`NEST_GLOBAL_PREFIX_KIND` / `CLIENT_BASE_PREFIX_KIND`, each defined once in its producer crate and
# referenced everywhere else by symbol), so a future rename can no longer silently desync even though
# the census itself won't catch a NEW instance of the same mistake pattern.
#
# No deps beyond grep/sed/sort/comm.
set -euo pipefail

# Collation-pinned: the snapshot mixes lowercase paths, '/:._-' and uppercase const names — exactly
# the tokens whose sort order differs between C and UTF-8 locales. Without this, an --update run on
# one machine and a check run on another (CI) can disagree on ORDER alone and report spurious drift.
export LC_ALL=C

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

census_file="scripts/policy-census.txt"
pattern='^[[:space:]]*(pub(\((crate|super|in [^)]+)\))? )?const [A-Z_][A-Z0-9_]*: (&\[&str\]|\[&str;[[:space:]]*[0-9]+\]|usize|u32|i32|f64)'

dirs=()
for d in crates/engine/src crates/core/src parser/*/src rules/native/*/src; do
  [ -d "$d" ] && dirs+=("$d")
done

current="$(grep -rnE "$pattern" "${dirs[@]}" 2>/dev/null \
  | sed -E 's/^([^:]+):[0-9]+:[[:space:]]*(pub(\((crate|super|in [^)]+)\))? )?const ([A-Z_][A-Z0-9_]*):.*/\1:\5/' \
  | sort -u)"

if [ "${1:-}" = "--update" ]; then
  printf '%s\n' "$current" > "$census_file"
  count="$(printf '%s\n' "$current" | grep -c . || true)"
  echo "check-policy-census: snapshot regenerated ($count entries) -> $census_file"
  exit 0
fi

if [ ! -f "$census_file" ]; then
  echo "check-policy-census: missing $census_file — run: bash scripts/check-policy-census.sh --update" >&2
  exit 1
fi

committed="$(cat "$census_file")"

if [ "$current" != "$committed" ]; then
  echo "check-policy-census: policy-shaped constant census has drifted from $census_file" >&2
  added="$(comm -13 <(printf '%s\n' "$committed") <(printf '%s\n' "$current") || true)"
  removed="$(comm -23 <(printf '%s\n' "$committed") <(printf '%s\n' "$current") || true)"
  if [ -n "$added" ]; then
    echo "  added:" >&2
    printf '    %s\n' $added >&2
  fi
  if [ -n "$removed" ]; then
    echo "  removed:" >&2
    printf '    %s\n' $removed >&2
  fi
  echo >&2
  echo "new policy-shaped constant — triage it against the policy-value inventory (tier T1/T2/T3 or not-policy) and regenerate: bash scripts/check-policy-census.sh --update" >&2
  exit 1
fi

count="$(printf '%s\n' "$current" | grep -c . || true)"
echo "check-policy-census: OK ($count policy-shaped constants tracked)"
