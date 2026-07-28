#!/usr/bin/env bash
# Convention-vocabulary declarability guard (ratchet) — fails when a name vocabulary the policy census
# classifies as `convention` is not declarable from a `zzop.config.jsonc`, unless it is grandfathered in
# scripts/convention-vocab-baseline.txt.
#
# ## The invariant
# `scripts/policy-census.txt`'s `convention` axis means "the PROJECT picks this name, not a framework"
# (that script's header states the axis taxonomy and the one question behind it). A name in that class
# that the engine holds as a built-in literal is the engine GUESSING what a project calls its own
# things — it silently misclassifies every project that names them differently. The standing decision is
# that such vocabulary is DECLARED in config, with the built-in value shipped as a template default
# rather than as a hidden judgment. So:
#
#   census axis == convention  =>  a config key exists for it, AND the starter template names that key.
#
# "Exists" is checked against the two files that already own those facts — crates/config/config-surface.json
# (the machine-checked knob dictionary) and crates/config/src/template.rs (the starter document). No third
# list is introduced: a key this guard accepts is a key the config front end really takes.
#
# ## How a census line declares itself
# A `convention` line in the census may carry a config path after its axis:
#
#     path/to/file.rs:SOME_VOCAB convention -> git.commitSubjectPatterns # why it is a convention
#
# Absent that marker, the vocabulary is undeclarable and must be in the baseline. The marker lives on the
# census line — beside the axis it qualifies — rather than in a fourth file, for the same reason the axis
# itself does: `--update` carries the tail forward keyed by `path:CONST`, so renaming or moving the
# constant drops BOTH the axis and the declarability claim and forces a fresh judgment.
#
# ## Why a baseline, and what makes it different from an exemption
# Switched on cold, this guard is red on every convention entry at once: the only declarable vocabulary
# keys that exist today are `git.commitTypePatterns` and `git.commitSubjectPatterns`. The baseline freezes
# that starting debt so the ratchet can be useful immediately in the direction that matters — anything NEW
# must be declarable from its first commit, and the frozen entries are paid off one at a time.
#
# THE BASELINE IS DEBT, NOT PERMISSION. It only shrinks:
#   - a convention entry NOT in the baseline and not declarable fails (new undeclarable vocabulary), and
#   - a baseline entry that has BECOME declarable (or stopped being a convention entry, or disappeared)
#     fails as STALE until removed, so the file always states the real remaining debt.
# `--update-baseline` enforces the same direction: it refuses to ADD an entry. There is no mode, flag or
# argument of this script that lets a new undeclarable convention vocabulary through.
#
# The baseline reached ZERO on 2026-07-27 and the file stays, empty. That is not a leftover: this script
# exits 1 when the baseline is absent, and the half of the ratchet that still does work is the NEW-entry
# half — an undeclarable convention constant added tomorrow is red on its first commit whether the
# baseline holds one line or none. See the baseline file's own header for how the last entry went.
#
# Modes:
#   (default)          enforce; exit 1 on any violation.
#   --update-baseline  rewrite the baseline from the working tree. Shrink/remove only.
#
# Ratchet shape, baseline-file conventions and the shrink-only `--update-baseline` contract are copied
# from scripts/check-max-file-lines.sh deliberately — one ratchet dialect in this repo, not two.
#
# No deps beyond grep/sed/awk. Exit 1 on any violation.
set -euo pipefail
cd "$(dirname "$0")/.."

# Collation-pinned for the same reason the census itself is: these lists mix lowercase paths, '/:._-'
# and uppercase const names, whose sort order differs between C and UTF-8 locales.
export LC_ALL=C

CENSUS=scripts/policy-census.txt
BASELINE=scripts/convention-vocab-baseline.txt
SURFACE=crates/config/config-surface.json
TEMPLATE=crates/config/src/template.rs

for f in "$CENSUS" "$SURFACE" "$TEMPLATE"; do
  if [ ! -f "$f" ]; then
    echo "convention-vocab guard: missing $f — cannot judge declarability without it." >&2
    exit 1
  fi
done

# `path:CONST` of every census line whose axis column is exactly `convention`.
convention_keys() { awk '$2 == "convention" { print $1 }' "$CENSUS"; }

# The config path a census line claims, or empty. Format: `<key> convention -> <configPath> [# ...]`.
declared_path_of() { awk -v k="$1" '$1 == k && $3 == "->" { print $4 }' "$CENSUS"; }

# A claimed config path counts only if BOTH owners agree it exists. config-surface.json is read as text
# (no jq in this repo's shells — same constraint the census header records): a dotted path must appear as
# a quoted element, an undotted one must appear in the `configKeys.top` list. The template must then name
# it, either dotted in its prose or as a quoted JSON key.
path_is_declarable() { # <configPath>
  local p="$1" leaf="${1##*.}"
  if [ "$p" = "$leaf" ]; then
    # Top-level key: must be inside the configKeys.top array.
    awk -v k="\"$p\"" '
      /"top"[[:space:]]*:/ { intop = 1 }
      intop && index($0, k) { found = 1 }
      intop && /\]/ { intop = 0 }
      END { exit(found ? 0 : 1) }
    ' "$SURFACE" || return 1
  else
    grep -qF "\"$p\"" "$SURFACE" || return 1
  fi
  # Two statements rather than `grep ... || grep ...`: the SIGPIPE seal's line scan reads the second
  # `|` of a `||` as the head of a `| grep -q` pipeline, so the one-liner false-reds that guard.
  if grep -qF "$p" "$TEMPLATE"; then return 0; fi
  grep -qF "\"$leaf\"" "$TEMPLATE"
}

