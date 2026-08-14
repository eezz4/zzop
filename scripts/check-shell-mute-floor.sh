#!/usr/bin/env bash
# Machine seal for the UNREACHABLE EMPTY-SET FLOOR class (bit for real 2026-08-14, seven live sites in
# one sweep; first single instance 7b73ca7).
#
# Under `set -e -o pipefail`, an assignment from a command substitution takes the status of the
# pipeline's LAST FAILING stage — not its last stage — and a non-zero status there kills the script ON
# THE ASSIGNMENT. So this shape:
#
#     n=$(grep -rl 'needle' "${roots[@]}" | wc -l)      # grep exits 1 when nothing matches
#     if [ "$n" -lt 20 ]; then abort "the scan roots are wrong"; fi
#
# never reaches its own diagnosis. When the subject set genuinely collapses to zero — the ONE case the
# floor was written for — the script dies at the assignment with exit 1 and ZERO BYTES of output. It
# fails closed, but MUTE, which on screen is indistinguishable from a real finding, and the abort text
# that would have named the broken extraction is unreachable code. That is strictly worse than having
# no floor: the floor is quoted as proof the axis is covered.
#
# The rule this file seals: if a floor is written below such an assignment, the assignment must let the
# floor RUN. `|| true` there is not hygiene, it is load-bearing.
#
# ## What counts as letting the floor run
# Any of these, all accepted (this guard REQUIRES none of them in particular):
#   x=$(... || true)  /  x=$(...) || true        # swallow the status, reach the floor
#   set +e; x=$(...); rc=$?; set -e              # PREFERRED — see below
#   an `if`/`while` head, or a `local`/`export`/`readonly`/`declare` prefix (errexit is suspended or
#   the builtin's own status is what propagates; both measured 2026-08-14)
#
# `set +e; out=$(...); rc=$?; set -e` is the better idiom and this repo already uses it twice
# (check-rules-catalog-sync.sh:71, check-framework-prose-enumeration.sh:94): it separates rc==1 ("no
# match", legitimate) from rc>1 ("grep itself broke"), where `|| true` swallows both and lets a broken
# extraction read as an empty subject set. It is deliberately NOT REQUIRED here. Requiring it would
# red-flag all seven sites repaired in 334ceac, each of which chose `|| true` with a measured comment
# explaining why — turning a guard's first run into a seven-site rewrite is how a guard gets an escape
# hatch bolted on instead of being obeyed. The preference lives in 0.guides §5.5 where it can be read;
# this file enforces the reachability, not the spelling.
#
# ## The needle, stated narrowly enough to be true
# A line is flagged only when ALL of these hold. Anything outside this conjunction is NOT seen by this
# guard, whatever the header's opening paragraph makes you feel:
#
#   1. The line BEGINS with a bare assignment `var=...` (also `var+=`, `var[k]=`) whose right-hand
#      side contains a command substitution `$(...)`. Arithmetic `$((...))` is skipped. Beginning the
#      line is what excludes the shapes measured non-lethal on 2026-08-14 — `local`/`export`/
#      `readonly`/`declare x=$(...)` propagate the BUILTIN's status, and `if`/`while x=$(...)` sits in
#      a condition where errexit is suspended. Indentation is fine: the seventh repaired site lives
#      inside a `while read` loop.
#   2. errexit is ON at that point in the file — parsed from the `set` lines actually executed above it
#      (see the flag parser below), not assumed.
#   3. Some stage of that substitution's pipeline is spelled with one of the FALLIBLE commands below.
#      Without `pipefail` only the LAST stage is examined, since only its status survives.
#   4. Nothing on the logical line contains `||`.
#   5. A FLOOR sits within 12 lines below, on a variable REACHABLE from the assigned one.
#
# ### 3 — FALLIBLE is a hand list, and it is a list of HAZARDS, not of subjects
# The subject set (which files, which lines) is derived; this is the definition of the defect, which
# cannot be derived from the tree. Members, with the reason each is fallible on an outcome the floor
# below is written for:
#   grep egrep fgrep rg   exit 1 when nothing matches — the empty subject set itself
#   comm diff cmp         exit 1 on difference / unreadable input
#   du                    exits non-zero on an unreadable tree WHILE PRINTING A USABLE TOTAL
#                         (preflight-build-env.sh:42 was exactly this, and its last stage was `awk`)
#   git, EXCEPT `git ls-files`  `git grep`/`git diff`/`git describe`/`git config --get`/`git rev-parse`
#                         all exit non-zero on ordinary negative answers. `git ls-files` exits 0 on an
#                         unmatched pathspec, which is why five live sites spell their floor without
#                         `|| true` and are correct to.
# NOT SEEN, therefore: `jq -e`, `find` on an unreadable dir, `node`/`cargo`/any project binary, `cat`
# on a missing file, and every SHELL FUNCTION (`tracked_files_matching`, `extract`, …) — a function's
# body can fail and this needle reads one word. Measured 2026-08-14: inverting the list (everything
# except a whitelist of pure text filters is fallible) produced 41 flags fleet-wide of which the
# overwhelming majority were `git ls-files` and repo shell functions that cannot fail on an empty
# result. The inverse needle is the honest one and it is unusable; this one is narrow and true.
#
# ### 5 — REACHABLE means DOWNSTREAM, and the missed shape is named
# `reach` starts as the assigned variable and grows by one rule: a later assignment whose right-hand
# side MENTIONS a variable already in `reach` joins it. So a floor on a derived counter counts
# (`guard_libs` -> `lib_count=$(printf … "$guard_libs" | grep -c .)` -> `[ "$lib_count" -lt 1 ]`).
#
# What this does NOT see: a floor on a SIBLING — a variable that shares an INPUT with the assignment
# instead of deriving from it. The measured instance is check-docs-rule-ids.sh:212, where
# `valid_ids=$(printf … "$catalog_ids" "$pack_ids" | grep -v '^$' | sort -u)` is muted by the same
# collapse that the floors on `catalog_id_count`/`pack_id_count` four lines below diagnose — but those
# counters descend from `catalog_ids`, not from `valid_ids`. Recall against the seven sites 334ceac
# repaired is therefore 6/7, measured by running this needle against 334ceac^.
#
# The sibling lane was BUILT AND REJECTED, not overlooked. Walking one hop UP (to the assignment's
# inputs) and back down catches that seventh site — and simultaneously false-flags
# check-server-json-hashes.sh:216, whose `pkg_count=$(printf … "$pkg_platforms" | grep -c .)` has a
# floor on `bad`, a sibling through `pkg_platforms`, and is CORRECT to omit `|| true` because the
# `[ -n "$pkg_platforms" ]` abort above it already refused the empty case. The two are the same shape
# to any line-shaped needle; the only thing separating them is whether the shared source's own floor
# sits above or below, which is a second inference stacked on a heuristic. A false RED on a
# deliberately asymmetric line whose comment says "do NOT add `|| true` for symmetry" would be this
# guard arguing with a measurement. One documented blind spot beats one manufactured contradiction.
#
# ## File scope: DERIVED, same axis as check-shell-pipe-sigpipe.sh
# Every git-known `*.sh` plus everything under `.githooks/` (hooks carry no extension), tracked AND
# untracked-but-not-ignored — a new script must be caught before its first `git add`.
# `.github/workflows/*.yml` is deliberately OUT, unlike in that sibling: a `run:` block is a fresh
# shell whose errexit comes from the runner's `shell:` setting rather than from any `set` line this
# needle could read, so premise 2 would be assumed rather than parsed. Named as a residual, not closed.
#
# ## errexit is PARSED, never sniffed
# `set -uo pipefail` (check-version-relative-prose.sh, the fleet's only one) does NOT enable errexit,
# and its `:77` looks exactly like this defect while its floor is genuinely reachable. A `*e*` test
# matches the `e` in "pipefail" and gets that file wrong — measured, 2026-08-14, in the sweep that was
# hunting this very class. So `set` lines are tokenised: `-o`/`+o` consume their next word as an option
# NAME (never as flag letters), and only a `-abc`-shaped cluster contributes letters. State is tracked
# top-to-bottom, so a `set +e` … `set -e` window (scripts/lib/tracked-grep.sh) reads as errexit-off
# inside it. Files that are SOURCED start at errexit-ON because they inherit the caller's shell; that
# set is derived from the sourcing sites, the same way check-guards-wired.sh derives it, never listed.
#
# ## Other residuals, stated
#   * errexit is suspended inside a function invoked from a condition (`if myfunc; then`). This needle
#     reads file position, not call context, so such an assignment reads as errexit-on.
#   * Single-quoted regions are blanked before parsing, so a `grep` living inside an awk program is not
#     read as a pipeline stage (check-shell-pipe-sigpipe.sh:112 is the live instance). An apostrophe
#     used as an apostrophe inside a double-quoted string on an assignment line would desync that
#     state; no line in the subject set does it.
#   * `mapfile`/`while … < <(…)` are out of premise 1 by construction: process-substitution status is
#     not checked by errexit at all (measured), so those shapes cannot die this way.
set -euo pipefail
cd "$(dirname "$0")/.."

