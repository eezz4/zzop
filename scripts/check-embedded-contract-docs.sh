#!/usr/bin/env bash
# Binds the documents COMPILED INTO the binary to the release rule that governs them.
#
# `crates/summary/src/contracts.rs` bakes several docs with `include_str!` and serves them as the
# `zzop contract` / `zzop://contract/*` resources. A reader on a prebuilt binary therefore sees the
# bytes of the release they installed, never the repository's — which makes "only docs changed, no
# release needed" FALSE for exactly this set and true for every other file under docs/.
#
# That fact used to live in a backlog row, i.e. in somebody's memory. This guard moves it into the
# tree: VERSIONING.md carries the list, and the two must agree in BOTH directions.
#
#   baked but unlisted   -> a doc whose changes silently need a release, with nothing saying so
#   listed but unbaked   -> VERSIONING.md claims a release requirement that does not exist, which
#                           costs a release nobody needed and teaches the reader to distrust the list
#
# Deliberately NOT a "did these change since the last tag" check: that is true on every working-branch
# commit between releases and would either cry wolf or be muted, and a muted guard is the shape this
# repo keeps paying for. The invariant here holds at every commit.
set -euo pipefail

SELF="check-embedded-contract-docs"
SOURCE="crates/summary/src/contracts.rs"
LISTING="VERSIONING.md"
MARKER="EMBEDDED-CONTRACT-DOCS"

abort() {
  echo "$SELF: $1" >&2
  exit 1
}

[ -f "$SOURCE" ] || abort "$SOURCE is missing — the scan subject moved and this guard would pass vacuously"
[ -f "$LISTING" ] || abort "$LISTING is missing"

# --- what the binary actually bakes -------------------------------------------------------------
# `include_str!("../../../docs/x.md")` -> `docs/x.md`. The `../../../` prefix is the crate-relative
# hop out to the repo root; stripping it is what makes the two sides comparable.
baked=$(grep -o 'include_str!("[^"]*")' "$SOURCE" \
  | sed 's/include_str!("//; s/")$//; s|^\(\.\./\)*||' \
  | sort -u)

[ -n "$baked" ] || abort "found zero include_str! docs in $SOURCE — the extraction broke, and an empty
subject set would make this whole check vacuously green (see the repo's guard-coverage rule)"

# --- what VERSIONING.md claims ------------------------------------------------------------------
grep -q "$MARKER" "$LISTING" || abort "$LISTING carries no $MARKER block — the list this guard checks
against is gone. Restore it, or delete this guard along with the section it protects."

# The list is the bullet run that follows the marker comment: `- \`path\`` lines, stopping at the first
# line that is neither a bullet nor blank.
# The marker comment spans several lines, so collection starts only after it CLOSES (`-->`); then it
# takes the bullet run and stops at the first line that is neither a bullet nor blank. Keying off the
# marker line alone stopped on the comment's own continuation lines — found by running it.
listed=$(awk -v marker="$MARKER" '
  index($0, marker) { in_comment = 1 }
  in_comment { if (index($0, "-->")) { in_comment = 0; collecting = 1 } ; next }
  collecting && /^- `/ { gsub(/^- `|`$/, ""); print; next }
  collecting && /^[[:space:]]*$/ { next }
  collecting { exit }
' "$LISTING" | sort -u)

[ -n "$listed" ] || abort "the $MARKER block in $LISTING lists no documents"

fail=0
unlisted=$(comm -23 <(echo "$baked") <(echo "$listed") || true)
unbaked=$(comm -13 <(echo "$baked") <(echo "$listed") || true)

if [ -n "$unlisted" ]; then
  fail=1
  echo "$SELF: document(s) baked into the binary but not listed in $LISTING:"
  echo "$unlisted" | sed 's/^/  /'
  echo
  echo "  Each of these reaches a user only through a RELEASE. Add it to the $MARKER list so"
  echo "  \"docs-only change, no release needed\" cannot be said about it."
fi

if [ -n "$unbaked" ]; then
  fail=1
  echo "$SELF: document(s) listed in $LISTING but no longer baked into the binary:"
  echo "$unbaked" | sed 's/^/  /'
  echo
  echo "  The list would make a reader ship a release nobody needed. Remove the entr(y|ies)."
fi

[ "$fail" -eq 0 ] || exit 1

count=$(echo "$baked" | wc -l | tr -d ' ')
echo "$SELF: OK ($count embedded contract document(s); $LISTING and $SOURCE agree both directions)"
