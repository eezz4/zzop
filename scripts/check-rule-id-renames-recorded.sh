#!/usr/bin/env bash
# check-rule-id-renames-recorded.sh — a rule id that changed since the last release must be named in
# the release notes.
#
# ## The defect this exists for (2026-09-07, external review round 11, review ledger V86)
#
# `VERSIONING.md`'s compatibility surface promises that rule ids are covered and that "every rename
# that reached a release lands as a row". Nothing checked it. The review looked for the machine and
# found none: no script compares the id set at HEAD against the id set at the last tag, so a rename
# that edited `docs/rules/catalog.md` and the labelled corpus together passed the whole guard fleet
# green with no CHANGELOG row and no VERSIONING row. Every OTHER half of that promise is machine-held
# (`catalog_sync.rs` pins the catalog against the packs, `check-prose-rule-ids.sh` pins prose against
# the catalog) — the half that says "and it was written down" was the gap.
#
# That gap is load-bearing for 1.0. `one-point-oh.md`'s expiry list orders E2 (reclaim the freedom to
# rename) before E1 (start carrying compatibility), and the reason is that a rename after 1.0 costs a
# MAJOR bump. Reclaiming a freedom while the machine that records its use does not exist means the
# first post-1.0 rename ships silently, which is exactly the state the promise was written against.
#
# ## What it compares, and why the subject is the catalog
#
# The catalog is the id SSOT for this purpose: `catalog_sync.rs` already fails if it disagrees with the
# packs, so a rename must reach it, and it is one file that exists at both revisions. Comparing pack
# JSON directly would mean parsing 8 files at two revisions to learn the same thing.
#
# The remote tag is the baseline, read with `git ls-remote` — a local `git tag` answers when this clone
# last fetched, a trap CLAUDE.md names twice and this repo has been burnt by twice.
#
# ## Why "named in the notes" and not a stricter shape
#
# The guard asserts the OLD spelling appears in `CHANGELOG.md` or `VERSIONING.md`. It deliberately does
# not demand a table row or a particular wording: the migration row's shape is a human judgement that
# `VERSIONING.md` already documents, and a guard that dictated it would fire on correct notes written
# differently. What cannot be a judgement call is whether the retired spelling appears at all — a
# reader searching for the id they configured against must find it.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

CATALOG=docs/rules/catalog.md
[ -f "$CATALOG" ] || { echo "check-rule-id-renames-recorded: missing $CATALOG -- re-anchor this guard." >&2; exit 1; }

# The release this branch is measured against: the newest tag ON THE REMOTE.
#
# 🔴 This used to require that tag to sit exactly at `origin/main`, and that made the guard a no-op
# in CI for its entire life (2026-09-13, review ledger V186). Two things compounded:
#
#   1. `actions/checkout@v4` does a shallow fetch of the checked-out ref and does NOT create an
#      `origin/main` remote-tracking ref. On any PR run, `git rev-parse origin/main` fails.
#   2. `git rev-parse` on an unknown ref does not print nothing -- it ECHOES THE REF NAME BACK and
#      exits non-zero. With `set +e` swallowing the status, `head_sha` became the literal string
#      "origin/main", which matches no sha, so the tag lookup found nothing and the SKIP branch ran.
#
# The result was a guard that announced a skip nobody was reading and exited 0, forever. It passed
# locally only because a developer clone happens to have that ref. Measured: with `head_sha` set to
# a missing ref, the tag resolves empty and this exits 0.
#
# So the baseline no longer depends on a LOCAL ref at all. `git ls-remote` asks the remote directly,
# which is what CI can actually answer, and the newest release tag is the right baseline regardless of
# which commit currently sits at main's tip.
set +e
tag="$(git ls-remote --tags origin 2> /dev/null \
  | sed 's|.*refs/tags/||; s|\^{}||' \
  | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' | sort -V | tail -1)"
set -e
if [ -z "${tag:-}" ]; then
  echo "check-rule-id-renames-recorded: SKIPPED -- the remote advertised no release tag."
  echo "  Not a failure: an offline clone has no baseline to diff against. This is now the ONLY way"
  echo "  to reach this branch -- it used to also fire whenever a local origin/main ref was missing,"
  echo "  which is every CI run (review ledger V186)."
  exit 0
fi

ids_at() {
  # `| ` + backticked id, in the DSL-pack and native-analysis tables. The native table ends at the next
  # `###`, a convention `docs/rules/catalog.md` states in its own text -- counting past it double-counts
  # `circular`, which appears again as a recommendation id.
  { if [ "$1" = "WORKTREE" ]; then cat "$CATALOG"; else git show "$1:$CATALOG" 2> /dev/null; fi; } | awk '
    /^## /   { sec = $0 }
    /^### /  { if (sec ~ /Native analyses/) sec = "closed" }
    /^\| `/  { if (sec ~ /DSL packs/ || sec ~ /Native analyses/) { gsub(/^\| `/, ""); gsub(/`.*$/, ""); print } }
  ' | sort -u
}

old_ids="$(ids_at "$tag")"
new_ids="$(ids_at WORKTREE)"
if [ -z "$old_ids" ] || [ -z "$new_ids" ]; then
  echo "check-rule-id-renames-recorded: extracted ZERO ids at $tag or in the working tree." >&2
  echo "  An empty subject is a broken guard, never a catalog with no rules. The extraction shape is" >&2
  echo "  the table row '| \`id\` |' under the DSL-packs and native-analyses headings." >&2
  exit 1
fi

# `comm` exits non-zero on some platforms when a side is empty, and "no id was retired" is the
# COMMON case here — under `set -e -o pipefail` that would kill the script on the assignment, before
# the OK line written to report exactly that state could print. Fail-closed but mute is worse than
# either outcome: exit 1 with zero bytes reads on screen like a real finding.
set +e
retired="$(comm -23 <(printf '%s
' "$old_ids") <(printf '%s
' "$new_ids"))"
rc=$?
set -e
if [ "$rc" -ne 0 ] && [ -n "$retired" ]; then
  echo "check-rule-id-renames-recorded: comm failed (exit $rc) while diffing the id sets." >&2
  echo "  That is not 'nothing retired' -- the comparison itself did not run." >&2
  exit 1
fi
[ -z "$retired" ] && {
  n="$(printf '%s\n' "$new_ids" | awk 'END { print NR }')"
  echo "check-rule-id-renames-recorded: OK ($n ids, none retired since $tag)."
  exit 0
}

notes=""
for f in CHANGELOG.md VERSIONING.md; do
  [ -f "$f" ] && notes="$notes $f"
done
[ -n "$notes" ] || { echo "check-rule-id-renames-recorded: no CHANGELOG.md or VERSIONING.md to search." >&2; exit 1; }

unrecorded=""
for id in $retired; do
  # shellcheck disable=SC2086
  if ! grep -q -F -- "$id" $notes; then
    unrecorded="$unrecorded $id"
  fi
done

if [ -n "$unrecorded" ]; then
  echo "check-rule-id-renames-recorded: rule id(s) retired since $tag that the release notes never name:" >&2
  echo " $unrecorded" >&2
  echo "  A user configured \`disabledRules\`/\`severityOverrides\` against these spellings and their" >&2
  echo "  config now silently does nothing. Name the old spelling in CHANGELOG.md (and in" >&2
  echo "  VERSIONING.md's rename table if it reached a release), so a reader searching for the id they" >&2
  echo "  wrote can find out what it became." >&2
  exit 1
fi

n_ret="$(printf '%s
' "$retired" | awk 'END { print NR }')"
echo "check-rule-id-renames-recorded: OK ($n_ret retired id(s) since $tag, each named in the release notes)."
