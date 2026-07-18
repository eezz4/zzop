#!/usr/bin/env bash
# Site SDK-page token guard — fails when site/sdk.html's prose drifts from the real @zzop/native
# SDK surface (packages/native/index.d.ts).
#
# The drift class (bitten twice): sdk.html said "four functions" when the addon exported five, and
# still said "five" after queryIo made it six — a prose count and a hand-written function table
# have no compiler, so they rot silently on every SDK-surface addition.
#
# Three containment checks (token-level, same idiom as the other sync guards):
#   1. completeness — every `export function X(` name in packages/native/index.d.ts must appear as
#      a code token (`<code>X</code>`) somewhere in site/sdk.html.
#   2. count — (a) any number word/numeral immediately preceding "function"/"functions" in
#      sdk.html must equal the real export count, and (b) the literal "<count-word> functions"
#      (e.g. "six functions") must appear at least once. Only the adjacent position is anchored;
#      free-floating count references ("All six are ...") are deliberately NOT chased — no
#      reliable anchor. The required literal makes the guard fail on the next export addition,
#      forcing a human re-read of the count prose.
#   3. subset — every function name presented in sdk.html's signature-table rows (the exact row
#      shape `<td><code>NAME</code></td><td><code>(`) must be a real export, so a removed or
#      renamed export cannot linger in the table. Prose uses of words like "analyze" are never
#      matched — only the code-formatted table anchor is.
# Prose quality is NOT checked — only that tokens and counts agree with index.d.ts.
#
# No deps beyond grep -P (PCRE). Exit 1 on any drift, listing it.
set -euo pipefail
cd "$(dirname "$0")/.."

dts=packages/native/index.d.ts
page=site/sdk.html
[ -f "$dts" ] || { echo "check-site-sdk-tokens: missing $dts" >&2; exit 1; }
[ -f "$page" ] || { echo "check-site-sdk-tokens: missing $page" >&2; exit 1; }

exports="$(grep -oP '^export function \K[A-Za-z0-9_]+(?=\()' "$dts" | sort -u)"
[ -n "$exports" ] || { echo "check-site-sdk-tokens: no 'export function' declarations found in $dts" >&2; exit 1; }
count="$(printf '%s\n' "$exports" | wc -l | tr -d '[:space:]')"

case "$count" in
  1) word=one ;;   2) word=two ;;   3) word=three ;; 4) word=four ;;
  5) word=five ;;  6) word=six ;;   7) word=seven ;; 8) word=eight ;;
  9) word=nine ;; 10) word=ten ;;  11) word=eleven ;; 12) word=twelve ;;
  *) word="$count" ;;
esac

fail=0

# --- Check 1: every export mentioned as a <code> token ---
missing=""
while IFS= read -r name; do
  grep -qP "<code[^>]*>$name</code>" "$page" || missing="$missing $name"
done <<< "$exports"
if [ -n "$missing" ]; then
  echo "check-site-sdk-tokens: exported functions never mentioned as <code>...</code> tokens in $page:" >&2
  printf '    %s\n' $missing >&2
  fail=1
fi

# --- Check 2a: any "<number> functions" phrasing must state the real count ---
# Every site page, not just sdk.html — architecture.html/usage.html carried a stale "five
# functions" claim the day this guard landed (same rot class, different page). PLURAL only across
# the loop: singular "one function" is ordinary prose (rules.html: "appear together in one
# function"), never a surface-count claim; sdk.html's own count prose has always been plural.
for count_page in site/*.html; do
  bad_counts="$(grep -oiP '\b(one|two|three|four|five|six|seven|eight|nine|ten|eleven|twelve|[0-9]+)(?=\s+functions\b)' "$count_page" \
    | tr '[:upper:]' '[:lower:]' | grep -vx -e "$word" -e "$count" | sort -u || true)"
  if [ -n "$bad_counts" ]; then
    echo "check-site-sdk-tokens: $count_page states a function count that is not $count ($word):" >&2
    printf '    "%s functions"\n' $bad_counts >&2
    fail=1
  fi
done

# --- Check 2b: the real count must be stated somewhere ("<word> functions") ---
if ! grep -qiP "\\b${word}\\s+functions\\b" "$page"; then
  echo "check-site-sdk-tokens: $page must state the current surface size — expected the literal '$word functions'" >&2
  echo "    ($dts exports $count functions; update the count prose, and the table, for the new surface)." >&2
  fail=1
fi

# --- Check 3: signature-table function names are a subset of the real exports ---
rogue=""
while IFS= read -r name; do
  [ -z "$name" ] && continue
  # Herestring, never `printf | grep -q`: under pipefail a -q early exit can SIGPIPE printf (141).
  grep -qxF "$name" <<< "$exports" || rogue="$rogue $name"
done < <(grep -oP '<td[^>]*><code>\K[A-Za-z0-9_]+(?=</code></td><td[^>]*><code>\()' "$page" | sort -u)
if [ -n "$rogue" ]; then
  echo "check-site-sdk-tokens: $page's function table lists names that are not exported by $dts:" >&2
  printf '    %s\n' $rogue >&2
  fail=1
fi

if [ "$fail" -ne 0 ]; then
  echo "check-site-sdk-tokens: FAILED — sync site/sdk.html's function tokens/count with $dts." >&2
  exit 1
fi
echo "check-site-sdk-tokens: OK ($count exports, all tokens and counts in sync)"
