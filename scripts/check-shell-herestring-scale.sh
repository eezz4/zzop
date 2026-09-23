#!/usr/bin/env bash
# Machine seal for the SILENT HERE-STRING DEADLOCK class (bit for real 2026-09-08, review ledger V109).
#
# bash implements a here-string as a PIPE, not a temp file, when the string is small enough: it writes
# the whole thing into the pipe and only afterwards execs the reader. The pipe buffer is 65,536 bytes.
# Fill it and the write blocks against a pipe nobody is draining yet, and the script stops -- at 0%
# CPU, with no error, no exit status and nothing for a timeout to catch. It does not fail closed. It
# does not fail at all.
#
# 🔴 IT IS A WINDOW, NOT A CLIFF, and this header said cliff until 2026-09-08 (review ledger V117).
# Bisected on this machine (bash 5.3.9(1)-release, x86_64-pc-cygwin, Git for Windows): a here-string
# whose operand is 65,536..65,663 bytes long DEADLOCKS; 65,535 and below pass, and so does 65,664 and
# ABOVE -- past there bash gives up on the pipe and uses a temp file. The window is 128 values wide.
#
# ⚠ The unit is half the fact. bash appends a newline, so in BYTES WRITTEN the window is
# 65,537..65,664 and the first passing write is 65,665. Quote either number without saying which it
# is and the next person measures the wrong side of it.
#
# ## Which direction a site re-arms from -- there is no single "is it safe today"
# 📏 2026-09-08: `git ls-files -- '*.rs'` is 65,805 bytes, i.e. 142 bytes ABOVE the window. The site
# that hung the fleet is passing today because the repository OVERSHOT, not because the hazard went
# away: that one re-arms if the repo SHRINKS. Meanwhile `check-max-file-lines.sh`'s per-file census
# string is 41,424 bytes over 843 files (49 B/file) -- BELOW the window, arming at about 1,335 files,
# which is the direction repositories actually move. Two sites, opposite approaches, same window.
# ⇒ neither `git bisect` nor "what did we change" finds either one. The only safe state is not
# having the window at all, which is why this is a guard and not a size check.
#
#     while IFS= read -r p; do ...; done <<< "$scanned"    # $scanned = git ls-files output
#
# Measured on this machine (bash 5.3.9(1)-release, x86_64-pc-cygwin, Git for Windows), 2026-09-08:
# `git ls-files -- '*.rs'` had just crossed the ceiling at 65,603 bytes. The same string truncated to
# 64,000 ran in milliseconds; the full 65,603 hung until killed. `< <(printf '%s\n' "$scanned")` --
# process substitution, which gives the reader a live writer instead of a pre-filled buffer -- passed
# the identical 65,603 bytes.
#
# ## Why this is a guard and not a note
# The site that blew up was `assert_workspace_members_scanned` in scripts/lib/tracked-grep.sh, which
# SIX guards call, so a repository that merely grew took the whole fleet down. A pre-commit sat in
# check-max-file-lines for 31 minutes and read, from outside, as "slow". A guard that goes silent is
# worse than a guard that goes green wrongly: a false green is at least a claim someone can check.
#
# And the fleet's own advice pushed toward the trap. check-shell-pipe-sigpipe.sh recommends
# `grep -q <pattern> <<< "$var"` precisely because a here-string has no writer process to receive
# SIGPIPE. That advice is correct and stays -- under 64 KiB. This guard draws the line the advice was
# missing.
#
# ## The needle, stated narrowly enough to be true
# A here-string is flagged only when BOTH hold:
#
#   1. Its operand is a plain variable reference: `<<< "$name"` or `<<< "${name}"`. A literal
#      (`<<< "text"`), a command substitution, and anything unquoted are not seen.
#   2. That same file makes `name` TREE-SCALE, by either of two routes:
#      (a) DIRECTLY -- an assignment whose right-hand side runs a whole-tree enumeration: `git
#          ls-files`, `git grep`, `git diff`, `find`, or one of this repo's own tracked-file helpers
#          (`tracked_grep`, `tracked_files*`, `workspace_member_dirs`, `list_rs_files`).
#      (b) THROUGH A TEMP FILE, added 2026-09-08 (review ledger V117). An enumeration redirected into
#          `> "$f"` makes `$f` a tree-scale FILE, and any later assignment that reads `$f` is
#          tree-scale in turn -- transitively, to a fixed point.
#      Those are the strings whose length is the repository's size, so those are the ones that cross
#      the window by GROWING rather than by being written long.
#
#      🔴 Route (b) is not a generalisation for its own sake: `check-max-file-lines.sh` -- the very
#      script whose 31-minute hang started all of this -- had a SECOND here-string one function later,
#      and route (a) could not see it. `wc` output went to a temp file, the temp file was read back
#      into `all_counts`, and `all_counts` fed `<<<`. The guard was green over the same script, the
#      same hazard and the same failure shape. A needle that only sees the instance it was written
#      for is a record of one incident, not a seal on a class.
#
# Comment lines are skipped, which is what lets this header quote the banned shape.
#
# ## What this guard CANNOT see -- read this before quoting a green run
#   - A variable that gets its enumeration THROUGH A FUNCTION whose body is elsewhere
#     (`x=$(collect_paths)`, defined in another file). Propagation is textual and per file: it follows
#     variables and redirect targets, not call graphs, and there is still no cross-file view.
#   - A tree-scale string built up in a loop (`acc="$acc$line"`) rather than assigned.
#   - A here-string over a large CONSTANT, or over a variable read from a big file. Those can cross
#     64 KiB too. They are excluded because they cross it by being edited, which is a visible act,
#     rather than by the tree growing under a script nobody touched -- which is the class that bit.
#   - Every other 64 KiB pipe-write in the fleet that is not spelled `<<<`.
# The remedy is one line in every case: `<<< "$x"` becomes `< <(printf '%s\n' "$x")`.
#
# Invalidation drills (2026-09-08, all three run):
#   - reverting `assert_workspace_members_scanned`'s `$scanned` loop in scripts/lib/tracked-grep.sh to
#     a here-string makes this guard name that exact line and exit 1;
#   - neutralising `git ls-files` in the needle makes the direct canary abort instead of passing on a
#     set the guard can no longer see;
#   - deleting the temp-file propagation makes the transitive canary abort -- which is what route (b)
#     is for, and the reason it gets a canary of its own rather than sharing the count.
#
# No deps beyond git + awk. Exit 1 on any violation, listing (file, line, variable).
set -euo pipefail
cd "$(dirname "$0")/.."

