#!/usr/bin/env bash
# Max-file-lines guard (ratchet) — fails when a Rust source file exceeds LIMIT lines, unless it
# is grandfathered in scripts/max-file-lines-baseline.txt at (at most) its recorded line count.
#
# Policy: source files stay under 300 lines; an oversized file is split into a directory module
# (foo.rs -> foo/mod.rs + foo/*.rs). Violations that predate the guard are frozen in the baseline
# and may only shrink — the ratchet never loosens:
#   - a file NOT in the baseline exceeding the limit fails (new oversized file), and
#   - a baseline file exceeding its recorded count fails (grandfathered file grew).
# Entries whose file no longer exceeds the limit (split done, or file removed) are stale and fail
# until removed, so the baseline always reflects the real remaining debt.
#
# Modes:
#   (default)          enforce; exit 1 on any violation, listing file:lines.
#   --update-baseline  rewrite the baseline from the working tree. Only tightens: refuses to add
#                      an entry or raise a recorded count — fix the file instead.
#
# Scope: tracked + untracked-but-not-ignored *.rs (same rationale as check-english-source.sh);
# generated code is out of scope via the standard target/ exclusion (build-script output lives
# under target/, never in-tree). Line counts come from ONE `xargs wc -l` invocation — a per-file
# subshell loop costs minutes under Windows msys process spawning.
#
# TEST FILES ARE EXEMPT (2026-07-16 policy: the cap exists to keep SOURCE units small; tests may
# grow freely and should live beside their subject as a pair — foo.rs + foo_test.rs, or
# foo/tests.rs). Exempt patterns: any path under a tests/ directory (cargo integration tests),
# files ending _test.rs or _tests.rs (the in-tree convention is the plural form), files named
# tests.rs, and rules/dsl/** (pack dirs hold only [[test]] targets by construction).
#
# No deps beyond git + awk + mktemp. Exit 1 on any violation.
set -euo pipefail
cd "$(dirname "$0")/.."

LIMIT=300
BASELINE=scripts/max-file-lines-baseline.txt

list_rs_files() {
  { git ls-files -- '*.rs'
    git ls-files --others --exclude-standard -- '*.rs'
  } | sort -u | grep -v '^\.claude/' | grep -v '/target/' | grep -v 'node_modules/' \
    | grep -vE '(^|/)tests/' | grep -vE '_tests?\.rs$' | grep -vE '(^|/)tests\.rs$' \
    | grep -v '^rules/dsl/' || true
}

# Drop tracked-but-deleted paths (a file converted to a directory module stays listed by
# git ls-files until the deletion is committed); shell builtins only — no per-file spawn.
existing_only() {
  while IFS= read -r f; do [ -f "$f" ] && printf '%s\n' "$f"; done
}

# One wc call for every in-scope file: lines "<path> <count>" (wc prints "<count> <path>";
# the trailing "total" row is dropped). Assumes no spaces in repo paths (holds today; the
# english guard's xargs scope shares the assumption).
#
# wc's raw output goes to a temp file, not straight into a `var=$(pipe | awk ...)` command
# substitution: a mid-pipeline wc failure (a tracked file deleted or made unreadable between the
# `git ls-files` snapshot and this `wc` call) would otherwise risk silently under-reporting the
# census (an oversized file slipping the ratchet) if this line is ever refactored into a context
# (e.g. `local var=$(...)`) where `set -o pipefail` no longer aborts the script on its own. Capture
# PIPESTATUS explicitly instead of relying on that implicit propagation, and fail loud.
wc_out="$(mktemp)"
trap 'rm -f "$wc_out"' EXIT
set +e
list_rs_files | existing_only | xargs -d '\n' -r wc -l > "$wc_out"
wc_pipestatus=("${PIPESTATUS[@]}")
set -e
if [ "${wc_pipestatus[2]}" -ne 0 ]; then
  echo "max-file-lines guard: wc failed reading one or more tracked .rs files (deleted or became" >&2
  echo "unreadable mid-scan) -- aborting rather than risk an under-reported census." >&2
  exit 1
fi
all_counts=$(awk '$2 != "total" {print $2, $1}' "$wc_out")

# Census of files over the limit.
census=$(awk -v lim="$LIMIT" '$2 > lim' <<< "$all_counts")

declare -A base
if [ -f "$BASELINE" ]; then
  while read -r path lines; do
    case "$path" in ''|'#'*) continue;; esac
    base["$path"]=$lines
  done < "$BASELINE"
fi

declare -A current
while read -r f n; do
  [ -n "$f" ] || continue
  current["$f"]=$n
done <<< "$all_counts"

if [ "${1:-}" = "--update-baseline" ]; then
  refused=0
  {
    echo "# max-file-lines ratchet baseline — files grandfathered above the ${LIMIT}-line limit."
    echo "# Maintained by scripts/check-max-file-lines.sh --update-baseline (shrink/remove only)."
    while read -r f n; do
      [ -n "$f" ] || continue
      if [ -z "${base[$f]:-}" ]; then
        echo "max-file-lines guard: refusing to ADD $f ($n lines) to the baseline — split it instead." >&2
        refused=1
      elif [ "$n" -gt "${base[$f]}" ]; then
        echo "max-file-lines guard: refusing to RAISE $f (${base[$f]} -> $n lines) — split it instead." >&2
        refused=1
      else
        printf '%s %s\n' "$f" "$n"
      fi
    done <<< "$census"
  } > "$BASELINE.tmp"
  if [ "$refused" -ne 0 ]; then rm -f "$BASELINE.tmp"; exit 1; fi
  mv "$BASELINE.tmp" "$BASELINE"
  echo "max-file-lines guard: baseline updated ($(grep -vc '^#' "$BASELINE") entries)."
  exit 0
fi

violations=0

# New oversized files, and grandfathered files that grew.
while read -r f n; do
  [ -n "$f" ] || continue
  if [ -z "${base[$f]:-}" ]; then
    echo "  NEW   $f: $n lines (> $LIMIT) — split into a directory module (foo.rs -> foo/*.rs)"
    violations=1
  elif [ "$n" -gt "${base[$f]}" ]; then
    echo "  GREW  $f: ${base[$f]} -> $n lines — the ratchet only shrinks; split instead of growing"
    violations=1
  fi
done <<< "$census"

# Stale baseline entries: file gone or now within the limit.
for f in "${!base[@]}"; do
  if [ -z "${current[$f]:-}" ] || [ "${current[$f]}" -le "$LIMIT" ]; then
    echo "  STALE $f: baseline entry no longer needed — run: bash scripts/check-max-file-lines.sh --update-baseline"
    violations=1
  fi
done

if [ "$violations" -ne 0 ]; then
  echo
  echo "max-file-lines guard: violations found (limit $LIMIT lines, baseline $BASELINE)."
  exit 1
fi
echo "max-file-lines guard: clean ($(grep -c . <<< "$census" || true) grandfathered files remaining)."
