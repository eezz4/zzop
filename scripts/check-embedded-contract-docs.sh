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

# --- the SECOND baking path, which no `include_str!` in $SOURCE can reveal (2026-08-12) ------------
# `examples/packs/*.json` is baked too, but through a GENERATED file: crates/config/build.rs emits one
# `include_str!` per pack into $OUT_DIR, and crates/summary/src/contracts.rs appends those rows to the
# table. Scanning $SOURCE alone therefore sees a table SMALLER than the one the binary serves — the
# exact "the guard covers less than it claims" shape working-agreements §5.5 is about, and it would
# have been introduced by the very commit that added the rows.
#
# The subject is DERIVED from git rather than listed here, so exporting the next pack cannot slip past:
# the pack lands, this set grows, and the run fails until VERSIONING.md names it. That is the intended
# cost — the listing below is a RELEASE contract ("this file needs a release to reach a reader"), and a
# new shipped document silently joining it is precisely what this guard exists to prevent.
example_packs=$(git ls-files 'examples/packs/*.json' | sort -u)
[ -n "$example_packs" ] || abort "git ls-files found zero examples/packs/*.json — either the exported
packs moved (re-point this derivation in the same commit as the move) or the enumeration broke. An
empty half of the subject set is a silently narrower guard, not a clean tree."
baked=$(printf '%s\n%s\n' "$baked" "$example_packs" | sort -u)

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