# NOTE: this file does NOT exempt itself from its own scan, and that is measured rather than assumed
# (2026-08-14: with the exclusion removed it reports clean on 51 files including itself). Its sibling
# check-shell-pipe-sigpipe.sh must exempt itself because its header spells the banned pipeline in
# code-shaped prose; the shapes here are only ever spelled inside `#` comments, which the needle skips,
# and every live assignment below carries the `|| true` it demands of everyone else. A guard for muted
# diagnoses that excused itself would be the same joke as one that misattributes its own line numbers.

# `|| true` here is this file's own instance of the class it seals: `grep -vE` exits 1 when it filters
# nothing out, which is the normal case, and the zero-file abort below would never print without it.
files="$(
  {
    git ls-files -- '*.sh' '.githooks/*'
    git ls-files --others --exclude-standard -- '*.sh' '.githooks/*'
  } | sort -u | grep -vE '(^|/)(target|node_modules)/' || true
)"

# Which files inherit errexit instead of declaring it. Derived from the SOURCING SITES for the reason
# check-guards-wired.sh:161-171 gives: a lib that has not been committed yet is exactly the one whose
# first run matters, and a lib nobody sources cannot inherit anything. Both POSIX spellings.
# `|| true` on both stages, same class as everything else in this file.
sourced="$(
  grep -hoE '(^|[;&[:space:]])(\.|source)[[:space:]]+\./scripts/lib/[A-Za-z0-9_-]+\.sh' \
    scripts/*.sh scripts/lib/*.sh .githooks/* 2>/dev/null |
    grep -oE 'scripts/lib/[A-Za-z0-9_-]+\.sh' | sort -u || true
)"

# Non-emptiness floor on THAT extraction, because a collapse there is silent in the worst direction:
# every lib would read as errexit-OFF and drop out of the scan while the count printed below stayed
# unchanged. Measured 2026-08-14 by pointing the needle at a directory that does not exist — without
# this the run stayed green on 51 files and said nothing. The fleet already requires at least one
# sourced lib (check-guards-wired.sh's own `lib_count -lt 1` abort), so zero is never healthy here.
if [ -z "$sourced" ]; then
  echo "check-shell-mute-floor: FAILED -- found ZERO sourced scripts/lib/*.sh. Those files declare no" >&2
  echo "  'set -e' of their own and are judged by INHERITING the caller's; with this list empty every" >&2
  echo "  one of them silently drops out of the scan and this guard reports on a smaller fleet than it" >&2
  echo "  names. Fix the sourcing-site extraction above, not this floor." >&2
  exit 1
fi

# ## Empty-enumeration floor, declared BEFORE awk runs
# `awk 'prog'` with no file operands reads STDIN, so an empty list would hang or certify silence rather
# than report an empty scan — check-shell-pipe-sigpipe.sh:87-92 records the same ordering for the same
# reason. A derived enumeration that comes back empty must abort loudly.
scanned=0
files_arr=()
if [ -n "$files" ]; then
  while IFS= read -r f; do
    [ -f "$f" ] || continue          # index may list a file deleted in the working tree
    files_arr+=("$f")
    scanned=$((scanned + 1))
  done <<< "$files"
fi

if [ "$scanned" -eq 0 ]; then
  echo "check-shell-mute-floor: FAILED -- enumerated ZERO shell files. 'git ls-files -- \"*.sh\"" >&2
  echo "  \".githooks/*\"' returned nothing, so this guard would have vouched for nothing. Either this" >&2
  echo "  is not a git work tree, or the repo genuinely has no shell left; neither is a clean run." >&2
  exit 1
fi

hits="$(awk -v SOURCED="$sourced" '
BEGIN {
  WINDOW = 12
  # See the header. Two-word `git <sub>` handling lives in fallible_cmd().
  split("grep egrep fgrep rg comm diff cmp du git", f, " ")
  for (i in f) FALLIBLE[f[i]] = 1
  n = split(SOURCED, s, "[[:space:]\n]+")
  for (i = 1; i <= n; i++) if (s[i] != "") ISLIB[s[i]] = 1
}

# Buffer one file at a time. bufname is assigned exactly when the buffer is EMPTY, so it always names
# the file whose lines are in it. That is not a style choice: the prototype for this guard read
# FILENAME inside the deferred scan, where FILENAME has already advanced to the NEXT file, and
# reported its one true finding against a sibling file 13 lines too short to contain the line number
# it printed. A guard for muted diagnoses that misattributes its own diagnosis is the worst joke in
# this repo, so the buffer carries its own name.
FNR == 1 { if (nf > 0) scan(); nf = 0; delete L }
{ if (nf == 0) bufname = FILENAME; L[++nf] = $0 }
END { if (nf > 0) scan() }

# Blank every single-quoted region, carrying quote state across physical lines via SQ.
function deq(s, st,   out, i, c) {
  out = ""
  for (i = 1; i <= length(s); i++) {
    c = substr(s, i, 1)
    if (c == "\047") { st = 1 - st; out = out " "; continue }
    out = out (st ? " " : c)
  }
  SQ = st
  return out
}

function unbalanced(s,   t, o, c) { t = s; o = gsub(/\(/, "", t); t = s; c = gsub(/\)/, "", t); return (o > c) }

# First real word of a pipeline stage: skip env-var prefixes and a leading `!`.
function firstword(stage,   n, w, i) {
  n = split(stage, w, /[[:space:]]+/)
  for (i = 1; i <= n; i++) {
    if (w[i] == "") continue
    if (w[i] ~ /^[A-Za-z_][A-Za-z0-9_]*=/) continue
    if (w[i] == "!" || w[i] == "command" || w[i] == "time" || w[i] == "sudo") continue
    # The SUBCOMMAND is what decides whether a git call is fallible, and the global flags sit in
    # front of it. `-C <dir>` / `-c <k=v>` take a SEPARATE argument, so skipping only `-`-leading
    # words reads the DIRECTORY as the subcommand — measured on check-rules-catalog-sync.sh:177
    # (`git -C "$repo_root" ls-files`), which this guard false-flagged until those two were taught.
    # (No apostrophe anywhere in this awk program: the whole thing is a single-quoted shell string.
    # Its siblings check-guards-wired.sh:140 and :480 carry the same restriction for the same reason.)
    if (w[i] == "git") {
      for (i = i + 1; i <= n; i++) {
        if (w[i] == "") continue
        if (w[i] == "-C" || w[i] == "-c" || w[i] == "--git-dir" || w[i] == "--work-tree" || w[i] == "--namespace") { i++; continue }
        if (w[i] ~ /^-/) continue
        return "git " w[i]
      }
      return "git"
    }
    return w[i]
  }
  return ""
}

# "" when the stage cannot fail on a legitimately empty/negative result.
function fallible_cmd(cmd) {
  if (cmd ~ /^git /) return (cmd == "git ls-files") ? "" : cmd   # ls-files exits 0 on no match
  return (cmd in FALLIBLE) ? cmd : ""
}

function mentions(rhs, set,   v) {
  for (v in set)
    if (rhs ~ ("\\$\\{?" v "[^A-Za-z0-9_]") || rhs ~ ("\\$\\{?" v "$")) return 1
  return 0
}

# The variable an emptiness/zero floor judges, or "". Numeric comparisons must be against a LITERAL:
# `[ "$a" -ge "$b" ]` is an ordering test between two results, not a floor under one of them, and
# reading it as a floor false-flags check-git-spawn-isolation.sh:97-98 (measured).
function floorof(s,   m) {
  if (match(s, /\[\[?[[:space:]]+(![[:space:]]*)?-[zn][[:space:]]+"?\$\{?[A-Za-z_][A-Za-z0-9_]*/)) {
    m = substr(s, RSTART, RLENGTH); sub(/.*\$\{?/, "", m); return m
  }
  if (match(s, /\[\[?[[:space:]]+"?\$\{?[A-Za-z_][A-Za-z0-9_]*[}"]*[[:space:]]+-(lt|le|gt|ge|eq|ne)[[:space:]]+"?[0-9]+"?/)) {
    m = substr(s, RSTART, RLENGTH); sub(/^\[\[?[[:space:]]+"?\$\{?/, "", m); sub(/[^A-Za-z0-9_].*/, "", m); return m
  }
  if (match(s, /\$\{[A-Za-z_][A-Za-z0-9_]*:[-=]/)) {                 # ${x:-fallback} IS a floor
    m = substr(s, RSTART, RLENGTH); sub(/^\$\{/, "", m); sub(/:[-=]$/, "", m); return m
  }
  return ""
}

function scan(   i, j, k, c, ERR, PF, on, line, w, n, t, nx, body, seg, rest, end, st, depth, p, q,
                 stages, ns, cmd, bad, var, rhs, reach, changed, hit, hitline, s) {
  # A sourced lib declares nothing of its own and runs under whatever flags the caller had; every
  # sourcing site in this tree is `set -euo pipefail`. Everything else starts where bash really
  # starts: errexit OFF until a `set` line turns it on.
  ERR = (bufname in ISLIB) ? 1 : 0
  PF  = (bufname in ISLIB) ? 1 : 0

  for (i = 1; i <= nf; i++) {
    line = L[i]
    if (line ~ /^[[:space:]]*#/) continue

    if (line ~ /^[[:space:]]*set[[:space:]]+[-+]/) {
      n = split(line, w, /[[:space:]]+/)
      for (k = 1; k <= n; k++) if (w[k] == "set") break
      for (k = k + 1; k <= n; k++) {
        t = w[k]
        if (t == "") continue
        # `-o`/`+o` consume the NEXT WORD as an option name. This is the whole reason `pipefail` never
        # donates its `e` to the errexit verdict.
        if (t == "-o" || t == "+o") {
          nx = (k < n) ? w[k+1] : ""
          if (nx == "errexit") ERR = (t == "-o"); else if (nx == "pipefail") PF = (t == "-o")
          k++; continue
        }
        if (t ~ /^-[a-zA-Z]+$/ || t ~ /^\+[a-zA-Z]+$/) {
          on = (substr(t, 1, 1) == "-")
          if (index(t, "e")) ERR = on
          # a cluster ending in `o` (`set -euo pipefail`) still consumes the next word as a name
          if (t ~ /o$/) { nx = (k < n) ? w[k+1] : ""; if (nx == "pipefail") { PF = on; k++ } else if (nx == "errexit") { ERR = on; k++ } }
          continue
        }
        break
      }
      continue
    }

    if (!ERR) continue
    if (line !~ /^[[:space:]]*[A-Za-z_][A-Za-z0-9_]*(\[[^]]*\])?\+?=/) continue
    if (line !~ /\$\(/) continue

    var = line; sub(/^[[:space:]]*/, "", var); sub(/(\[[^]]*\])?\+?=.*/, "", var)

    # Accumulate physical lines until the parens of the substitution balance, blanking quoted text.
    SQ = 0
    body = deq(L[i], 0); st = SQ
    end = i
    while (unbalanced(body) && end - i < 8 && end < nf) { end++; body = body " " deq(L[end], st); st = SQ }

    if (index(body, "||")) continue              # `|| true`, `|| rc=$?`, `|| abort ...` — status caught

    sub(/^[^=]*=/, "", body)

    # Every `$(` on the logical line, skipping `$((` arithmetic; a fallible stage in ANY of them is
    # enough. `rest` is consumed left to right so nesting is visited too.
    bad = ""
    rest = body
    while (bad == "") {
      p = index(rest, "$(")
      if (p == 0) break
      if (substr(rest, p + 2, 1) == "(") { rest = substr(rest, p + 3); continue }
      seg = substr(rest, p + 2)
      rest = seg
      depth = 1; q = 0
      for (c = 1; c <= length(seg); c++) {
        t = substr(seg, c, 1)
        if (t == "(") depth++
        else if (t == ")") { depth--; if (depth == 0) { q = c; break } }
      }
      if (q > 0) seg = substr(seg, 1, q - 1)
      ns = split(seg, stages, "[|]")
      for (j = 1; j <= ns; j++) {
        if (!PF && j < ns) continue              # without pipefail only the LAST stage sets the status
        cmd = fallible_cmd(firstword(stages[j]))
        if (cmd != "") { bad = cmd; break }
      }
    }
    if (bad == "") continue

    # Downstream closure, then a floor on anything inside it. See the header for the sibling shape
    # this deliberately does not reach.
    delete reach; reach[var] = 1
    for (k = 0; k < 4; k++) {
      changed = 0
      for (j = i; j <= end + WINDOW && j <= nf; j++) {
        s = L[j]
        if (s ~ /^[[:space:]]*#/) continue
        if (s !~ /^[[:space:]]*(local[[:space:]]+|export[[:space:]]+|readonly[[:space:]]+|declare[[:space:]]+[-a-zA-Z]*[[:space:]]+)?[A-Za-z_][A-Za-z0-9_]*(\[[^]]*\])?\+?=/) continue
        t = s; sub(/^[[:space:]]*(local[[:space:]]+|export[[:space:]]+|readonly[[:space:]]+|declare[[:space:]]+[-a-zA-Z]*[[:space:]]+)?/, "", t)
        rhs = t; sub(/^[^=]*=/, "", rhs)
        sub(/(\[[^]]*\])?\+?=.*/, "", t)
        if (t in reach) continue
        if (mentions(rhs, reach)) { reach[t] = 1; changed = 1 }
      }
      if (!changed) break
    }

    hit = ""
    for (j = end + 1; j <= end + WINDOW && j <= nf; j++) {
      s = L[j]
      if (s ~ /^[[:space:]]*#/) continue
      t = floorof(s)
      if (t != "" && (t in reach)) { hit = j; hitline = s; break }
    }
    if (hit == "") continue

    sub(/^[[:space:]]*/, "", hitline)
    printf "%s:%d: %s=$(... %s ...) under set -e -o pipefail, no `|| true`\n", bufname, i, var, bad
    printf "%s:%d:   the floor it mutes: %.100s\n", bufname, hit, hitline
  }
}
' "${files_arr[@]}")"

if [ -n "$hits" ]; then
  printf '%s\n' "$hits" >&2
  echo >&2
  echo "check-shell-mute-floor: FAILED — each assignment above can exit non-zero on a legitimately" >&2
  echo "  empty result, and under 'set -e -o pipefail' that kills the script ON THE ASSIGNMENT, before" >&2
  echo "  the floor written to diagnose exactly that state can print. The run fails closed but MUTE:" >&2
  echo "  exit 1, zero bytes, indistinguishable on screen from a real finding." >&2
  echo >&2
  echo "  Fix by letting the floor run — 'x=\$(... || true)', or the idiom that tells 'no match' apart" >&2
  echo "  from 'the extraction broke':  set +e; x=\$(...); rc=\$?; set -e   (see this script's header," >&2
  echo "  and check-rules-catalog-sync.sh:71 for a live example). Do NOT delete the floor instead." >&2
  exit 1
fi

echo "check-shell-mute-floor: OK (no muted empty-set floor in $scanned files: every git-known *.sh and .githooks/*)"
