#!/usr/bin/env bash
# guards-wired meta-guard — fails when a scripts/check-*.sh guard is not wired into BOTH
# .githooks/pre-commit and .github/workflows/ci.yml's `guards` job. Nothing else in this repo
# mechanically enforces that a newly authored guard actually runs anywhere; without this, a guard
# can be written, committed, and quietly never invoked again.
#
# Single hardcoded EXTRA requirement (2026-07-26 — it used to be an EXEMPTION):
# check-parser-fingerprint-bump.sh must additionally be wired into .githooks/pre-push. It used to be
# exempted from the pre-commit requirement on the claim that it "structurally cannot run from
# pre-commit, which only ever sees the working tree, not a range". Both halves were wrong once
# measured: the guard now takes its file list from the working tree when the range's right end is
# HEAD (see its own header), and the batch flow that leaves work uncommitted is exactly when its
# subject exists. So it is held to the same pre-commit + CI bar as every sibling, and pre-push — which
# asks the separate push-shaped question about the outgoing COMMIT range — is asserted on top.
# A hardcoded name that only ever ADDS a requirement cannot become a hole; the old spelling could.
#
# This script wires itself the same way its siblings are wired (see .githooks/pre-commit and
# ci.yml, both updated alongside this file) rather than special-casing itself out of the
# requirement — a meta-guard that isn't itself wired in is exactly the failure mode it exists to
# catch, so it does not exempt its own name from the loop below.
#
# Also asserts scripts/lib/tracked-grep.sh exists: several check-*.sh guards source it for their
# TRACKED-file enumeration (see its own header comment), so a missing lib silently breaks every
# guard that depends on it at the `. ./scripts/lib/tracked-grep.sh` line -- worth a fast, specific
# failure here rather than each guard's own less obvious "command not found: tracked_files_matching".
# It is NOT itself wired into pre-commit/CI/pre-push the way a scripts/check-*.sh guard is: it lives
# under scripts/lib/, has no independent exit status of its own, and is only ever sourced by a real
# guard -- the `git ls-files -z -- 'scripts/check-*.sh'` glob below does not (and must not) match it.
#
# SECOND SUBJECT (2026-07-28): cross-workflow `workflow_run: workflows: [<name>]` references.
# Added here rather than as a 24th scripts/check-*.sh because this file is already the repo's answer to
# "is the defense actually CONNECTED to a trigger" — and a new script would itself need wiring into
# pre-commit, ci.yml and this very loop to be worth anything.
#
# The reference is by workflow NAME, not by path, and GitHub resolves an unknown name to SILENCE: the
# dependent workflow simply never fires, with no error anywhere. `.github/workflows/pages.yml` gates the
# public site on `workflows: [ci]`, so renaming ci.yml's `name:` would not break the gate loudly — it
# would remove it, and the only symptom is a site that quietly stops updating. That is a literal
# cross-file constant with nothing checking it, i.e. the same shape this repo spent 2026-07-28 removing
# from sixteen guards, committed in the very edit that added the gate.
#
# No deps beyond git + grep + awk. Exit 1 on any violation, listing the exact (guard,
# missing-location) pairs.
set -euo pipefail
cd "$(dirname "$0")/.."

PRE_COMMIT=.githooks/pre-commit
PRE_PUSH=.githooks/pre-push
CI=.github/workflows/ci.yml
TRACKED_GREP_LIB=scripts/lib/tracked-grep.sh

ALSO_NEEDS_PRE_PUSH="check-parser-fingerprint-bump"

missing=0        # any failure at all -> exit 1
wiring_missing=0 # the scripts/check-*.sh axis only, so its spelling epilogue prints only for it
count=0

