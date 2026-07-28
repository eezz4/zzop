#!/usr/bin/env bash
# check-example-config-vocabulary — every committed `zzop.config.jsonc` under `examples/` must declare
# the SAME convention vocabulary the shipped starter template declares.
#
# Why this guard exists. Since 2026-07-27 an undeclared `vocabulary.*` key is a judgment NOT MADE, not a
# built-in (`crates/engine/src/vocabulary.rs`), and every analysis lane refuses to run without a config at
# all (`zzop_config::load_for_root`). That makes these fixture configs load-bearing in a way they were not
# before: a benchmark tree whose config omits the block is analyzed by an engine that recognizes none of
# its own guard names, and the detection score would silently record that as the truth about the rules.
#
# The block is COPIED into each config rather than referenced, because a JSONC config has no include —
# which is precisely why a checked copy needs a check. `--update` rewrites every one of them from the
# template, so the template stays the single author of the values and these files stay derived.
#
# Not a ratchet: there is no baseline of tolerated drift. A config either matches the template or it does
# not, and "partly matches" has no useful meaning for a vocabulary whose keys are independent.
set -uo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo" || exit 1

template="crates/config/src/template.rs"
[ -f "$template" ] || { echo "check-example-config-vocabulary: missing $template" >&2; exit 1; }

# The template's `vocabulary` block, verbatim: from the line that opens it to its closing brace at the
# same indentation. Read as TEXT (not parsed) so the committed copies are byte-comparable, which is what
# makes a diff readable when this fails.
extract_template_block() {
  awk '
    /^  "vocabulary": \{$/ { inblock = 1 }
    inblock { print }
    inblock && /^  \}$/ { exit }
  ' "$template"
}

block="$(extract_template_block)"
if [ -z "$block" ] || ! grep -q '"authGuardPattern"' <<< "$block"; then
  echo "check-example-config-vocabulary: could not extract the template's vocabulary block from $template" >&2
  echo "  (the block must open with a line exactly '  \"vocabulary\": {' and close with '  }')" >&2
  exit 1
fi

configs="$(git ls-files 'cases/**/zzop.config.jsonc' 'cases/zzop.config.jsonc' 2>/dev/null)"
[ -n "$configs" ] || { echo "check-example-config-vocabulary: no committed example configs found" >&2; exit 1; }

# One config's own vocabulary block, extracted the same way, so a comparison is text-to-text.
extract_config_block() {
  awk '
    /^  "vocabulary": \{$/ { inblock = 1 }
    inblock { print }
    inblock && /^  \}$/ { exit }
  ' "$1"
}

if [ "${1:-}" = "--update" ]; then
  blockfile="$(mktemp)"
  trap 'rm -f "$blockfile"' EXIT
  printf '%s\n' "$block" > "$blockfile"
  for cfg in $configs; do
    tmp="$(mktemp)"
    # Two-file awk: pass 1 buffers the template block, pass 2 rewrites the config. Drops any existing
    # block and re-emits the template's just before the config's final closing brace, so the result is
    # the same whether the config had one or not.
    awk 'NR == FNR { block = block $0 ORS; next }
         { lines[FNR] = $0 }
         END {
           last = FNR
           while (last > 1 && lines[last] !~ /^\}/) last--
           skipping = 0; out = 0
           for (i = 1; i < last; i++) {
             if (lines[i] ~ /^  "vocabulary": \{$/) { skipping = 1; continue }
             if (skipping) { if (lines[i] ~ /^  \}$/) skipping = 0; continue }
             kept[++out] = lines[i]
           }
           # The member that will now precede the block needs a separating comma. Found by walking back
           # past blank and comment-only lines, so a trailing annotation cannot swallow the comma.
           for (j = out; j >= 1; j--) {
             if (kept[j] ~ /^[[:space:]]*$/ || kept[j] ~ /^[[:space:]]*\/\//) continue
             if (kept[j] !~ /[,{[]$/) kept[j] = kept[j] ","
             break
           }
           for (j = 1; j <= out; j++) print kept[j]
           printf "%s", block
           for (i = last; i <= FNR; i++) print lines[i]
         }' "$blockfile" "$cfg" > "$tmp"
    mv "$tmp" "$cfg"
  done
  echo "check-example-config-vocabulary: rewrote $(printf '%s\n' $configs | wc -l | tr -d ' ') config(s) from $template"
  exit 0
fi

status=0
for cfg in $configs; do
  have="$(extract_config_block "$cfg")"
  if [ -z "$have" ]; then
    echo "  MISSING  $cfg declares no \"vocabulary\" block — an analysis under it makes none of the name judgments" >&2
    status=1
  elif [ "$have" != "$block" ]; then
    echo "  DRIFTED  $cfg's \"vocabulary\" block differs from the shipped starter template" >&2
    diff <(printf '%s\n' "$block") <(printf '%s\n' "$have") | head -20 >&2
    status=1
  fi
done

if [ "$status" -ne 0 ]; then
  echo "" >&2
  echo "check-example-config-vocabulary: regenerate with: bash scripts/check-example-config-vocabulary.sh --update" >&2
  exit 1
fi

echo "check-example-config-vocabulary: OK ($(printf '%s\n' $configs | wc -l | tr -d ' ') config(s) match the starter template)."
