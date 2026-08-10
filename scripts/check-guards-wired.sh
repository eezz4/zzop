#!/usr/bin/env bash
# guards-wired meta-guard — fails when a scripts/check-*.sh guard is not wired into BOTH
# .githooks/pre-commit and .github/workflows/ci.yml's `guards` job. Nothing else in this repo
# mechanically enforces that a newly authored guard actually runs anywhere; without this, a guard
# can be written, committed, and quietly never invoked again.
#
# NO per-guard exceptions: two locations, every guard, one flat rule.
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
# It is NOT itself wired into pre-commit/CI the way a scripts/check-*.sh guard is: it lives
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
# `invoked_in` reads ci.yml as a flat stream of LINES and cannot see JOB boundaries, so on its own it
# proves a guard is NAMED in ci.yml, not that it RUNS. Those coincide only while every invocation sits
# in one unconditional job.
#
# That precondition is now ASSERTED (see the shape check below), not assumed. Until 2026-08-08 this
# note read "Not a live hole today ... becomes a REQUIRED fix the moment that job is split or
# conditioned" — an audit result standing in for a check, with nothing able to notice the moment it
# named. Adding `if:` to the guards job would have silenced the whole fleet on PRs while this script
# printed `clean (27 guards checked)`.
#
# Closing it did NOT require reading ci.yml as YAML, which is what that note assumed and why it was
# deferred. The property needed is structural — one job, no job-level `if:` — and asserting THAT is a
# line-shape question. Invalidation-checked in all three directions (2026-08-08): a planted job-level
# `if:` reports CONDITIONED, a planted second guard-holding job reports SPLIT, and neutralizing every
# invocation reports ZERO rather than passing on an empty subject set. If a future split really is
# wanted, the YAML rewrite is still the way — but now the tree goes red first instead of quietly
# vouching for a fleet CI never executes.
#
# No deps beyond git + grep + awk. Exit 1 on any violation, listing the exact (guard,
# missing-location) pairs.
set -euo pipefail
cd "$(dirname "$0")/.."

PRE_COMMIT=.githooks/pre-commit
CI=.github/workflows/ci.yml


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
#  2. Skipping comments was not enough: the `.githooks/pre-push` hook of the day (deleted 2026-07-29
#     along with the range-shaped guard it existed to run) printed an ERROR MESSAGE naming the script
#     it had just run (`echo "pre-push: scripts/check-foo.sh FAILED ..."`), and a bare substring test
#     accepted that string as the wiring. Measured: commenting out the one real invocation and leaving
#     the echo behind still reported "clean (18 guards checked)".
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
# only ever reads ci.yml).
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
# ## Rejected: an own-line rule for the invocation
# Requiring the call to STAND ON ITS OWN LINE settles the same mention-vs-real question elsewhere in
# this repo (a marker that must lead its line cannot be quoted inside a sentence). Checked here, and it
# does NOT transfer — measured against this very tree, where NO invocation is line-leading:
#   * .github/workflows/ci.yml — `        run: bash scripts/check-english-source.sh` (YAML key first)
# Requiring line-leading would false-red the file on day one, and false red is the more dangerous
# failure here: a guard that cries wolf gets silenced with an escape hatch, and the escape hatch is
# what turns a lane off for good. (`.githooks/pre-commit` is the one place the own-line rule DOES
# hold, and the check below already uses it: the GUARDS array entry is line-anchored, which is why
# that axis was never fooled by either of the two bugs above.)
#
# ## Known limitation, deliberately not closed
# A string that quotes a COMPLETE invocation still counts — e.g. a hint message
# `echo "on failure run: bash scripts/check-policy-census.sh --update"` inside a ci.yml `run:` block
# would satisfy this check on its own. Closing it needs "is this offset inside a quoted string", and
# the obvious implementations break on real code: naive quote-pairing on a line that nests `$( )`
# inside double quotes (`out="$(VAR="$x" bash scripts/... )"`) pairs the wrong quotes across the
# `$( )` and strips the REAL call — i.e. the cure produces exactly the false red described above. The
# residual is strictly narrower than what was there before (a bare mention no longer counts at all),
# it is confined to one hand-edited file, and it is recorded here rather than left for someone to
# rediscover.
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