# Is `scripts/<base>.sh` INVOKED on a code line of <file>? Full-line comments are skipped — the same
# idiom check-shell-pipe-sigpipe.sh:27 uses, and for the same reason.
#
# Bit for real 2026-07-25 (twice, same class — "a mention is not a call"):
#  1. This was a bare `grep -qF`, so commenting a CI step out
#     (`# DISABLED: run: bash scripts/check-max-file-lines.sh`) left this meta-guard reporting
#     "clean (18 guards checked)" — the one thing that is supposed to notice a guard that stopped
#     running instead vouched for the disabled step's own tombstone. Fixed by skipping full-line
#     comments (a real invocation with a trailing `# note` still counts).
#  2. Skipping comments was not enough: `.githooks/pre-push` prints an ERROR MESSAGE naming the
#     script it just ran (`echo "pre-push: scripts/check-parser-fingerprint-bump.sh FAILED ..."`),
#     and a bare substring test accepted that string as the wiring. Measured: commenting out the
#     one real invocation and leaving the echo behind still reported "clean (18 guards checked)".
#
# So the needle is no longer "the path appears" but "the path appears in INVOCATION FORM" — one of
# `bash <path>`, `sh <path>`, `./<path>` — plus their combinations (`bash ./<path>`, flags in between,
# a quoted path `bash "scripts/x.sh"`) — with a non-word character required in front so a word merely
# ENDING in `sh` (`pre-push: scripts/...`, `finish scripts/...`) cannot supply the `sh`. Each form is
# accepted ANYWHERE on the line.
#
# The quoted form is accepted because a quoted literal path is an ordinary shell spelling, NOT because
# some file here spells it that way — nothing in this tree does (corrected 2026-07-25; this comment
# used to cite `.githooks/pre-commit`, which actually spells it `bash "scripts/$g.sh"`, a VARIABLE.
# That line can never match this needle, which interpolates a literal base name — and does not need
# to: pre-commit is checked by the line-anchored GUARDS-array grep below, not by `invoked_in`, which
# only ever reads ci.yml and pre-push).
#
# ## Residual: two legitimate spellings ARE rejected (measured, deliberately not closed)
# This block used to claim "narrowing to them costs no legitimate spelling". That is an absolute
# claim and it is false — measured against this awk:
#   * `run: scripts/check-foo.sh` (exec bit, no interpreter)  -> nomatch
#   * `bash "$REPO/scripts/check-foo.sh"` (variable prefix)   -> nomatch
# Neither is worth what closing it costs, and the two costs are different:
#   * The bare-path form is UNCLOSABLE here. Accepting `scripts/<base>.sh` with no interpreter puts
#     back bug #2 above verbatim: measured, an accepting variant matches
#     `echo "pre-push: scripts/check-foo.sh FAILED"`. `run: ` (a YAML key) and `pre-push: ` (text
#     inside an echo) are the same shape to a line regex, so no needle can take one and refuse the
#     other. A false GREEN there is the lane silently turning off — strictly worse than a false red.
#   * The variable-prefix form IS closable: allowing an optional `<dir>/` prefix INSIDE the `bash|sh`
#     branch only was measured to keep all three known false-green strings at nomatch. Not adopted —
#     no file in the tree uses that spelling, and it would let a same-basename script under ANY
#     directory (`bash vendor/x/scripts/check-foo.sh`) vouch for `scripts/check-foo.sh`. Adding a
#     hole to close a hypothesis is the wrong trade; if that spelling ever lands, this is the change.
# The residual therefore shows up as a false RED, and the escape-hatch risk that creates is answered
# by naming the accepted spellings in the failure output below rather than by widening the needle.
#
# ## Rejected: the own-line rule that check-parser-fingerprint-bump.sh uses for its marker
# That guard settles the same mention-vs-real question by requiring its marker to stand ON ITS OWN
# LINE (see its header). Checked here, and it does NOT transfer — measured against this very tree,
# where NO invocation is line-leading:
#   * .github/workflows/ci.yml — `        run: bash scripts/check-english-source.sh` (YAML key first)
#   * .githooks/pre-push       — `  if ! out="$(FINGERPRINT_DIFF_RANGE="$range" bash scripts/...`
# Requiring line-leading would false-red both files on day one, and false red is the more dangerous
# failure here: a guard that cries wolf gets silenced with an escape hatch, and the escape hatch is
# what turns a lane off for good. (`.githooks/pre-commit` is the one place the own-line rule DOES
# hold, and the check below already uses it: the GUARDS array entry is line-anchored, which is why
# that axis was never fooled by either of the two bugs above.)
#
# ## Known limitation, deliberately not closed
# A string that quotes a COMPLETE invocation still counts — e.g. a hint message
# `echo "on failure run: bash scripts/check-policy-census.sh --update"` inside pre-push would
# satisfy this check on its own. Closing it needs "is this offset inside a quoted string", and the
# obvious implementations break on the real code: naive quote-pairing on pre-push's own invocation
# line (`out="$(VAR="$range" bash scripts/... )"`) pairs the wrong quotes across the `$( )` and
# strips the REAL call — i.e. the cure produces exactly the false red described above. The residual
# is strictly narrower than what was there before (a bare mention no longer counts at all), it is
# confined to two hand-edited files of ~50 and ~100 lines, and it is recorded here rather than left
# for someone to rediscover.
invoked_in() { # <file> <guard base name>
  awk -v base="$2" '
    BEGIN {
      # `bash <p>` / `sh <p>` / `bash ./<p>` / `./<p>`, with optional flags after bash|sh and an
      # optional quote around the path (`bash "scripts/x.sh"`). The path must be the LITERAL
      # `scripts/<base>.sh` — the header residual section lists the two legitimate spellings this
      # rejects and why. (\047 is the single quote, spelled octal so this awk program can stay
      # single-quoted in sh. For the same reason no apostrophe may appear in these comments.)
      re = "(^|[^[:alnum:]_])((bash|sh)[[:space:]]+(-[^[:space:]]+[[:space:]]+)*[\"\047]?(\\./)?" \
           "|[\"\047]?\\./)scripts/" base "\\.sh"
    }
    /^[[:space:]]*#/ { next }
    $0 ~ re { found = 1; exit }
    END { exit(found ? 0 : 1) }
  ' "$1"
}

