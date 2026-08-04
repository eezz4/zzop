#!/usr/bin/env bash
# check-committed-config-vocabulary — every committed `zzop.config.jsonc` in this repo must declare the
# SAME convention vocabulary the shipped starter template declares, unless it is on the exemption list
# below with a reason.
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
#
# ## Why the name changed, and why the subject is DERIVED (2026-08-01)
#
# This shipped as `check-example-config-vocabulary.sh`. Its filename, its header and its CI step name all
# said `examples/`; its pathspec read `'cases/**/zzop.config.jsonc' 'cases/zzop.config.jsonc'` — one
# directory, and not the one it named. (The benchmark used to live at `examples/cases/`; the move to
# `cases/` updated the pathspec and left every sentence about it behind.) So the two configs OUTSIDE
# `cases/` had never been read by it: measured on the day this changed, `git ls-files
# '*zzop.config.jsonc'` returned 25 files while the guard printed "OK (23 config(s) match)".
#
# Both repairs the situation allowed — widen the pathspec to the name, or rename to the pathspec — were
# available; widening wins because it COVERS MORE: the name promised `examples/` (2 files at the time),
# and deriving the subject from `git ls-files` promises every committed config, in any directory,
# including ones nobody has created yet. The name then had to move to the wider subject rather than the
# other way round, which is why the file is called `committed` and not `example`. A guard's name is read
# far more often than its pathspec, so the two disagreeing is not cosmetic — it is what let this one be
# cited as covering `examples/` for as long as it was.
#
# Deriving surfaced two real configs the old pathspec never saw, and they landed on opposite sides:
#   - `examples/adapters/override-required/zzop.config.jsonc` declared NO vocabulary block at all, while
#     being a real analyzed fixture (`crates/engine/tests/analyze_override_displacement.rs` runs the
#     engine over that tree, and its README documents a `zzop graph --config` invocation on it). It was
#     the exact case this guard's rationale describes, so it was FIXED, not exempted.
#   - the repo-root `zzop.config.jsonc` is exempt, for the reason recorded beside it below.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo"

template="crates/config/src/template.rs"
[ -f "$template" ] || { echo "check-committed-config-vocabulary: missing $template" >&2; exit 1; }

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
  echo "check-committed-config-vocabulary: could not extract the template's vocabulary block from $template" >&2
  echo "  (the block must open with a line exactly '  \"vocabulary\": {' and close with '  }')" >&2
  exit 1
fi

# ── Subject set: DERIVED, then an exemption list SUBTRACTED ──────────────────────────────────────────
#
# `git ls-files '*zzop.config.jsonc'` is the whole claim: a config committed anywhere is checked, and a
# config added in a directory nobody thought of is checked on the day it lands rather than the day
# someone remembers this file.
all_configs=()
while IFS= read -r path; do
  [ -n "$path" ] && all_configs+=("$path")
done < <(git ls-files '*zzop.config.jsonc')

# ── Non-empty floor ──────────────────────────────────────────────────────────────────────────────────
#
# A pathspec that stops matching returns an empty list, and a loop over an empty list reports OK over
# zero files — indistinguishable from a clean tree in the output, which is how the whole class of defect
# this guard was just repaired for stays invisible. The floor is well under the real count (25 committed
# / 24 checked when it was written) so ordinary churn never trips it, and far above zero so a broken
# derivation always does.
if [ "${#all_configs[@]}" -lt 20 ]; then
  echo "check-committed-config-vocabulary: FAILED -- derived only ${#all_configs[@]} committed config(s)." >&2
  echo "  This repo commits a zzop.config.jsonc per benchmark tree and has had 20+ of them since the" >&2
  echo "  cases/ split, so a number this small is a broken 'git ls-files' pathspec, never a clean tree." >&2
  exit 1
fi

# EXEMPTIONS — `<path>|<reason>`. Subtracted from the derived set above; never a substitute for it.
# A reason is mandatory: an exemption without one is where a missing subject hides.
EXEMPT_CONFIGS=(
  "zzop.config.jsonc|the repo's own self-analysis config, not a fixture tree. It is tuned for ONE question (is our cache lane closed?) and its vocabulary block is DELIBERATELY different from the template's: it sets cacheLaneAnchorPattern to ^compute_fresh_artifact\$ where the template ships null, so rewriting it from the template would silence the only rule it exists to run."
)

# Exemption check, direction 2: an entry naming a config that is no longer committed FAILS. Without this,
# a fixture deleted and later recreated under the same path would come back already excused, with nobody
# having re-read the reason.
for entry in "${EXEMPT_CONFIGS[@]}"; do
  exempt_path="${entry%%|*}"
  exempt_reason="${entry#*|}"
  found=0
  for cfg in "${all_configs[@]}"; do
    [ "$cfg" = "$exempt_path" ] && found=1 && break
  done
  if [ "$found" -ne 1 ]; then
    echo "check-committed-config-vocabulary: EXEMPT_CONFIGS names '$exempt_path', which is not a committed" >&2
    echo "  zzop.config.jsonc any more. Reason on file: $exempt_reason" >&2
    echo "  Delete the entry — an exemption that outlives its subject silently excuses whatever returns" >&2
    echo "  under that path." >&2
    exit 1
  fi
done

configs=()
for cfg in "${all_configs[@]}"; do
  skip=0
  for entry in "${EXEMPT_CONFIGS[@]}"; do
    [ "$cfg" = "${entry%%|*}" ] && skip=1 && break
  done
  [ "$skip" -eq 0 ] && configs+=("$cfg")
done

# The same floor one step later: the exemption list must never become the thing that empties the
# subject. Separate from the derivation floor above so the failure message names the right cause.
if [ "${#configs[@]}" -lt 20 ]; then
  echo "check-committed-config-vocabulary: FAILED -- ${#all_configs[@]} config(s) derived but only" >&2
  echo "  ${#configs[@]} left after subtracting EXEMPT_CONFIGS. The exemption list is meant to hold" >&2
  echo "  single, argued cases; if it is now eating the subject set, the guard checks nothing." >&2
  exit 1
fi

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
  for cfg in "${configs[@]}"; do
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
  echo "check-committed-config-vocabulary: rewrote ${#configs[@]} config(s) from $template"
  exit 0
fi

status=0
for cfg in "${configs[@]}"; do
  have="$(extract_config_block "$cfg")"
  if [ -z "$have" ]; then
    echo "  MISSING  $cfg declares no \"vocabulary\" block — an analysis under it makes none of the name judgments" >&2
    status=1
  elif [ "$have" != "$block" ]; then
    echo "  DRIFTED  $cfg's \"vocabulary\" block differs from the shipped starter template" >&2
    # `|| true` on BOTH links: diff exits 1 on a difference (which is the case being reported) and head
    # closes the pipe early, either of which would abort the loop under `set -e -o pipefail` and hide
    # every remaining offender behind the first one.
    { diff <(printf '%s\n' "$block") <(printf '%s\n' "$have") || true; } | { head -20 || true; } >&2
    status=1
  fi
done

if [ "$status" -ne 0 ]; then
  echo "" >&2
  echo "check-committed-config-vocabulary: regenerate with: bash scripts/check-committed-config-vocabulary.sh --update" >&2
  exit 1
fi

echo "check-committed-config-vocabulary: OK (${#configs[@]} of ${#all_configs[@]} committed config(s) match the starter template; ${#EXEMPT_CONFIGS[@]} exempt)."