# Undeclarable convention entries, in census order.
undeclarable=""
while IFS= read -r k; do
  [ -n "$k" ] || continue
  p="$(declared_path_of "$k")"
  if [ -n "$p" ] && path_is_declarable "$p"; then continue; fi
  undeclarable="${undeclarable}${k}"$'\n'
done < <(convention_keys)
undeclarable="$(printf '%s' "$undeclarable" | grep -v '^$' || true)"

baseline_keys() { grep -v '^[[:space:]]*#' "$BASELINE" 2>/dev/null | grep -v '^[[:space:]]*$' || true; }

if [ "${1:-}" = "--update-baseline" ]; then
  refused=0
  existing="$(baseline_keys)"
  {
    echo "# Convention vocabulary that is NOT yet declarable from a zzop.config.jsonc — DEBT, not permission."
    echo "#"
    echo "# Every line is a name vocabulary the policy census classifies as \`convention\` (a name the PROJECT"
    echo "# picks, not one a framework fixed) that the engine still holds as a built-in literal. Holding it"
    echo "# built-in means the engine is guessing what a project calls its own guards, secrets, money fields"
    echo "# or ignored directories, and silently misclassifying the projects that name them differently."
    echo "#"
    echo "# THE LIST IS EMPTY AND THE FILE STAYS. This file existed so the ratchet could be switched on before"
    echo "# the debt was paid, and the debt reached zero on 2026-07-27 — the last entry (parser-prisma's"
    echo "# discover.rs SKIP_DIRS) was retired by DELETION rather than by wiring: the sole reader of that"
    echo "# constant was a schema-discovery walk with zero callers workspace-wide, so giving it a config key"
    echo "# would have declared a knob nothing could reach. An empty baseline is not a finished guard, it is the"
    echo "# floor this ratchet holds. The direction is what mattered all along: any NEW convention vocabulary"
    echo "# that is not declarable is red on its first commit, and --update-baseline refuses to ADD. Deleting"
    echo "# the file would not retire the guard, it would BREAK it (a missing baseline exits 1) — and an empty"
    echo "# file states the real remaining debt where a deleted one is indistinguishable from a lost one."
    echo "#"
    echo "# An entry is retired by giving the vocabulary a config key (crates/config/config-surface.json)"
    echo "# and naming that key in the starter template (crates/config/src/template.rs), then adding"
    echo "# \`-> <configPath>\` after the axis on that constant's scripts/policy-census.txt line — at which"
    echo "# point this guard reports the baseline entry as STALE and it is deleted. Tracked as backlog item"
    echo "# D14 (\"convention vocabulary is declared, not guessed; built-ins ship as template defaults\")."
    echo "#"
    echo "# An entry here is NOT an exemption and NOT permanent. Maintained by"
    echo "# scripts/check-convention-vocab-declarable.sh --update-baseline, which only shrinks."
    while IFS= read -r k; do
      [ -n "$k" ] || continue
      if ! grep -qxF "$k" <<< "$existing"; then
        echo "convention-vocab guard: refusing to ADD $k to the baseline — make it declarable instead" >&2
        echo "  (add a config key + template mention, then '-> <configPath>' on its census line)." >&2
        refused=1
      else
        printf '%s\n' "$k"
      fi
    done <<< "$undeclarable"
  } > "$BASELINE.tmp"
  if [ "$refused" -ne 0 ]; then rm -f "$BASELINE.tmp"; exit 1; fi
  mv "$BASELINE.tmp" "$BASELINE"
  echo "convention-vocab guard: baseline updated ($(baseline_keys | grep -c . || true) entries remaining)."
  exit 0
fi

if [ ! -f "$BASELINE" ]; then
  echo "convention-vocab guard: missing $BASELINE — run: bash $0 --update-baseline" >&2
  exit 1
fi

base="$(baseline_keys)"
violations=0

# New undeclarable convention vocabulary.
while IFS= read -r k; do
  [ -n "$k" ] || continue
  if ! grep -qxF "$k" <<< "$base"; then
    echo "  NEW   $k — census says \`convention\` but no config key declares it."
    violations=1
  fi
done <<< "$undeclarable"

# Stale baseline entries: paid off, reclassified, or gone.
while IFS= read -r k; do
  [ -n "$k" ] || continue
  if ! grep -qxF "$k" <<< "$undeclarable"; then
    echo "  STALE $k — no longer undeclarable convention vocabulary; drop it from the baseline"
    echo "        (run: bash scripts/check-convention-vocab-declarable.sh --update-baseline)"
    violations=1
  fi
done <<< "$base"

if [ "$violations" -ne 0 ]; then
  echo
  echo "convention-vocab guard: a vocabulary the census calls \`convention\` is a name the PROJECT picks,"
  echo "so the engine must not hold it as a built-in guess. Give it a config key in $SURFACE,"
  echo "name that key in $TEMPLATE, then mark the census line:"
  echo "    <path>:<CONST> convention -> <configPath> # <why it is a convention>"
  echo "The baseline ($BASELINE) freezes pre-existing debt only and never grows."
  exit 1
fi

echo "convention-vocab guard: clean ($(grep -c . <<< "$base" || true) undeclarable vocabularies remaining in the baseline)."