# The shared libraries guards source. DERIVED, never hand-listed: a hand list silently stops covering
# the next lib the day it lands, which is exactly what happened when `release-matrix.sh` joined a
# one-name assertion. A guard that sources a missing lib dies at its `. ./scripts/lib/...` line, so the
# value here is a named diagnosis instead of a bare shell error.
# Derived from the SOURCING SITES, not from a listing of `scripts/lib/`. Two reasons the directory is
# the wrong axis: `git ls-files` misses a lib that has not been committed yet (the first run of a new
# lib is exactly when this check earns its keep), and a lib nobody sources cannot cause the failure
# this assertion exists to name. The subject set is "what a guard will try to source", so read that.
# Both POSIX sourcing spellings, `.` and `source`. Matching only one would leave a lib sourced the
# other way outside the subject set while this header claimed completeness — the same shape as the
# `git ls-files` attempt that preceded it, which silently dropped every uncommitted lib.
guard_libs=$(grep -hoE '(^|[;&[:space:]])(\.|source)[[:space:]]+\./scripts/lib/[A-Za-z0-9_-]+\.sh' \
  scripts/*.sh scripts/lib/*.sh .githooks/* 2>/dev/null |
  grep -oE 'scripts/lib/[A-Za-z0-9_-]+\.sh' | sort -u)
lib_count=$(printf '%s\n' "$guard_libs" | grep -c '.')
if [ "$lib_count" -lt 1 ]; then
  echo "check-guards-wired: no guard sources a scripts/lib/*.sh at all -- either every shared library" >&2
  echo "  was inlined, or this extraction stopped matching. An empty set would pass vacuously." >&2
  missing=1; wiring_missing=1
fi
for lib in $guard_libs; do
  [ -f "$lib" ] && continue
  echo "check-guards-wired: $lib -- sourced by a guard but not present. That guard dies at its" >&2
  echo "  '. ./$lib' line with a bare shell error; this names it instead." >&2
  missing=1; wiring_missing=1
done

while IFS= read -r -d '' f; do
  base="$(basename "$f" .sh)"
  count=$((count + 1))

  if ! grep -qE "^[[:space:]]*${base}[[:space:]]*$" "$PRE_COMMIT"; then
    echo "check-guards-wired: ($base, $PRE_COMMIT) -- not wired into pre-commit's GUARDS array"
    missing=1; wiring_missing=1
  fi

  if ! invoked_in "$CI" "$base"; then
    echo "check-guards-wired: ($base, $CI) -- not wired into CI's guards job"
    missing=1; wiring_missing=1
  fi
done < <(list_guards)

# --- the precondition that makes the flat-line reading above VALID ----------------------------------
# `invoked_in` reads ci.yml as a stream of lines, so it proves "this guard is named somewhere in the
# file", not "this guard runs". Those two coincide only while every guard invocation sits in ONE job
# that carries no `if:`. That is a PROPERTY OF ci.yml, and until now it was a sentence in this header
# ("Not a live hole today") with nothing enforcing it — precisely the shape §4.5 forbids: an audit
# result standing in for a check. Someone adding `if: github.event_name != 'pull_request'` to the
# guards job would silence the whole fleet on PRs while this script printed a clean bill.
#
# Enforcing the property needs no YAML parser, because it is about SHAPE, not semantics:
#   1. every `bash scripts/check-*` invocation falls inside exactly ONE job's span, and
#   2. that job declares no job-level `if:`.
# Job spans come from 2-space-indented keys AFTER the top-level `jobs:` key — the `after jobs:` part is
# load-bearing, since `on:`'s own `push:`/`pull_request:` keys sit at the same indent and would
# otherwise read as jobs.
shape="$(awk '
  function indent_of(s,   i) { i = match(s, /[^ ]/); return (i == 0 ? -1 : i - 1) }
  /^[[:space:]]*#/ { next }
  /^jobs:[[:space:]]*$/ { injobs = 1; next }
  injobs && /^  [A-Za-z_][A-Za-z0-9_-]*:[[:space:]]*$/ { job = $0; sub(/:[[:space:]]*$/, "", job); sub(/^  /, "", job); next }
  # A job-level `if:` is indented exactly one step inside the job key.
  injobs && job != "" && /^    if:/ { hasif[job] = 1; next }
  injobs && job != "" && /bash[[:space:]]+scripts\/check-/ { holds[job]++ }
  END {
    n = 0
    for (j in holds) { n++; owner = j }
    if (n == 0) { print "ZERO: no job in this file invokes any scripts/check-* guard"; exit }
    if (n > 1) {
      msg = "SPLIT: guard invocations are spread across " n " jobs:"
      for (j in holds) msg = msg " " j "(" holds[j] ")"
      print msg
      exit
    }
    if (hasif[owner]) print "CONDITIONED: job \047" owner "\047 holds every guard invocation AND declares a job-level if:"
  }
' "$CI")"

if [ -n "$shape" ]; then
  echo "check-guards-wired: FAILED -- $shape" >&2
  echo "  This script proves a guard is NAMED in $CI, not that it RUNS. That inference holds only while" >&2
  echo "  every invocation lives in one unconditional job. It no longer does, so every 'wired' verdict" >&2
  echo "  above is now unproven. Fix by restoring the single unconditional guards job, or by teaching" >&2
  echo "  invoked_in to read jobs -> steps -> run as YAML before splitting/conditioning." >&2
  missing=1
fi

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
  echo ".github/workflows/ci.yml's guards job."
  echo
  echo "If the guard IS wired and this still fails, check the SPELLING: ci.yml invocations"
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

# THIRD SUBJECT (2026-07-29): core.hooksPath must be RELATIVE, or a WORKTREE commit runs the MAIN
# repo's hooks instead of its own. Measured on this clone: the value had drifted to an absolute path
# even though CONTRIBUTING.md's own setup line prescribes a relative one. That drift splits this
# meta-guard in half -- it reads the WORKTREE's files to decide the fleet is wired, while the commit
# actually runs the MAIN repo's hook set. A worktree branch adding a guard would be certified by a
# fleet that never saw it. Relative is safe and was measured 2026-07-29 in a scratch repo: git chdirs
# to the working-tree top level before invoking a hook, so `.githooks` resolves per-worktree AND
# commits from a subdirectory still fire it (root and sub/deeper both ran, pwd at the top level both
# times). UNSET is also fine -- that is CI, where no hook runs and the guards job invokes each script
# directly.
#
# The expected value is DERIVED from CONTRIBUTING.md's setup line rather than spelled here, so the
# guard and the instruction it enforces cannot drift apart -- a second hand-written copy of a policy
# value is the class this repo keeps paying for.
expected_hooks_path="$(sed -n 's/^git config core\.hooksPath \([^ ]*\).*$/\1/p' CONTRIBUTING.md | head -1)"
if [ -z "$expected_hooks_path" ]; then
  echo "check-guards-wired: FAILED -- CONTRIBUTING.md no longer contains a 'git config core.hooksPath"
  echo "  <path>' setup line, so this check has nothing to compare against and cannot judge. That line"
  echo "  is the only place the prescribed value lives; restore it rather than hardcoding a value here."
  exit 1
fi
actual_hooks_path="$(git config --get core.hooksPath 2>/dev/null || true)"
if [ -n "$actual_hooks_path" ] && [ "$actual_hooks_path" != "$expected_hooks_path" ]; then
  echo "check-guards-wired: FAILED -- core.hooksPath is '$actual_hooks_path', not '$expected_hooks_path'."
  echo
  echo "  An absolute (or otherwise non-prescribed) hooksPath makes every WORKTREE commit run the MAIN"
  echo "  repo's hooks. The guard fleet then verifies one directory and protects another, and this very"
  echo "  script would report a worktree's new guard as wired while the commit ran without it."
  echo
  echo "  Fix: git config core.hooksPath $expected_hooks_path"
  echo "  (CONTRIBUTING.md prescribes exactly that; leaving it UNSET is fine only in CI, where the"
  echo "  guards job invokes each script directly and no hook runs at all.)"
  exit 1
fi

echo "check-guards-wired: clean ($count guards checked; ${#workflow_files[@]} workflows, ${wf_refs:-0} workflow_run reference(s) resolved; core.hooksPath ${actual_hooks_path:-unset})."
