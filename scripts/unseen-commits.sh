#!/usr/bin/env bash
# unseen-commits.sh — print how many commits on this branch no machine has ever scored.
#
# ## The defect this exists for (2026-09-06, external review lane C)
#
# `.github/workflows/ci.yml` triggers on `push: branches: [main]` and `pull_request`. The working branch
# is named `never-push-just-commit` and, by user instruction, never reaches the remote. So CI is not
# "occasionally behind" on this work — it has never run on any of it. Measured at the time: the last CI
# run was 2026-08-31 on `main`, and `git rev-list --count origin/main..HEAD` was 45.
#
# Those 45 commits had the 43 pre-commit guards run against them and nothing else: no `cargo test`, no
# clippy, no site render check. The parser property tests live in `cargo test`, which is how a
# stack-overflow abort on a 1 KB file (fixed in `bb17c0d7`) survived to be found by an outside reviewer
# instead of by the machine that exists to find it.
#
# ## Why this prints a number instead of refusing
#
# Running the full local mirror (`scripts/ci-local.sh`) on every commit is not affordable: `cargo test
# --workspace` alone is 282s warm (measured 2026-09-06), and this repo already carries a user complaint
# about gate cost. A per-commit refusal would be skipped by habit within a day, and a skipped gate
# reports the same green as a passing one.
#
# So the lever is disclosure, not enforcement: the count is printed at every commit, it only ever grows,
# and it is the exact quantity that was invisible. 🔴 **There is deliberately no threshold.** A number
# like "warn past 20" would be a policy nobody measured — the audit that removed this repo's unbacked
# numeric thresholds is the reason a new one is not being invented here.
#
# The floor case is real and is reported as such: with no `origin/main` (a fresh clone with no remote,
# or a renamed default branch) the honest answer is "cannot tell", never 0. A silent 0 would read as
# "everything has been seen", which is the opposite of the truth this script exists to carry.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

if ! git rev-parse --verify --quiet origin/main > /dev/null; then
  echo "pre-commit: no origin/main in this clone -- cannot say how many commits CI has never seen."
  echo "  (That is not zero. Fetch the remote, or read scripts/ci-local.sh as the local mirror.)"
  exit 0
fi

# `set -e` would kill this script ON THE ASSIGNMENT if rev-list ever exits non-zero (a shallow clone,
# an unrelated-histories state), and it would die BEFORE the line that explains what happened — the
# mute-failure shape check-shell-mute-floor.sh exists for, which is how it caught this script's first
# draft. Tell "no answer" apart from "the count is 0" instead of conflating them.
set +e
n="$(git rev-list --count origin/main..HEAD 2> /dev/null)"
rc=$?
set -e
if [ "$rc" -ne 0 ] || [ -z "$n" ]; then
  echo "pre-commit: could not count commits against origin/main (rev-list exit $rc)."
  echo "  That is not zero -- nothing here has been scored by CI and this script could not say how much."
  exit 0
fi
[ "$n" -eq 0 ] && exit 0


# What that number MEANS depends on where origin/main is, and it is usually a release (2026-09-07,
# review ledger V76). This branch is never pushed, so origin/main only moves when a release lands --
# which makes "$n commits CI has never scored" and "$n commits since v0.34.0" the same set with very
# different urgency. The first reads as a CI gap to close; the second is the release delta, and it is
# the one a reader can act on. Read the tag from the REMOTE: a local `git tag` answers when this clone
# last fetched, a trap CLAUDE.md has had to name twice.
set +e
tag="$(git ls-remote --tags origin 2> /dev/null \
  | awk -v head="$(git rev-parse origin/main 2> /dev/null)" '$1 == head { print $2 }' \
  | sed 's|refs/tags/||; s|\^{}||' | sort -V | tail -1)"
set -e
if [ -n "${tag:-}" ]; then
  echo "pre-commit: $n commit(s) since ${tag} -- origin/main IS that release, so this is the whole"
  echo "  unreleased delta. Nothing in it has been scored by CI."
else
  echo "pre-commit: $n commit(s) on this branch that CI has never scored (origin/main..HEAD)."
fi
# Counted, never typed. The first draft of this line said "the 43 guards above" and was stale within a
# day of being written -- by the very next guard this branch added. A guard-fleet size is exactly the
# kind of number this repo refuses to write into prose, and printing one from a script is no different.
set +e
g="$(ls scripts/check-*.sh 2> /dev/null | awk 'END { print NR }')"
set -e
[ -n "$g" ] || g="?"
echo "  CI runs on main/PR only and this branch is never pushed, so the $g guards above are the ONLY"
echo "  machine that has read them -- no cargo test, no clippy, no site render check."
echo "  Full local mirror, when you want it scored:  bash scripts/ci-local.sh"
