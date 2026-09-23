#!/usr/bin/env bash
# subtraction-trend.sh — the numbers review ledger row V125 is about, counted the way that survives
# being re-run.
#
# ## Why this is a script and not a `git log --shortstat` line in a document
# The row carried three points taken with `git log origin/main..HEAD --shortstat` piped through awk.
# 📏 2026-09-15: that route was re-run for a fourth point and printed **167 commits** for a range
# `git rev-list --count` puts at **185** — the summary line wraps, and a parse that reads it drops
# records without saying so. A `grep -c 'files changed'` over the same output gives 187, because a
# commit MESSAGE can contain the phrase. Three spellings, three answers, none of them flagged.
#
# So: commits come from `rev-list` (authoritative, one line per commit), and lines come from
# `--numstat` with a NUL record separator — one record per commit, so a commit with no file change
# cannot silently vanish and a wrapped summary cannot double-count. The script prints the commit count
# it used, and the caller can check it against `git rev-list --count <range>` in one line.
#
# ## What the number is NOT
# It is not the doctrine. `CLAUDE.md`'s subtraction bias is "ask whether something can be removed
# before adding", which is a question about PROCESS; this is output volume in a repo whose main work
# product is guards, tests and disclosures — all of which add lines while removing defects. Reading the
# ratio as a verdict is the proxy-metric failure the design cheatsheet names twice (§5.38, §5.42).
# It is a TREND to look at, and V125 records the four points and the judgement made from them.
#
# Usage:  bash scripts/measure/subtraction-trend.sh [<git range>]     (default: origin/main..HEAD)
set -euo pipefail
cd /Users/eezz/Documents/project/zzop
RANGE="${1:-origin/main..HEAD}"
commits=$(git rev-list --count "$RANGE")
git log "$RANGE" --numstat --format='%x00' | awk -v commits="$commits" '
  /^\x00/ { if (seen) { tot_a += a; tot_d += d; if (d > a) net++ } ; seen = 1; a = 0; d = 0; next }
  /^[0-9]+\t[0-9]+\t/ { a += $1; d += $2 }
  END { if (seen) { tot_a += a; tot_d += d; if (d > a) net++ }
        printf "  commits %d | +%d / -%d | ratio %.1f:1 | net-decrease %d (%.1f%%)\n",
               commits, tot_a, tot_d, tot_a/tot_d, net, net*100/commits }'
