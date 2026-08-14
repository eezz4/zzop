#!/usr/bin/env bash
# Guards that the two COMMITTED pages scripts/gen-site.mjs writes — site/index.html (English) and
# site/ko/index.html (Korean) — are still what that generator produces from site-src/.
#
# ## Why a guard and not a build step
# Those two files are generated AND committed, because GitHub Pages serves site/ as plain files. That
# pairing drifts by default and the drift is silent in the worst way available: the page keeps
# rendering, keeps passing every link/prose guard, and simply describes a site-src/ that no longer
# exists. Editing a sentence in site-src/content/*.mjs and forgetting `node scripts/gen-site.mjs`
# leaves the repo carrying a page that will never be deployed, and nothing else in the tree notices.
#
# site-src/ is not the only input, and the second one is the easier of the two to forget: gen-site.mjs
# SLICES the dep-graph data block and the graph viewer out of site/graph.html at build time rather than
# keeping a copy. So regenerating site/graph.html (scripts/site-graph-data.mjs, run from the
# `site graph is regenerated…` step in ci.yml) makes both editions stale too, and that path involves
# nobody editing a sentence at all. This guard reads the outputs, not the inputs, so it covers both
# without having to enumerate either.
# This repo has already paid for exactly this shape once: scripts/site-graph-data.mjs had zero callers
# anywhere in the tree while its output was committed on every release (closed 2026-07-29 by the
# `site graph is regenerated from the tree it claims to draw` step in ci.yml).
#
# ## Why it is not folded into that ci.yml step
# That step needs a `--release` build of `zzop` to produce its input, so it rides the one job that
# already pays for one and cannot be a pre-commit hook. This one needs `node` and nothing else — no
# cargo, no network, no npm install — so it belongs in the fleet, where it runs on the commit that
# introduces the drift instead of on the push afterwards.
#
# ## Why this writes into the working tree at all
# The sibling generator-check, check-rules-catalog-sync.sh, calls `node scripts/gen-site-rules.mjs
# --check`, which never writes. gen-site.mjs has no such mode: its non-preview target paths are fixed
# (`site/index.html`, `site/ko/index.html`) and it resolves them from its own file location, so it
# cannot be pointed at a scratch directory from outside. Copying the whole repo to a temp tree to give
# it somewhere else to write would cost more than the check.
#
# So this snapshots site/ first, regenerates in place, compares, and puts every byte back — through an
# EXIT trap, so the restore happens on the failure path and on Ctrl-C too, not only on the happy one.
# The restore is driven by the SNAPSHOT, not by the generator's own report of what it wrote: a run that
# died halfway through its second file must still be undone, and at that point there is no report.
#
# ## The subject is DERIVED, and the derivation has a floor
# Nothing here hand-lists the two output paths. The comparison is over all of site/, so a third edition
# added to gen-site.mjs tomorrow is covered the day it lands rather than the day someone remembers this
# file — the defect class this repo spent 2026-07-28 removing from sixteen guards. What IS read from the
# generator is its `OK  <path>  <size>` report, and only to assert that it actually wrote something: a
# comparison of a directory against itself is empty whether the generator produced two files or none, so
# without that floor a generator that silently no-ops reads exactly like a perfectly fresh tree.
# The report's shape is therefore load-bearing. If gen-site.mjs ever stops printing those lines this
# guard goes RED with the message below rather than quietly certifying an empty run.
#
# ## Line endings
# `.gitattributes` pins both outputs to `text eol=lf`, and that pin is this guard's precondition, not
# housekeeping. gen-site.mjs writes `\n`; with `core.autocrlf=true` (the Git-for-Windows default, set on
# at least one dev box here) an unpinned checkout would hand this guard a CRLF working-tree copy of a
# file whose committed blob is LF, and every byte comparison below would report drift on a tree that has
# none — a guard that cries wolf on a clean clone is a guard that gets deleted. The same reasoning, on
# the same axis, already sits at the top of .gitattributes for four other byte-compared artifacts.
#
# No deps beyond `node` + coreutils. Exit 1 on any drift, naming each file and the command that fixes it.
set -euo pipefail
cd "$(dirname "$0")/.."

SITE="site"
GENERATED_BY="node scripts/gen-site.mjs"

[ -f scripts/gen-site.mjs ] || { echo "check-site-generated: scripts/gen-site.mjs is missing" >&2; exit 1; }
[ -d "$SITE" ] || { echo "check-site-generated: $SITE/ is missing" >&2; exit 1; }

# node is REQUIRED, not optional — same stance and the same wording as check-rules-catalog-sync.sh and
# check-license-shipping.sh take for their own generators. Skipping the one check that can see this
# drift, on exactly the machine that cannot run it, is a check that is off.
if ! command -v node >/dev/null 2>&1; then
  echo "check-site-generated: \`node\` not found, so the committed site/ pages could not be compared" >&2
  echo "  against what scripts/gen-site.mjs produces. This guard does not skip a check it cannot run —" >&2
  echo "  a skipped check is an off check. (No cargo, no network and no npm install are needed here.)" >&2
  exit 1
fi

