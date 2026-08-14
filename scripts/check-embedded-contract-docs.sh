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
#
# `|| true` is load-bearing: without it a grep that matches nothing exits 1, `set -e -o pipefail` kills
# the script on the ASSIGNMENT, and the abort below — the one that exists to refuse a vacuously green
# run — never speaks. The guard still failed closed, but silently and with no diagnosis, which is the
# same "muted guard" shape the header rejects. Measured 2026-08-14 by planting a $SOURCE with no
# include_str! in it: exit 1, zero output.
baked=$(grep -o 'include_str!("[^"]*")' "$SOURCE" \
  | sed 's/include_str!("//; s/")$//; s|^\(\.\./\)*||' \
  | sort -u || true)

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

# --- the THIRD baking path: bytes OWNED BY ANOTHER CRATE (2026-08-14) ------------------------------
# Some rows in $SOURCE take their content from a `zzop_*::CONST` rather than from an `include_str!` of
# their own: the bytes are baked by the crate that already embeds them for its own use and are merely
# RE-served here, deliberately, so there is one embed and one truth. A grep for `include_str!` inside
# $SOURCE cannot see those, and `crates/config/config-surface.json` was therefore shipped in every
# binary while absent from $LISTING for the whole life of its row — the third instance of "the needle
# is narrower than the class the header names" (working-agreements §5.5), one hop further out than the
# pack lane above.
#
# Derived, never listed: the constants are read out of $SOURCE, the crate is found by the package name
# its manifest declares, and the definition is read inside that crate. A constant that cannot be
# resolved ABORTS rather than contributing nothing — the defect being defended against is a served
# document that neither half of the comparison can see, so "could not follow it" has to be loud.
#
# A constant whose definition is NOT an `include_str!` (a raw string literal — the `config-template`
# row) has no file to list and is skipped. That exclusion is DERIVED, not assumed: the definition is
# found and read before the decision, which is the whole difference between an exclusion and a blind
# spot. `disclosure-classes` is excluded the same way one shape over — it is rendered from Rust at
# build time and its `content:` is a function call, not a path.

# "crates/config/src" + "../config-surface.json" -> "crates/config/config-surface.json"
resolve_relative() {
  awk -v joined="$1/$2" 'BEGIN {
    n = split(joined, seg, "/")
    top = 0
    for (i = 1; i <= n; i++) {
      if (seg[i] == "" || seg[i] == ".") continue
      if (seg[i] == "..") { if (top > 0) top--; continue }
      st[++top] = seg[i]
    }
    out = ""
    for (i = 1; i <= top; i++) out = out (i > 1 ? "/" : "") st[i]
    print out
  }'
}

# Shape sweep FIRST. Every `content:` value in $SOURCE must fall into a shape this guard knows how to
# judge; an unrecognised fourth shape is a document being served that no lane below is looking at. This
# sweep is also what makes an EMPTY cross-crate lane safe to report clean — inlining these rows back to
# `include_str!` is a legitimate future, and the sweep proves the rows did not merely become invisible.
content_rows=0
while IFS= read -r line; do
  content_rows=$((content_rows + 1))
  case "$line" in
    *include_str!\(*) ;;                                   # lane 1, handled above
    *"content: "*::*) ;;                                   # lane 3, handled below
    *"content: "*"()"*) ;;                                 # rendered from Rust — no file to list
    *) abort "unrecognised \`content:\` shape in $SOURCE — this guard can neither follow it to a file
nor deliberately exclude it, so the document it serves is invisible to BOTH halves of this check:
  $line" ;;
  esac
done < <(grep -E '^[[:space:]]*content: ' "$SOURCE" || true)
[ "$content_rows" -gt 0 ] || abort "no \`content:\` rows found in $SOURCE — the shape sweep matched
nothing, so it would vouch for every row while reading none. The row syntax changed; re-point it."

# `|| true` on this and on the definition lookup below is load-bearing under `set -e -o pipefail`, not
# hygiene: a grep that finds nothing exits 1, which killed the whole script with NO output and exit 1 —
# a guard that fails closed but silently, i.e. indistinguishable from a real finding. Found by running
# the invalidation drill for the abort paths below, which is the only reason it is not still here.
cross_consts=$(grep -oE '^[[:space:]]*content: [a-z][a-z0-9_]*(::[A-Za-z0-9_]+)+' "$SOURCE" \
  | sed 's/^[[:space:]]*content: //' \
  | sort -u || true)

for ref in $cross_consts; do
  crate_ident="${ref%%::*}"
  const_name="${ref##*::}"
  pkg="${crate_ident//_/-}"

  manifest=""
  while IFS= read -r m; do
    if grep -qE "^name = \"$pkg\"[[:space:]]*$" "$m"; then manifest="$m"; break; fi
  done < <(git ls-files '*Cargo.toml')
  [ -n "$manifest" ] || abort "$SOURCE serves \`$ref\`, but no manifest declares the package \`$pkg\`.
The constant moved crates — re-point this derivation in the same commit as the move."

  crate_dir=$(dirname "$manifest")
  def=$(grep -rnE --include='*.rs' "const[[:space:]]+$const_name[[:space:]]*:" "$crate_dir/src" \
    | grep '=' | head -n1 || true)
  [ -n "$def" ] || abort "$SOURCE serves \`$ref\`, but no definition of \`$const_name\` was found under
$crate_dir/src. Either it was renamed or it now lives elsewhere; this guard cannot judge what it
cannot read."

  def_file=${def%%:*}
  def_rest=${def#*:}
  def_line=${def_rest%%:*}
  def_head=$(sed -n "${def_line}p" "$def_file")

  case "$def_head" in
    *include_str!\(*) ;;
    *"= r"*|*'= "'*) continue ;;   # a literal in the source — no file exists to list
    *)
      # The `=` can sit on a following line; widen once before giving up.
      def_head=$(sed -n "${def_line},$((def_line + 2))p" "$def_file")
      case "$def_head" in
        *include_str!\(*) ;;
        *) abort "cannot classify the definition of \`$const_name\` at $def_file:$def_line — it is
neither an include_str! nor a literal this guard recognises. Classify it here rather than letting it
fall through: an unclassified served constant is a document nothing checks." ;;
      esac
      ;;
  esac

  inc=$(printf '%s' "$def_head" | grep -o 'include_str!("[^"]*")' | head -n1 \
    | sed 's/include_str!("//; s/")$//' || true)
  [ -n "$inc" ] || abort "\`$const_name\` at $def_file:$def_line was classified as an include_str! but
no path could be extracted from it — the extraction broke, and a lane that contributes nothing while
reporting success is the shape this file exists to prevent."
  resolved=$(resolve_relative "$(dirname "$def_file")" "$inc")
  [ -f "$resolved" ] || abort "\`$const_name\` bakes $inc (resolved to $resolved), which does not
exist. The path resolution here is wrong, or the file moved without its include_str! following."
  baked=$(printf '%s\n%s\n' "$baked" "$resolved" | sort -u)
done

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
