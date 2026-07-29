#!/usr/bin/env bash
# tracked-grep.sh — shared file-enumeration helpers for the isolation/scope guards
# (check-syn-isolation.sh, check-tree-sitter-isolation.sh, check-swc-isolation.sh,
# check-ruff-isolation.sh, ...). Extracted because the
#   git ls-files -z -- '<globs>' | xargs -0 -r grep -lP "$PAT" -- 2>/dev/null | grep -v ... || true
# idiom was copy-pasted across those scripts, and the exclusion sets had already drifted (the
# isolation guards omitted node_modules/; check-english-source.sh/check-max-file-lines.sh included
# it; some guards anchored `.claude/` as `^\.claude/`, others as `(^|/)\.claude/`). That drift is
# exactly the kind of gap that let the recent corpus-scan bug land in only SOME guards. This file
# is sourced (not executed) by its callers after they `cd` to the repo root: they already do
# `cd "$(dirname "$0")/.."`, so `. ./scripts/lib/tracked-grep.sh` resolves from any of them.
#
# NOT a guard itself: this lives in scripts/lib/, not scripts/check-*.sh, so check-guards-wired.sh's
# `scripts/check-*.sh` glob does not (and must not) pick it up as something requiring pre-commit/CI
# wiring of its own — it has no independent exit status, only functions callers use.
#
# Tradeoff accepted by extracting this: callers are no longer standalone one-file scripts; each
# depends on this lib existing and being sourceable. Mitigated by keeping the lib in-repo (no
# external fetch) and having check-guards-wired.sh assert its file exists.
#
# Standard exclusions (the piece that unifies the drift described above): anything under target/,
# node_modules/, or .claude/ — all three anchored `(^|/)` so a path SEGMENT must match, never a bare
# substring (a hypothetical `my_node_modules_helper.rs` must not be excluded). `.claude/` is untracked
# by git entirely per CLAUDE.md's global-ignore policy, so this exclusion is normally a no-op against
# `git ls-files` output alone; kept anyway as a defense-in-depth belt (and load-bearing for the
# --others-including variant below, which CAN surface untracked paths) in case that policy ever
# lapses locally — same rationale the original per-guard `grep -v` lines already carried.
#
# Hardening (the pipefail-hardening class check-max-file-lines.sh's PIPESTATUS fix introduced,
# applied here instead of the fragile `$(... || true)` swallow every call site used before): each
# enumeration pipeline's real exit statuses are captured via PIPESTATUS rather than trusted to
# propagate through a `var=$(... || true)` substitution, where the trailing `|| true` — there only to
# tame grep's ordinary "no match" exit code — silently absorbs a genuine `git ls-files` failure
# (corrupt index, bad ref, ...) too, letting a guard report "clean" on a producer that never actually
# ran. Two different failure signals are distinguished:
#   - `git ls-files` (and `sort`) exit status: checked directly. A pathspec matching zero files is
#     NOT an error (git ls-files exits 0), so any nonzero here is a real problem.
#   - the `xargs -0 -r grep -lP` stage: its exit CODE alone is not a reliable failure signal, because
#     xargs batches its argv (splitting into multiple grep invocations if the file list ever exceeds
#     one command line) and folds every per-invocation exit status 1-125 — which includes grep's
#     ordinary "no match in this batch" — into its own exit 123. Treating exit 123 as failure would
#     false-positive the instant the file list needs a second batch. Instead these functions capture
#     grep's stderr (never routed to /dev/null, unlike the original call sites): "no match" writes
#     nothing to stderr, so an empty stderr always means "ran fine, maybe found nothing" regardless of
#     the numeric exit code xargs folds together; a REAL grep error (bad regex, an unreadable file
#     surviving `git ls-files`) writes a message, and that's the producer failure worth aborting on.
#
# Bash gotcha both functions below must not reintroduce: `local -a stat=("${PIPESTATUS[@]}")` MUST
# stay one statement immediately after the pipeline. Splitting it into `local -a stat` followed by a
# separate `stat=("${PIPESTATUS[@]}")` line silently breaks the capture -- the bare `local -a stat`
# declaration is itself a simple command, so running it resets $PIPESTATUS to ITS OWN one-element
# exit status before the assignment line ever reads it, and the real pipeline's statuses are lost
# (this shipped broken once during this file's own development and was caught by a manual test, not
# by any guard -- there was nothing to grep for).

# Internal: NUL-delimited stdin -> NUL-delimited stdout, dropping paths that no longer exist on disk.
# `git ls-files` reports the INDEX, so a tracked file deleted in the working tree but not yet committed
# (a normal transient during a rename/split/delete-before-commit) is still listed — and `grep -lP` on it
# writes "No such file or directory" to stderr, which the stderr-means-a-real-error check below would
# otherwise treat as a producer failure and abort a legitimate commit. Filtering to on-disk paths keeps
# a genuinely unreadable-but-present file (permissions, etc.) still erroring loud.
_existing_paths_only() {
  local f
  while IFS= read -r -d '' f; do
    [ -e "$f" ] && printf '%s\0' "$f"
  done
}

