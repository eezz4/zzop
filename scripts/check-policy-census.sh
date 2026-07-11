#!/usr/bin/env bash
# Mechanical census of every policy-shaped constant under the crates that hold rule/extraction logic
# (packages/engine/src, packages/core/src, parser/*/src, rules/native/*/src). This is the "continuous
# drift review" mechanism (A5): the census tracks EXISTENCE (path:CONST_NAME), never values — values
# change legitimately and are not what this guard is for. Its job is to force a triage moment (tier
# T1/T2/T3, or "not policy") every time a *new* policy-shaped constant is introduced, by failing CI
# until the committed snapshot (scripts/policy-census.txt) is regenerated to include it.
#
# Regex is intentionally narrow: `^\s*(pub )?const NAME: (&[&str]|usize|u32|i32|f64) = ...`. This
# is tighter than "every const" on purpose — it's scoped to the shapes that actually carry policy
# (string-list vocabularies and small numeric thresholds), which keeps the initial census under
# ~200 lines. If a future sweep needs to widen the type list, re-run --update and re-check the line
# count; if it balloons, narrow back down to just `&[&str]` + `usize` (the two shapes that have
# actually carried policy so far) and note that here.
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
pattern='^[[:space:]]*(pub )?const [A-Z_][A-Z0-9_]*: (&\[&str\]|usize|u32|i32|f64)'

dirs=()
for d in packages/engine/src packages/core/src parser/*/src rules/native/*/src; do
  [ -d "$d" ] && dirs+=("$d")
done

current="$(grep -rnE "$pattern" "${dirs[@]}" 2>/dev/null \
  | sed -E 's/^([^:]+):[0-9]+:[[:space:]]*(pub )?const ([A-Z_][A-Z0-9_]*):.*/\1:\3/' \
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
