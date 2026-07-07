#!/usr/bin/env bash
# Guards that packages/cli/README.md has not drifted from the `--help` text embedded in
# packages/cli/bin/zzop.js (the source of truth for CLI flags).
#
# Two asymmetric checks close the drift class that actually bites us — a flag added to the CLI but
# never documented, or a flag documented in the README that no longer exists (renamed/removed):
#   1. HELP -> README (help ⊆ README): every long option token (`--foo`) that appears in the help
#      text must also appear somewhere in the README.
#   2. README -> HELP (README ⊆ help): every long option token documented in the README's options
#      section must also appear in the help text, so the README cannot document a flag that was
#      renamed or removed.
# Short flags (`-a`, `-h`) are intentionally not checked — they always appear paired with a long
# form (`-a, --all`) in both files, so the long-form check already covers them.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cli_js="$repo_root/packages/cli/bin/zzop.js"
readme="$repo_root/packages/cli/README.md"

for f in "$cli_js" "$readme"; do
  [ -f "$f" ] || { echo "check-cli-readme-sync: missing $f" >&2; exit 1; }
done

fail=0

# --- Extract the USAGE (help text) block from zzop.js ---------------------------------------
# The help text lives in a `const USAGE = \`...\`;` template literal. Grab everything between the
# opening backtick and the closing backtick line.
help_text="$(sed -n '/^const USAGE = `/,/^`;$/p' "$cli_js")"
if [ -z "$help_text" ]; then
  echo "check-cli-readme-sync: could not locate the USAGE template literal in $cli_js" >&2
  exit 1
fi

help_flags="$(printf '%s\n' "$help_text" | grep -oE -- '--[a-z][a-z-]*' | sort -u)"
readme_flags="$(grep -oE -- '--[a-z][a-z-]*' "$readme" | sort -u)"

# --- Check 1: every help-text flag is documented in the README (help ⊆ README) --------------
missing_in_readme=""
while IFS= read -r flag; do
  [ -z "$flag" ] && continue
  grep -qF -- "$flag" "$readme" || missing_in_readme="$missing_in_readme $flag"
done <<< "$help_flags"
if [ -n "$missing_in_readme" ]; then
  echo "check-cli-readme-sync: flags in \`zzop --help\` missing from packages/cli/README.md:" >&2
  printf '    %s\n' $missing_in_readme >&2
  fail=1
fi

# --- Check 2: every README flag exists in the help text (README ⊆ help) ---------------------
stale_in_readme=""
while IFS= read -r flag; do
  [ -z "$flag" ] && continue
  grep -qF -- "$flag" <<< "$help_text" || stale_in_readme="$stale_in_readme $flag"
done <<< "$readme_flags"
if [ -n "$stale_in_readme" ]; then
  echo "check-cli-readme-sync: packages/cli/README.md documents flags not in \`zzop --help\`:" >&2
  echo "  (stale or invented — align the README with bin/zzop.js's USAGE text):" >&2
  printf '    %s\n' $stale_in_readme >&2
  fail=1
fi

if [ "$fail" -ne 0 ]; then
  echo "check-cli-readme-sync: FAILED — keep packages/cli/README.md in sync with bin/zzop.js's help text." >&2
  exit 1
fi

help_count="$(printf '%s\n' "$help_flags" | grep -c . || true)"
readme_count="$(printf '%s\n' "$readme_flags" | grep -c . || true)"
echo "check-cli-readme-sync: OK (${help_count} help flags documented in README, ${readme_count} README flags recognized by --help)"