# The subject set is `scripts/lib/shell-subjects.sh`'s, and it used to be NARROWER than this guard's
# own claim: tracked `scripts/**` plus hooks, which left out untracked scripts, `docs/demo/*.sh`,
# `.claude-plugin/hooks/bootstrap.sh` and every workflow. 68 files against its sibling's 74 — and one
# of the six it could not see was carrying the exact hazard this guard exists to seal.
. "$(dirname "$0")/lib/shell-subjects.sh"
subjects="$(shell_subject_files)"

subject_count=0
while IFS= read -r f; do [ -n "$f" ] && subject_count=$((subject_count + 1)); done \
  < <(printf '%s\n' "$subjects")

# NO MAGIC THRESHOLD HERE ANY MORE (2026-09-13, ledger V212). This read `-lt 20`, and the shared
# population's own header argues against exactly that: a total has to be re-tuned every time the repo
# gains a script, which is how a ratchet becomes a number people bump without reading. The per-pathspec
# floor in `shell_subject_files` catches the narrowing this was reaching for, and catches it derived.

report="$(
  while IFS= read -r f; do
    [ -n "$f" ] || continue
    [ -f "$f" ] || continue
    awk -v file="$f" '
      # Everything happens in END over stored lines. Marking cannot be a streaming decision any more:
      # route (b) is transitive, so a variable can become tree-scale because of a line BELOW the one
      # that uses it. Storing first also keeps the case a single pass always got wrong -- a line can BE
      # the assignment and CARRY the here-string at once (`x=$(grep -v y <<< "$scan")`).
      { lines[NR] = $0 }
      END {
        ENUMRE = "git[ \t]+ls-files|git[ \t]+grep|git[ \t]+diff|tracked_grep|tracked_files|workspace_member_dirs|list_rs_files|[ \t]find[ \t]"

        # ---- route (a): the assignment right-hand side IS an enumeration ----
        pending = ""
        for (i = 1; i <= NR; i++) {
          l = lines[i]
          if (l ~ /^[ \t]*#/) continue
          if (match(l, /^[ \t]*(local[ \t]+|export[ \t]+|readonly[ \t]+|declare[ \t]+[^ \t]+[ \t]+)?[A-Za-z_][A-Za-z0-9_]*(\+)?=/)) {
            eq = index(l, "=")
            head = substr(l, 1, eq - 1)
            rhs = substr(l, eq + 1)
            sub(/\+$/, "", head)
            n = split(head, parts, /[ \t]+/)
            name = parts[n]
            nameof[i] = name
            rhsof[i] = rhs
            if (rhs ~ ENUMRE) { enum[name] = 1; enumcount++; print "MARK " file " " name }
            # A multi-line `name="$(` opener: mark the name if an enumeration shows up before the
            # substitution closes.
            if (rhs ~ /^"?\$\($/) pending = name
            continue
          }
          if (pending != "") {
            if (l ~ /^[ \t]*\)"?[ \t]*$/) { pending = "" }
            else if (l ~ ENUMRE) { enum[pending] = 1; enumcount++; print "MARK " file " " pending; pending = "" }
          }
        }
        print "ENUMCOUNT " enumcount + 0

        # ---- route (b): through a temp file, to a fixed point ----
        # Two steps, alternating until nothing new appears. Files are small and the marked set only
        # grows, so this terminates in a couple of sweeps; the loop is here because a redirect can sit
        # after the read that depends on it.
        changed = 1
        while (changed) {
          changed = 0
          for (i = 1; i <= NR; i++) {
            l = lines[i]
            if (l ~ /^[ \t]*#/) continue

            # (b1) a tree-scale command redirected into a variable-named file makes that FILE tree-scale
            src = 0
            if (l ~ ENUMRE) src = 1
            else { for (v in enum) if (l ~ ("[$][{]?" v "([^A-Za-z0-9_]|$)")) { src = 1; break } }
            if (src && match(l, />[ \t]*"?[$][{]?[A-Za-z_][A-Za-z0-9_]*[}]?"?/)) {
              t = substr(l, RSTART, RLENGTH)
              gsub(/^>[ \t]*"?[$][{]?/, "", t)
              gsub(/[}]?"?$/, "", t)
              if (!(t in efile)) { efile[t] = 1; changed = 1; print "MARKF " file " " t }
            }

            # (b2) an assignment that READS a tree-scale variable or a tree-scale file is tree-scale
            if (i in nameof && !(nameof[i] in enum)) {
              r = rhsof[i]
              hit = 0
              for (v in enum) if (r ~ ("[$][{]?" v "([^A-Za-z0-9_]|$)")) { hit = 1; break }
              if (!hit) { for (v in efile) if (r ~ ("[$][{]?" v "([^A-Za-z0-9_]|$)")) { hit = 1; break } }
              if (hit) { enum[nameof[i]] = 1; changed = 1; print "MARK2 " file " " nameof[i] }
            }
          }
        }

        # ---- the here-strings, over the closed set ----
        for (i = 1; i <= NR; i++) {
          l = lines[i]
          if (l ~ /^[ \t]*#/) continue
          # The quotes are OPTIONAL here, and that is not a leniency (2026-09-13, ledger V209).
          # bash performs NO word splitting on a here-string operand, so `<<< $x` and `<<< "$x"` write
          # exactly the same bytes into exactly the same pipe — identical 64 KiB hazard, and the unquoted
          # spelling was invisible to this needle. An external review planted it and the guard stayed green.
          if (match(l, /<<<[ \t]*"?[$][{]?[A-Za-z_][A-Za-z0-9_]*[}]?"?/)) {
            frag = substr(l, RSTART, RLENGTH)
            gsub(/^<<<[ \t]*"?[$][{]?/, "", frag)
            gsub(/[}]?"?$/, "", frag)
            if (frag in enum) print "HIT " file " " i " " frag
          }
        }
      }
    ' "$f"
  done < <(printf '%s\n' "$subjects")
)"

# The needle's OWN floor. If no assignment in the whole fleet reads as an enumeration any more, this
# guard is looping over an empty marked-set and would pass having proved nothing -- the exact failure
# it exists to remove, one level up.
enum_total=0
while IFS= read -r n; do
  case "$n" in ENUMCOUNT\ *) enum_total=$((enum_total + ${n#ENUMCOUNT })) ;; esac
done < <(printf '%s\n' "$report")

if [ "$enum_total" -lt 10 ]; then
  echo "herestring-scale guard: FAILED -- matched only $enum_total whole-tree enumeration(s) across" >&2
  echo "$subject_count shell script(s). This fleet is built on git ls-files; a number this small means" >&2
  echo "the enumeration needle stopped matching, so the marked-variable set is vacuous." >&2
  exit 1
fi

# The floor above is a count, and a count is too soft on its own: neutralising the single most
# important needle (`git ls-files`) still left 21 marks here, which clears any total floor worth
# writing while the guard has gone blind to the exact assignment that caused V109. So the floor also
# names a CANARY -- `scanned` in scripts/lib/tracked-grep.sh, the variable whose here-string hung the fleet.
# If the needle stops recognising that one, this guard is not narrower, it is off.
if ! grep -qxF "MARK scripts/lib/tracked-grep.sh scanned" < <(printf '%s\n' "$report"); then
  echo "herestring-scale guard: FAILED -- the canary is not marked. \`scanned\` in" >&2
  echo "scripts/lib/tracked-grep.sh is assigned from \`git ls-files\` and is the very string whose here-string" >&2
  echo "hung the whole guard fleet on 2026-09-08, so the needle MUST see it. It does not, which means" >&2
  echo "the extraction is broken (or that assignment moved) -- not that the fleet is clean." >&2
  exit 1
fi

# Route (b) gets its OWN canary, and for the same reason route (a) has one: the count above cannot
# tell "the propagation works" from "the propagation marks nothing". The named variable is
# `all_counts` in check-max-file-lines.sh -- the second site, reached only through the temp file
# (`list_rs_files ... > "$wc_out"`, then `all_counts=$(awk ... "$wc_out")`). If that stops being
# marked, route (b) is off, and a green run says nothing about the class it was added for.
if ! grep -qxF "MARK2 scripts/check-max-file-lines.sh all_counts" < <(printf '%s\n' "$report"); then
  echo "herestring-scale guard: FAILED -- the transitive canary is not marked. \`all_counts\` in" >&2
  echo "scripts/check-max-file-lines.sh is built by reading a temp file that a whole-tree enumeration" >&2
  echo "wrote, which is exactly the route added on 2026-09-08 (review ledger V117). The needle no longer" >&2
  echo "follows it, so this run proves nothing about that class -- fix the propagation, do not delete" >&2
  echo "this check." >&2
  exit 1
fi

hits="$(printf '%s\n' "$report" | grep '^HIT ' || true)"
if [ -n "$hits" ]; then
  echo "herestring-scale guard: here-string(s) fed by a repository-sized string." >&2
  while IFS= read -r h; do
    [ -n "$h" ] || continue
    set -- $h
    echo "  $2:$3 — <<< \"\$$4\", and \$$4 is tree-scale in that file (assigned from a whole-tree" >&2
    echo "    enumeration, or read back from a temp file one wrote)." >&2
  done < <(printf '%s\n' "$hits")
  echo >&2
  echo "bash writes a here-string into a 65,536-byte pipe BEFORE the reader exists. At 65,536..65,663" >&2
  echo "bytes of operand the script hangs with no error and no exit (above that bash uses a temp file --" >&2
  echo "it is a window, so passing today means nothing tomorrow). Replace each with:" >&2
  echo "    < <(printf '%s\\n' \"\$var\")" >&2
  exit 1
fi

mark2_total=$(printf '%s\n' "$report" | grep -c '^MARK2 ' || true)
echo "herestring-scale guard: clean ($subject_count shell scripts, $enum_total enumeration-fed variables + $mark2_total reached through a temp file, no here-string over any)."