# Internal: shared tail for both public functions below — given the raw `grep -lP` stdout (a temp
# file of matched paths, not yet cleaned up), whether an earlier enumeration stage failed, and
# whatever grep wrote to stderr, either aborts loud or prints the standard-exclusion-filtered result
# and returns 0. Not part of the public API.
_tracked_grep_emit() {
  local who="$1" producer_failed="$2" grep_stderr="$3" dir="$4"

  if [ "$producer_failed" -ne 0 ]; then
    echo "$who: a file-enumeration stage failed -- aborting rather than risk an under-reported result." >&2
    rm -rf "$dir"
    return 1
  fi

  if [ -n "$grep_stderr" ]; then
    echo "$who: grep -lP reported an error (not just 'no matches'):" >&2
    printf '%s\n' "$grep_stderr" >&2
    rm -rf "$dir"
    return 1
  fi

  # ONE grep, not three chained (2026-07-29). The three exclusions are independent — a path is
  # dropped if ANY of them matches — so they are one alternation, and `(^|/)` still distributes over
  # each branch exactly as it did when each had its own grep. Three processes per call, on a helper
  # every isolation guard calls, is real money on a box where a spawn costs more than the whole scan.
  grep -vE '(^|/)(target|node_modules|\.claude)/' "$dir/out" || true
  rm -rf "$dir"
  return 0
}

# tracked_files_matching <perl-regex> <pathspec-glob>...
#
# Enumerates git-TRACKED files matching the given pathspec globs (git ls-files -z -- <globs>),
# greps them with `grep -lP <perl-regex>` (list matching filenames only, PCRE), applies the standard
# exclusions above, and prints one matching path per line on stdout. Prints nothing (and returns 0)
# when no tracked file matches the globs, or none of the matched files contain the pattern — an
# empty result is the ordinary, ubiquitous case, not a failure.
tracked_files_matching() {
  local pattern="$1"
  shift

  # ONE `mktemp -d` and one `rm -rf`, rather than two `mktemp`s and two `rm -f`s (2026-07-29). On
  # this box every external command costs roughly the same ~0.6s of fork/exec regardless of what it
  # does, so four of them per call was a measurable slice of every guard that scans. A private
  # directory (mktemp -d is mode 700) is also what makes the fixed inner names `out`/`err` safe:
  # deriving predictable sibling paths from a single `mktemp` FILE would not be.
  local dir
  dir="$(mktemp -d)"

  set +e
  git ls-files -z -- "$@" \
    | _existing_paths_only \
    | xargs -0 -r grep -lP "$pattern" -- \
    > "$dir/out" 2> "$dir/err"
  local -a stat=("${PIPESTATUS[@]}")
  set -e

  # `$(< file)` is bash's own no-fork file read; `$(cat "$err")` spawned a process to do the same
  # thing on every call (2026-07-29). Identical result — command substitution strips the trailing
  # newline in both spellings, so the `[ -n "$grep_stderr" ]` test downstream is unchanged.
  local grep_stderr
  grep_stderr="$(< "$dir/err")"

  local producer_failed=0
  [ "${stat[0]}" -ne 0 ] && producer_failed=1

  _tracked_grep_emit "tracked_files_matching" "$producer_failed" "$grep_stderr" "$dir"
}

# tracked_and_untracked_files_matching <perl-regex> <pathspec-glob>...
#
# Same contract as tracked_files_matching, but the enumeration ALSO includes untracked-but-not-
# git-ignored files (git ls-files --others --exclude-standard), matching check-english-source.sh's
# and check-max-file-lines.sh's "OSS-facing means will ship" scope: a fresh file must be caught
# before its first `git add`, not just from the moment it's tracked (see check-english-source.sh's
# header comment for the 2026-07-14 incident that motivated this).
tracked_and_untracked_files_matching() {
  local pattern="$1"
  shift

  # `mktemp -d` once — see the sibling function above.
  local dir
  dir="$(mktemp -d)"

  # ONE `git ls-files`, not two (2026-07-29). `--cached --others --exclude-standard` IS the union
  # this used to build by running the tracked half and the untracked-but-not-ignored half separately
  # and merging them: `--others` means "not in the index", so the two halves are disjoint by
  # definition and no entry can be produced twice. Verified on this repo the day it changed — 1264
  # paths from either spelling, identical set AND identical order after `sort -z -u`. `git ls-files`
  # measured ~1.2s per invocation here, which is why halving the count is worth a comment.
  #
  # `sort -z -u` is kept even though one invocation cannot emit a duplicate: it is what makes the
  # order independent of git's own listing order, and every caller that prints a result depends on
  # that stability.
  set +e
  git ls-files -z --cached --others --exclude-standard -- "$@" \
    | sort -z -u \
    | _existing_paths_only \
    | xargs -0 -r grep -lP "$pattern" -- \
    > "$dir/out" 2> "$dir/err"
  local -a stat=("${PIPESTATUS[@]}")
  set -e

  # `$(< file)` rather than `$(cat "$err")` — see the sibling function above.
  local grep_stderr
  grep_stderr="$(< "$dir/err")"

  # stat[0] = `git ls-files` (previously the `{ ls-files && ls-files; }` group; one command now, so
  # its status is read directly). stat[1] = `sort -z -u`.
  local producer_failed=0
  [ "${stat[0]}" -ne 0 ] && producer_failed=1
  [ "${stat[1]}" -ne 0 ] && producer_failed=1

  _tracked_grep_emit "tracked_and_untracked_files_matching" "$producer_failed" "$grep_stderr" "$dir"
}
