#!/usr/bin/env bash
# git spawn isolation guard — the whole workspace spawns `git` from exactly ONE non-test place, and
# the process census sits at that place.
#
# What this defends. `crates/engine/tests/git_spawn_census.rs` asserts "N trees sharing one
# repository collect git history once". That assertion is only total if every git process the engine
# can start passes the counter in `crates/git/src/process.rs::spawn_git` — a second `Command::new`
# anywhere in shipping code would be invisible to `zzop_git::spawn_log()`, and the census would read
# green while the regression it exists to catch walked past it. `crates/git/src/lib.rs`'s module doc
# has always CLAIMED "exactly one `std::process::Command` entry point"; before this guard nothing
# checked it, which is the repo's own "prose describing a check is not the check" class.
#
# Two checks:
#  1. `Command::new("git")` appears in exactly one tracked non-test `.rs` file: crates/git/src/process.rs.
#     Test code is exempt because a test's own `git init`/`git commit` is fixture setup, not analysis
#     — and deliberately NOT counted by the census (this is also why shimming `git` on PATH was
#     rejected as the measuring technique: it would have counted the harness's own calls).
#     `build.rs` is likewise exempt: a build script runs before, and outside, any analysis.
#  2. Inside that file, the spawn is preceded by the census call, so no path reaches the process
#     without being recorded.
#
# Scope: git-TRACKED files only (git ls-files), same reason as the swc/syn/ruff/tree-sitter isolation
# guards this is modeled on — untracked local corpora are not ours to police, and anything that can
# ship is tracked.
#
# No deps beyond git + grep -P (PCRE). Exit 1 on any violation.
set -euo pipefail
cd "$(dirname "$0")/.."
. ./scripts/lib/tracked-grep.sh

violations=0
SPAWN_PATTERN='Command::new\("git"\)'
OWNER='crates/git/src/process.rs'

# ## Subject-set floor
# Both checks read from a pathspec, and a pathspec that stops matching returns the same empty string
# a genuinely clean tree returns — the class this repo has paid for repeatedly. A zero here means the
# guard proved nothing, so it is a failure, never a pass.
RS_GLOBS=('*.rs')
rs_scanned="$(git ls-files -- "${RS_GLOBS[@]}" | grep -c . || true)"
if [ "$rs_scanned" -eq 0 ]; then
  echo "git spawn isolation guard: FAILED -- enumerated 0 .rs files. That pathspec matched nothing,"
  echo "so this run proved nothing about where git is spawned. An empty subject set is a broken"
  echo "guard, never a clean tree."
  exit 1
fi

echo "git spawn isolation guard: checking non-test git spawn sites..."
# Test code is identified by PATH, not by content: `tests/` directories, `tests.rs`, and `*_tests.rs`
# are this repo's only three spellings for it (the same three the 300-line file cap exempts).
spawn_files=$(tracked_files_matching "$SPAWN_PATTERN" "${RS_GLOBS[@]}" \
  | grep -v '/tests/' \
  | grep -v '/tests\.rs$' \
  | grep -v '_tests\.rs$' \
  | grep -v '/test_support\.rs$' \
  | grep -v '/build\.rs$' || true)

unexpected=$(grep -v -x "$OWNER" <<< "$spawn_files" || true)
if [ -n "$unexpected" ]; then
  echo "git spawn isolation guard: git spawned outside $OWNER:"
  while IFS= read -r f; do
    grep -nP "$SPAWN_PATTERN" "$f" | sed "s|^|  ${f#./}:|"
  done <<< "$unexpected"
  violations=1
fi

# The owner must actually still be the owner. If the spawn moved out of process.rs entirely, the
# check above would pass on an empty list — green while reading nothing, again.
if ! grep -qx "$OWNER" <<< "$spawn_files"; then
  echo "git spawn isolation guard: FAILED -- $OWNER no longer contains a git spawn."
  echo "Either it moved (update this guard AND crates/git/src/lib.rs's module doc together) or the"
  echo "pattern stopped matching. Until then the process census in crates/engine/tests/"
  echo "git_spawn_census.rs is measuring an unknown fraction of the real spawns."
  violations=1
fi

echo "git spawn isolation guard: checking the census sits ahead of the spawn..."
spawn_count=$(grep -cP "$SPAWN_PATTERN" "$OWNER" || true)
record_count=$(grep -cP '^\s*record_spawn\(repo\);' "$OWNER" || true)
if [ "$spawn_count" -ne 1 ] || [ "$record_count" -ne 1 ]; then
  echo "git spawn isolation guard: FAILED -- $OWNER has $spawn_count git spawn(s) and"
  echo "$record_count census call(s); expected exactly 1 of each. With more than one spawn the single"
  echo "census call cannot cover them all."
  violations=1
else
  spawn_line=$(grep -nP "$SPAWN_PATTERN" "$OWNER" | cut -d: -f1)
  record_line=$(grep -nP '^\s*record_spawn\(repo\);' "$OWNER" | cut -d: -f1)
  if [ "$record_line" -ge "$spawn_line" ]; then
    echo "git spawn isolation guard: FAILED -- the census call is at line $record_line but the spawn"
    echo "is at line $spawn_line. A spawn that runs before it is recorded is a spawn the census"
    echo "cannot see."
    violations=1
  fi
fi

if [ "$violations" -ne 0 ]; then
  echo
  echo "One counted door, or the git-process census proves nothing. See crates/engine/tests/"
  echo "git_spawn_census.rs for what the census asserts and why it is an equality rather than a timing."
  exit 1
fi

echo "git spawn isolation guard: clean ($rs_scanned .rs files scanned; sole non-test spawn is $OWNER, census ahead of it)."