# Every guard in the tree, tracked or not. The untracked half is the whole point: a guard is at its
# most un-wired the moment it is written, and until `git add` a tracked-only enumeration cannot see
# it — so the meta-guard stayed silent through exactly the window it exists to cover, then went red
# only after the commit that should have been blocked. Same `--others --exclude-standard` scope
# check-max-file-lines.sh:37-38 already uses; ignored paths stay out.
list_guards() {
  { git ls-files -z -- 'scripts/check-*.sh'
    git ls-files -z --others --exclude-standard -- 'scripts/check-*.sh'
  } | sort -zu
}

if [ ! -f "$TRACKED_GREP_LIB" ]; then
  echo "check-guards-wired: $TRACKED_GREP_LIB -- missing. Several isolation/scope guards source it" >&2
  echo "  for their TRACKED-file enumeration; without it they fail at the '. ./$TRACKED_GREP_LIB' line." >&2
  missing=1; wiring_missing=1
fi

while IFS= read -r -d '' f; do
  base="$(basename "$f" .sh)"
  count=$((count + 1))

  if [ "$base" = "$ALSO_NEEDS_PRE_PUSH" ] && ! invoked_in "$PRE_PUSH" "$base"; then
    echo "check-guards-wired: ($base, $PRE_PUSH) -- range-based guard not wired into pre-push"
    missing=1; wiring_missing=1
  fi

  if ! grep -qE "^[[:space:]]*${base}[[:space:]]*$" "$PRE_COMMIT"; then
    echo "check-guards-wired: ($base, $PRE_COMMIT) -- not wired into pre-commit's GUARDS array"
    missing=1; wiring_missing=1
  fi

  if ! invoked_in "$CI" "$base"; then
    echo "check-guards-wired: ($base, $CI) -- not wired into CI's guards job"
    missing=1; wiring_missing=1
  fi
done < <(list_guards)

# --- cross-workflow `workflow_run` name references --------------------------------------------------
# One awk pass over every tracked workflow: collect each file's `name:`, collect every workflow named by
# a `workflow_run:` block, then in END report the references that resolve to nothing. Both YAML list
# spellings are read (`workflows: [a, b]` and a `-` block), because accepting only the one this repo
# happens to use today would make a legal rewrite look like a broken reference.
workflow_files=()
while IFS= read -r -d '' f; do
  workflow_files+=("$f")
done < <(git ls-files -z -- '.github/workflows/*.yml' '.github/workflows/*.yaml')

if [ "${#workflow_files[@]}" -eq 0 ]; then
  echo "check-guards-wired: FAILED -- enumerated ZERO workflow files. The workflow_run cross-reference"
  echo "check therefore proved nothing. An empty subject set is a broken guard, never a clean tree."
  missing=1
