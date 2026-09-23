#!/usr/bin/env bash
# THE SHELL-FILE POPULATION, with one owner.
#
# Three guards seal three different hazards over what is nominally the same subject set — every piece
# of shell this repo ships:
#
#   check-shell-pipe-sigpipe.sh     a producer dying of SIGPIPE into an early-exiting reader
#   check-shell-mute-floor.sh       a derived enumeration that comes back empty and certifies silence
#   check-shell-herestring-scale.sh a here-string whose operand crosses the 64 KiB pipe window
#
# 🔴 They had THREE DIFFERENT populations, and nobody had put the three lists side by side
# (2026-09-13, external review round 18, ledger V201). Measured on that tree: pipe-sigpipe saw 74
# files, mute-floor 71 (no workflows), herestring-scale 68 (no workflows, no untracked files, and
# `scripts/**` only — so `docs/demo/*.sh` and `.claude-plugin/hooks/bootstrap.sh` were outside it).
#
# The gap was not theoretical. `.github/workflows/prebuild.yml` carried a here-string over a command's
# captured stdout, written DELIBERATELY to dodge the SIGPIPE class its sibling guard seals — and it
# landed on the here-string class, in one of the files the here-string guard could not see. Two
# hazards, and the documented remedy for one is the trigger for the other.
#
# So the list lives here once. A guard that wants a narrower set subtracts from this one visibly,
# rather than writing its own `git ls-files` and drifting by omission.
#
# ## What is in it, and why each part
#   *.sh                     every shell script, wherever it sits — not just `scripts/`.
#   .githooks/*              the hook bodies, which have no extension.
#   .github/workflows/*.yml  workflow `run:` blocks ARE shell, and they run on the release lane where
#   .github/workflows/*.yaml a silent hang costs the most. Both spellings: GitHub loads either, and
#                            check-guards-wired.sh already enumerates both for the same reason.
#   --others --exclude-standard   untracked-but-not-ignored files: a script added and not yet
#                            committed still runs, and the pre-commit fleet reads the WORKING TREE.
#
# ⚠ THE FLOOR BELOW DOES NOT COVER THE UNTRACKED HALF, and cannot in this shape (2026-09-13, ledger
# V210). Each class is checked for "did anything match", and the TRACKED half satisfies every class on
# its own, because normally nothing is untracked. Drilled: replacing the `--others` pathspecs with a
# non-matching one and then planting an untracked violation stays green at 76 files. That is structural
# — there is no derived non-zero to assert about a set that is legitimately empty most of the time — so
# it is stated rather than papered over. An earlier draft of this header implied the same reasoning
# covered both halves; it does not.
#
# `target/`/`node_modules/` are filtered rather than trusted to be ignored: `--others
# --exclude-standard` never lists a git-ignored path, so those can only arrive by being TRACKED, and
# the filter is the belt for that case (same reasoning as scripts/lib/tracked-grep.sh's exclusions).
#
# ## Usage
#   . "$(dirname "$0")/lib/shell-subjects.sh"
#   files="$(shell_subject_files)"
# Each caller still owns its own EMPTY floor, whose message names that guard and its own scan. What
# this file owns is a stronger floor the callers structurally cannot have — see below.
#
# ## Why the floor here is PER-PATHSPEC and not "is the list empty"
# 🔴 Found by drilling this very file (2026-09-13): changing `*.sh` to `*.NOPE` took the population
# from 75 files to 8 — every shell script in the repo gone — and TWO OF THE THREE GUARDS STILL EXITED
# 0. Their floors ask "did I enumerate nothing?", and the answer was no: the hooks and the workflows
# were still there. A floor that only catches ZERO cannot catch a 90% narrowing, and a narrowing is
# the shape this population actually failed in — it was 68 against its sibling's 74 for months.
#
# So the check is that EVERY pathspec class still contributes. That is derived rather than a
# threshold: this repo cannot be in a state where it has no `*.sh`, no hooks, or no workflows, so a
# zero from any one of them is a broken needle and never a fact about the tree. A magic total ("at
# least 70") would have to be re-tuned on every commit that adds a script, which is how a ratchet
# becomes a thing people bump without reading.
#
# It aborts rather than returning, because a caller cannot sensibly continue on a population that
# cannot see its own subject — and because the callers demonstrably did continue.

# Prints one path per line, sorted and deduplicated. Aborts (exit 1) if any pathspec class came back
# empty, naming which one.
shell_subject_files() {
  local spec label found
  # Each entry is "<label>|<pathspec>...". The labels are what the abort message says out loud.
  for spec in \
    "shell scripts|*.sh" \
    "git hooks|.githooks/*" \
    "workflows|.github/workflows/*.yml .github/workflows/*.yaml"; do
    label="${spec%%|*}"
    # shellcheck disable=SC2086 # the pathspecs are ours and deliberately word-split
    found="$(
      {
        git ls-files -- ${spec#*|}
        git ls-files --others --exclude-standard -- ${spec#*|}
      } | sort -u | grep -vE '(^|/)(target|node_modules)/' | head -1 || true
    )"
    if [ -z "$found" ]; then
      echo "shell-subjects: FAILED -- the '$label' pathspec matched NOTHING." >&2
      echo "  This repo cannot be in that state, so the needle is broken rather than the tree empty." >&2
      echo "  Every guard sourcing this file would otherwise have scanned a silently smaller set and" >&2
      echo "  reported OK over it -- measured: breaking '*.sh' took the population from 75 to 8 and" >&2
      echo "  two of the three guards still exited 0." >&2
      exit 1
    fi
  done

  {
    git ls-files -- '*.sh' '.githooks/*' '.github/workflows/*.yml' '.github/workflows/*.yaml'
    git ls-files --others --exclude-standard -- \
      '*.sh' '.githooks/*' '.github/workflows/*.yml' '.github/workflows/*.yaml'
  } | sort -u | grep -vE '(^|/)(target|node_modules)/' || true
}