# Paths under <root>, relative and sorted. Used for the snapshot, the comparison and the restore, so
# the three can never disagree about what "the site" is.
list_files() { # <root>
  ( cd "$1" && find . -type f -print | sed 's|^\./||' | LC_ALL=C sort )
}

tmp="$(mktemp -d)"
snap="$tmp/$SITE"
cp -a "$SITE" "$tmp/"

# Put the working tree back exactly as it was found, whatever happens next. Idempotent and silent, and
# it never fails: a restore that trips `set -e` inside an EXIT trap would leave the tree half-written.
restore_site() {
  local rel
  if [ -d "$snap" ]; then
    # One `cp -a` for the whole subtree rather than a per-file compare-then-copy: on this repo's
    # Windows box a process spawn costs more than the copy, and `-a` carries the original mtimes back
    # with the bytes, so a file the generator rewrote identically does not come out of this looking
    # touched. A failure here is the one outcome that must NOT be swallowed — it means the tree is
    # half-written — so it keeps the snapshot and says where it is instead of cleaning up.
    if ! cp -a "$snap/." "$SITE/"; then
      echo "check-site-generated: WARNING -- could not restore $SITE/ from its snapshot." >&2
      echo "  The snapshot is deliberately LEFT at $snap so nothing is lost;" >&2
      echo "  \`cp -a \"$snap/.\" $SITE/\` puts it back by hand." >&2
      return 0
    fi
    # Anything the generator created that was not there before goes away again, and so does a directory
    # it had to create to hold it (site/ko/ on a tree that never had one).
    while IFS= read -r rel; do
      [ -n "$rel" ] || continue
      [ -f "$snap/$rel" ] || rm -f -- "$SITE/$rel"
    done <<< "$(list_files "$SITE")"
    find "$SITE" -mindepth 1 -type d -empty -delete 2>/dev/null || true
  fi
  rm -rf -- "$tmp"
  return 0
}
trap restore_site EXIT

before="$(list_files "$snap")"

rc=0
gen_out="$(node scripts/gen-site.mjs 2>&1)" || rc=$?
if [ "$rc" -ne 0 ]; then
  echo "check-site-generated: FAILED -- \`$GENERATED_BY\` exited $rc. The committed pages cannot be" >&2
  echo "  judged against a build that did not happen; its own output follows." >&2
  printf '%s\n' "$gen_out" | sed 's/^/  /' >&2
  exit 1
fi

# The floor: the generator has to have REPORTED writing something. See the header — an empty comparison
# is indistinguishable from a fresh tree, so this is what stops a no-op from reading as a green.
written="$(printf '%s\n' "$gen_out" | sed -n 's|^OK  \([^ ][^ ]*\)  .*$|\1|p')"
written_n=0
while IFS= read -r w; do
  [ -n "$w" ] && written_n=$((written_n + 1))
done <<< "$written"

if [ "$written_n" -eq 0 ]; then
  echo "check-site-generated: FAILED -- \`$GENERATED_BY\` reported writing ZERO files. It exited 0, so" >&2
  echo "  the comparison below would have compared site/ against an untouched copy of itself and called" >&2
  echo "  that agreement. Either the generator no longer writes anything, or it no longer prints its" >&2
  echo "  'OK  <path>  <size>' line per output and this guard can no longer tell. Its output was:" >&2
  printf '%s\n' "$gen_out" | sed 's/^/  /' >&2
  exit 1
fi

after="$(list_files "$SITE")"

drift=""
while IFS= read -r rel; do
  [ -n "$rel" ] || continue
  if [ ! -f "$snap/$rel" ]; then
    drift="${drift}  NOT COMMITTED  $SITE/$rel"$'\n'
  elif ! cmp -s "$snap/$rel" "$SITE/$rel"; then
    drift="${drift}  STALE          $SITE/$rel"$'\n'
  fi
done <<< "$after"
while IFS= read -r rel; do
  [ -n "$rel" ] || continue
  [ -f "$SITE/$rel" ] || drift="${drift}  NO LONGER BUILT  $SITE/$rel"$'\n'
done <<< "$before"

if [ -n "$drift" ]; then
  echo "check-site-generated: FAILED -- regenerating the site changed these files:" >&2
  printf '%s' "$drift" >&2
  echo >&2
  echo "  site/index.html and site/ko/index.html are GENERATED and COMMITTED. Changing an input without" >&2
  echo "  regenerating leaves this repo carrying a page that will never be deployed, and every other" >&2
  echo "  check stays green because the stale page is still valid HTML about an input that is gone." >&2
  echo "  The inputs are site-src/ (the sentences and the stylesheet) AND site/graph.html, whose data" >&2
  echo "  block and viewer script this generator slices in rather than copying." >&2
  echo >&2
  echo "  Fix:  $GENERATED_BY" >&2
  echo "  (then commit the regenerated pages together with the site-src/ change that caused them)." >&2
  echo "  The working tree has been left exactly as this guard found it — nothing above was written." >&2
  exit 1
fi

echo "check-site-generated: clean ($written_n generated page(s) match the committed bytes; $(printf '%s\n' "$after" | wc -l | tr -d ' ') file(s) in $SITE/ compared)."