else
  dangling="$(awk '
    function indent_of(s,   i) { i = match(s, /[^ ]/); return (i == 0 ? -1 : i - 1) }
    FNR == 1 { inrun = 0; inlist = 0 }
    /^[[:space:]]*#/ { next }
    # A workflow`s own name. First one wins: `name:` also appears on steps, but always indented.
    /^name:[[:space:]]*[^[:space:]]/ {
      if (!(FILENAME in selfname)) {
        v = $0; sub(/^name:[[:space:]]*/, "", v)
        gsub(/^["\047]|["\047][[:space:]]*$/, "", v); sub(/[[:space:]]+$/, "", v)
        selfname[FILENAME] = v; names[v] = 1
      }
      next
    }
    /^[[:space:]]*workflow_run:[[:space:]]*$/ { inrun = 1; runind = indent_of($0); inlist = 0; next }
    # Any line at or left of `workflow_run:` closes its block.
    inrun && /[^[:space:]]/ && indent_of($0) <= runind { inrun = 0; inlist = 0 }
    inrun && /^[[:space:]]*workflows:[[:space:]]*\[/ {
      v = $0; sub(/^[^\[]*\[/, "", v); sub(/\].*$/, "", v)
      n = split(v, parts, ",")
      for (i = 1; i <= n; i++) {
        w = parts[i]; gsub(/^[[:space:]]*["\047]?|["\047]?[[:space:]]*$/, "", w)
        if (w != "") refs[++nref] = FILENAME "\t" FNR "\t" w
      }
      inlist = 0; next
    }
    inrun && /^[[:space:]]*workflows:[[:space:]]*$/ { inlist = 1; listind = indent_of($0); next }
    inlist && /^[[:space:]]*-[[:space:]]*[^[:space:]]/ && indent_of($0) > listind {
      w = $0; sub(/^[[:space:]]*-[[:space:]]*/, "", w)
      gsub(/^["\047]|["\047][[:space:]]*$/, "", w); sub(/[[:space:]]+$/, "", w)
      if (w != "") refs[++nref] = FILENAME "\t" FNR "\t" w
      next
    }
    inlist && /[^[:space:]]/ { inlist = 0 }
    END {
      # `#total` and not an empty leading field: TAB is IFS whitespace, so bash `read` collapses a
      # leading empty field and the count line would arrive shifted, be read as a file path, and
      # report itself as a dangling reference. Observed, not reasoned about. No real path can
      # collide -- every one of these starts with `.github/`.
      print "#total\t" nref + 0
      for (i = 1; i <= nref; i++) { split(refs[i], r, "\t"); if (!(r[3] in names)) print refs[i] }
    }
  ' "${workflow_files[@]}")"

  wf_refs=""
  while IFS=$'\t' read -r wf wl wn; do
    # The count line, matched before anything treats it as a path. It exists so a green states how many
    # references were actually resolved, rather than letting "no dangling references" read identically
    # whether there were nine or zero.
    if [ "$wf" = "#total" ]; then wf_refs="$wl"; continue; fi
    [ -n "$wf" ] || continue
    echo "check-guards-wired: ($wf:$wl, workflow_run) -- waits on a workflow named \"$wn\", and no"
    echo "  .github/workflows/*.yml declares that name. GitHub resolves an unknown name to SILENCE, so"
    echo "  this workflow would never run again -- and nothing else would say so."
    missing=1
  done <<< "$dangling"
fi

# The epilogue below is SPELLING advice about scripts/check-*.sh wiring. Printing it after a dangling
# `workflow_run` failure would send the reader to the wrong file entirely, so it is bound to the wiring
# axis rather than to `missing` — which now has two contributors.
if [ "$missing" -ne 0 ] && [ "$wiring_missing" -ne 0 ]; then
  echo
  echo "check-guards-wired: every scripts/check-*.sh must run in BOTH .githooks/pre-commit and"
  echo ".github/workflows/ci.yml's guards job. check-parser-fingerprint-bump.sh must ALSO run from"
  echo ".githooks/pre-push (the outgoing-commit-range question); that is an extra requirement on top"
  echo "of the two, not a substitute for them."
  echo
  echo "If the guard IS wired and this still fails, check the SPELLING: ci.yml/pre-push invocations"
  echo "are recognized only as 'bash scripts/<name>.sh', 'sh scripts/<name>.sh', './scripts/<name>.sh'"
  echo "(flags and a quoted path are fine, anywhere on the line). A bare 'scripts/<name>.sh' with no"
  echo "interpreter, or a variable path prefix, is NOT recognized — deliberately, see this script's"
  echo "header. Use one of the accepted spellings; .githooks/pre-commit wires by GUARDS-array entry."
fi

if [ "$missing" -ne 0 ]; then
  exit 1
fi

# The subject set is a glob, and a glob that stops matching makes THIS guard — the one whose whole
# job is proving the fleet is wired — certify a fleet of nothing. Measured 2026-07-28 by redirecting
# the pathspec in a scratch copy: it printed "clean (0 guards checked)" and exited 0. That is the
# repo's own twice-paid class (a scan root pointing at a deleted directory, green while reading
# nothing), landing on the meta-guard itself.
if [ "$count" -eq 0 ]; then
  echo "check-guards-wired: FAILED -- enumerated ZERO guards. scripts/check-*.sh matched nothing, so"
  echo "this run proved nothing about whether any guard is wired. Fix the enumeration before trusting"
  echo "a green from this script; an empty subject set is a broken guard, never a clean fleet."
  exit 1
fi

echo "check-guards-wired: clean ($count guards checked; ${#workflow_files[@]} workflows, ${wf_refs:-0} workflow_run reference(s) resolved)."
