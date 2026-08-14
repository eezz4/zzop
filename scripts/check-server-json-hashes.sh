#!/usr/bin/env bash
# server.json / release-platform guard. TWO subjects, one root cause: the `publish-mcp-registry` job in
# .github/workflows/prebuild.yml stamps `.packages[$i]` BY POSITION, so every place that writes down
# "which platforms exist, in which order" has to be provably the same list.
#
# ## Subject 1 — the THREE-WAY platform equality (asserted in every mode)
#   A. `.github/workflows/prebuild.yml`'s `build` job matrix, `include[].platform` — THE AUTHORITY.
#      It is the only one of the three that decides what is actually compiled, packaged and uploaded.
#   B. the `platforms=(...)` mirror inside that file's `publish-mcp-registry` job — a declaration, not
#      an authority: the stamping step derives its real list by calling this script (see `--print-
#      matrix-platforms` below) and then asserts the mirror equals it.
#   C. `server.json`'s `packages[]`, each reduced to the platform token its own `identifier` download
#      URL names (.../zzop-mcp-<platform>.mcpb).
# Same values, same order, all three. A is derived by parsing the matrix; B and C are the copies being
# held to it. Nothing in this script hand-lists a platform name (working-agreements §5.5): the subject
# set is whatever the matrix says today, and a broken extraction ABORTS instead of certifying zero
# platforms.
#
# ### Why this is a COMMIT-time check — the half-release it prevents
# The A==B equality existed before 2026-08-01, but only INSIDE the `publish-mcp-registry` job. That job
# is `needs: [release, meta]`, i.e. it starts after the GitHub release has been created and its assets
# uploaded, and it runs alongside `publish`, which has by then pushed the npm packages. Both of those
# are irreversible; a red `publish-mcp-registry` at that point does not prevent a release, it reports one
# that already half-shipped, and the repair costs a burned version number. Measured and recorded in
# prebuild.yml's own step comment: add a sixth matrix entry, change nothing else, and every local gate —
# this guard included — stayed green, because every equality in the chain compared two COPIES to each
# other and none of them could see the matrix change. That is the same defect shape as the autotag that
# used to run before its own gate: an irreversible action upstream of the check that judges it.
# Asking the question here moves it in front of everything irreversible twice over — pre-commit, and
# ci.yml's `guards` job, which prebuild's `meta` (the autotagger), `release` and `publish` all reach
# through `needs: [gate]`. A sixth platform now fails before the tag exists.
#
# ### What subject 1 does NOT cover (stated so a green line is not read as more than it is)
#   * It does not prove a matrix entry BUILDS, that its asset was uploaded, or that a package
#     `identifier` URL resolves. It proves three written-down lists agree, nothing about the bytes.
#   * It does not read the OTHER platform inventories — `packages/cli/npm/*`, `packages/cli/package.json`
#     optionalDependencies, the shim's PLATFORM_PACKAGES map, the plugin bootstrap. Those are
#     check-deploy-facts-prose.sh's subject; a platform added to the matrix but not to them goes red
#     there, not here.
#   * Order is compared, so a REORDER of packages[] fails even though the resulting registry entry would
#     list the same bundles. That is deliberate: the stamping loop is positional, and a guard that
#     accepted a reorder would accept exactly the mis-stamp this file exists to make unreachable.
#
# ### Parsing route: the SHARED reader, awk anchored to the matrix BLOCK — and not node
# The extraction lives in scripts/lib/release-matrix.sh (moved down a layer 2026-08-08; this guard's
# walk IS that code, it just no longer lives here). It walks jobs: -> build: -> strategy: -> matrix: ->
# include: by INDENTATION and reads `platform:`/`os:` only inside that block, then cross-checks the
# number of list items (`- target: ...`) against the number of `platform:` keys it found. Consequences,
# each one a failure this shape converts from silent to loud: a renamed job/key aborts (no anchor), a
# matrix entry that forgets `platform:` aborts (items != keys), and a stray `platform:` line elsewhere
# in the file is not read at all.
# It is shared because the alternative was measured and rejected: check-asset-name-prose.sh and
# check-deploy-facts-prose.sh each carried a LOOSER `/^  build:/ ... /^    runs-on:/` awk copy of this
# same block, so a reshaped matrix had to be chased through three files and the one that got missed
# would keep answering — greenly — a question the other two had stopped asking. Merging the guards was
# rejected for separate reasons (decision-ledger row D62 — their scan surfaces and their zero-floors
# must not merge); what is shared is the FACT, with each consumer injecting its
# own reading of it — this one takes the platform values in matrix order, asset-name takes the `os:`
# field too, deploy-facts takes only the count.
# A `.mjs` helper was considered and rejected: there is no YAML parser available offline in the
# pre-commit lane (no dependency install runs there), so a node helper would be a hand-written
# indentation scanner too — the same parsing risk in another language, plus a node runtime on the
# critical path of a git hook, for zero gain in fidelity. This script's stated contract of "no deps
# beyond grep/sed/awk" exists because jq is absent on this project's Windows dev box; that constraint
# has not changed.
#
# ## Subject 2 — the integrity-hash invariant
# `packages[].fileSha256` is either ABSENT (nothing is claimed) or a well-formed, non-placeholder hash
# written by the publish job from that entry's own asset. A 64-hex value that verifies nothing is the
# state this guard makes unreachable.
#
# ### What --stamped does NOT prove
# It does not re-hash the .mcpb bytes. In the publish job the hash and the identifier URL are written
# by the SAME loop iteration, so re-running `sha256sum` there would only recompute the number the loop
# just computed — it cannot disagree. What can silently go wrong is the loop's POSITIONAL indexing
# missing or misaligning an entry, and that is exactly what subject 1 plus the one-per-package check
# below cover. So: this guard proves "a real hash was written for every package, by a loop that still
# lines up with packages[]" — not "these bytes hash to this value".
#
# ### Why subject 2 exists
# `server.json` is the MCP-registry entry (`io.github.eezz4/zzop`). Its `packages[]` point at
# per-platform `.mcpb` bundles that only exist AFTER a release builds them, so no honest hash can be
# committed. The tree used to fill the field with sixty-four `0` characters instead. That value is
# schema-shaped (`^[a-f0-9]{64}$`), so it reads as an integrity hash to every consumer that does not
# know the convention: a client that verifies it fails the install, and a client that treats
# all-zeros as "skip" gets NO integrity guarantee while its UI says it has one. Under this project's
# disclosure doctrine (over-claiming forbidden, under-reporting fine) the placeholder was the wrong
# side of the line, so the field is now simply omitted — `fileSha256` is NOT in the MCP server schema's
# per-package `required` list (2025-12-11: `registryType`, `identifier`, `transport`), so an absent
# field is valid and says exactly the true thing: this file claims no hash.
#
# The real hashes are stamped at publish time by `.github/workflows/prebuild.yml`'s
# `publish-mcp-registry` job, which downloads the release's own `.mcpb` assets and `sha256sum`s them.
# `jq`'s assignment creates the key, so omitting it from the committed file changes nothing there.
#
# ## Modes
#   (default)   The COMMITTED file. Asserts the three-way equality, then fails if any `fileSha256` is
#               present at all. Wired into .githooks/pre-commit and ci.yml's guards job like every
#               other scripts/check-*.sh.
#   --stamped [path]
#               A file that has just been stamped, run as the PUBLISH BLOCKER immediately before
#               `mcp-publisher publish`. Asserts the same three-way equality, then fails unless EVERY
#               package carries a 64-lowercase-hex `fileSha256` that is not all-zeros, one per package.
#               `path` defaults to server.json (an explicit path exists so the mode is testable against
#               a fixture without mutating the tree).
#   --print-matrix-platforms
#               Prints the build matrix's `platform:` values, one per line, and nothing else — a THIN
#               wrapper over scripts/lib/release-matrix.sh, kept as a flag on this script because
#               prebuild.yml's stamping step calls it by that name and that contract is not worth
#               breaking to save a wrapper. It exists so that the stamping job and this guard cannot
#               end up with TWO parsers of one matrix: if they disagreed, this guard could pass on set
#               A at commit time while the job stamped set B at release time — which is the very hole
#               the three-way equality closes, reopened one layer down. Since 2026-08-08 that "one
#               parser" is literally one file (scripts/lib/release-matrix.sh), shared with the prose
#               guards that used to keep awk copies of it. Aborts (non-zero, empty stdout) on any
#               extraction failure.
#
# No deps beyond grep/sed/awk — deliberately NOT jq, which is absent on this project's Windows dev
# box while every other guard runs there.
set -euo pipefail
cd "$(dirname "$0")/.."

# shellcheck source=scripts/lib/release-matrix.sh
. ./scripts/lib/release-matrix.sh

SELF=check-server-json-hashes
PREBUILD=.github/workflows/prebuild.yml
ZEROS=0000000000000000000000000000000000000000000000000000000000000000

MODE=committed
SERVER_JSON=server.json
case "${1:-}" in
  "") ;;
  --stamped)
    MODE=stamped
    SERVER_JSON="${2:-server.json}"
    ;;
  --print-matrix-platforms)
    MODE=print-matrix
    [ $# -eq 1 ] || { echo "$SELF: --print-matrix-platforms takes no further arguments" >&2; exit 1; }
    ;;
  *)
    echo "$SELF: unknown argument '$1' (usage: $0 [--stamped [path] | --print-matrix-platforms])" >&2
    exit 1
    ;;
esac

abort() { echo "$SELF: $*" >&2; exit 1; }

[ -f "$PREBUILD" ] || abort "$PREBUILD -- missing. The build matrix and the stamping loop this guard
  checks against both live there."
if [ "$MODE" != print-matrix ]; then
  [ -f "$SERVER_JSON" ] || abort "$SERVER_JSON -- missing (moved/renamed?). This guard cannot run without it."
fi

# --- A. The build matrix: the authority -----------------------------------------------------------
# The structural walk moved to scripts/lib/release-matrix.sh (2026-08-08) — see the "Parsing route"
# section of the header. release_matrix_entries() emits one `<platform><TAB><os>` record per matrix
# entry in matrix order and owns every floor this section used to spell out itself: no summary line
# (its own awk broke), the walk never reaching include: (a renamed job/key), zero entries, an entry
# with no `platform:` key, and an empty record list. Each of those ABORTS, because reporting OK over
# zero platforms is how a guard certifies a set it never read (working-agreements §5.5).
matrix_records="$(release_matrix_entries "$SELF" "$PREBUILD")" || exit 1

# What THIS guard injects on top of the shared read: the platform values, in matrix order, and nothing
# else — the three-way equality below is positional, so the order is the load-bearing half. The `os:`
# field the same records carry is check-asset-name-prose.sh's business, not this one's.
matrix_platforms="$(release_matrix_platforms "$matrix_records")"
[ -n "$matrix_platforms" ] || abort \
  "projected 0 platform values out of a non-empty matrix record list -- that is this script's own
  projection failing, not a tree problem. Every check below would compare empty lists and pass."

if [ "$MODE" = print-matrix ]; then
  printf '%s\n' "$matrix_platforms"
  exit 0
fi

# --- B. The `platforms=(...)` mirror in the publish job --------------------------------------------
# Exactly one such line must exist: two would silently concatenate here, and the stamping job would
# use whichever bash assigned last.
mirror_raw="$(sed -n 's/^[[:space:]]*platforms=(\(.*\))[[:space:]]*$/\1/p' "$PREBUILD")"
mirror_hits="$(printf '%s\n' "$mirror_raw" | grep -c . || true)"
[ "$mirror_hits" -eq 1 ] || abort \
  "found $mirror_hits 'platforms=(...)' array line(s) in $PREBUILD; expected exactly 1.
  That array is the publish job's declaration of what it is about to stamp. At 0 the declaration was
  renamed or reshaped and this guard can no longer prove it matches the matrix; above 1 it is
  ambiguous which one the job actually uses. Re-point this extraction at the new spelling; do not
  delete the check."
loop_platforms="$(tr ' ' '\n' <<< "$mirror_raw")"

# --- C. server.json's packages[] -------------------------------------------------------------------
# In file order, each reduced to the platform token its own identifier URL names
# (.../zzop-mcp-<platform>.mcpb). One line per package; a package whose identifier does not have that
# shape yields the literal `?` so it can be reported rather than silently dropped.
pkg_platforms="$(sed -n 's/.*"identifier"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$SERVER_JSON" \
  | sed -e 's#.*/zzop-mcp-\([A-Za-z0-9._-]*\)\.mcpb$#\1#' -e 't' -e 's#.*#?#')"

[ -n "$pkg_platforms" ] || abort \
  "extracted 0 package identifiers from $SERVER_JSON -- the file layout changed and every check below
  would vacuously pass. Fix the extraction rather than trusting a green run."

# The asymmetry in the next two lines is DELIBERATE, and the abort above is what licenses it.
#
# `grep -c` exits 1 on a count of 0, and under `set -e -o pipefail` that kills the script on the
# assignment with no output at all. This line survives without `|| true` only because the `[ -n
# "$pkg_platforms" ]` abort directly above has already refused the empty case — i.e. ORDER IS
# LOAD-BEARING here. Moving, weakening or deleting that abort silently converts this line into a mute
# exit 1. The line below legitimately CAN count zero (no malformed identifiers is the healthy state),
# so it carries `|| true`; this one cannot reach zero without the guard being broken already.
#
# Do NOT "fix" this by adding `|| true` for symmetry — it would make things worse, not merely
# redundant. With the abort gone and `|| true` here, pkg_count becomes 0 and flows on: the three-way
# equality below compares three empty strings and passes, and `[ "$sha_count" -ne "$pkg_count" ]`
# compares 0 against 0 and passes. That is a vacuous green, which is the exact defect this file's
# floors exist to prevent. A loud death is worse than a diagnosis but far better than a false clean.
pkg_count="$(printf '%s\n' "$pkg_platforms" | grep -c .)"
bad="$(printf '%s\n' "$pkg_platforms" | grep -c '^?$' || true)"
[ "$bad" -eq 0 ] || abort \
  "$bad package identifier(s) in $SERVER_JSON do not end in /zzop-mcp-<platform>.mcpb -- the asset
  naming changed and the correspondence below can no longer be derived."

# --- The three-way equality ------------------------------------------------------------------------
if [ "$matrix_platforms" != "$loop_platforms" ] || [ "$loop_platforms" != "$pkg_platforms" ]; then
  echo "$SELF: the three release-platform lists disagree." >&2
  echo "  A. build matrix ($PREBUILD, jobs.build.strategy.matrix.include[].platform) -- THE AUTHORITY:" >&2
  printf '       %s\n' $matrix_platforms >&2
  echo "  B. platforms=(...) mirror in the publish-mcp-registry job:" >&2
  printf '       %s\n' $loop_platforms >&2
  echo "  C. $SERVER_JSON packages[] (by identifier URL):" >&2
  printf '       %s\n' $pkg_platforms >&2
  echo >&2
  [ "$matrix_platforms" = "$loop_platforms" ] || echo "  MISMATCH: A vs B -- the matrix and the mirror name different platforms (or a different order)." >&2
  [ "$loop_platforms" = "$pkg_platforms" ] || echo "  MISMATCH: B vs C -- the mirror and $SERVER_JSON's packages[] name different platforms (or a different order)." >&2
  [ "$matrix_platforms" = "$pkg_platforms" ] || echo "  MISMATCH: A vs C -- the matrix and $SERVER_JSON's packages[] name different platforms (or a different order)." >&2
  echo >&2
  echo "  A is the authority: it decides what gets built and uploaded. B and C are copies of it, and" >&2
  echo "  the publish job writes .packages[\$i] BY POSITION, so while these lists differ a release" >&2
  echo "  would stamp a hash onto the wrong asset, or upload a bundle it never lists at all -- with no" >&2
  echo "  error anywhere, because jq assigning past the end of an array is not an error." >&2
  echo "  Fix: make B and C match A exactly (same values, same order). A new platform needs a matrix" >&2
  echo "  entry, the same token in platforms=(...), and a packages[] entry whose identifier URL ends" >&2
  echo "  in /zzop-mcp-<platform>.mcpb." >&2
  echo "  This runs at COMMIT time on purpose: the release job asks the same question only after the" >&2
  echo "  GitHub release and the npm publish have already happened." >&2
  exit 1
fi

sha_lines="$(sed -n 's/.*"fileSha256"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$SERVER_JSON")"
sha_count="$(printf '%s\n' "$sha_lines" | grep -c . || true)"

# --- Mode: committed ------------------------------------------------------------------------------
if [ "$MODE" = committed ]; then
  if [ "$sha_count" -ne 0 ]; then
    echo "$SELF: $SERVER_JSON declares $sha_count fileSha256 value(s); the committed file must declare none." >&2
    printf '    %s\n' $sha_lines >&2
    echo >&2
    echo "  The assets these entries point at are built by the release that has not run yet, so no" >&2
    echo "  value here can be a hash of them. A placeholder is worse than absence, not better: it is" >&2
    echo "  schema-shaped, so a consumer reads it as an integrity guarantee it does not have." >&2
    echo "  Delete the field. .github/workflows/prebuild.yml's publish-mcp-registry job adds the real" >&2
    echo "  hash from the actual uploaded asset, and that same job runs this guard's --stamped mode" >&2
    echo "  as a publish blocker afterwards." >&2
    exit 1
  fi
  echo "$SELF: clean ($pkg_count platforms, identical in the build matrix, the platforms=(...) mirror
and $SERVER_JSON's packages[]; no fileSha256 claimed)."
  exit 0
fi

# --- Mode: stamped (publish blocker) --------------------------------------------------------------
fail=0
if [ "$sha_count" -ne "$pkg_count" ]; then
  echo "$SELF: $SERVER_JSON has $pkg_count package(s) but $sha_count fileSha256 value(s) -- the" >&2
  echo "  stamping loop did not reach every entry. Publishing now would list an asset with no" >&2
  echo "  integrity hash at all." >&2
  fail=1
fi

i=0
while IFS= read -r sha; do
  [ -n "$sha" ] || continue
  i=$((i + 1))
  plat="$(printf '%s\n' "$pkg_platforms" | sed -n "${i}p")"
  if [ "$sha" = "$ZEROS" ]; then
    echo "$SELF: packages[$((i - 1))] ($plat) still carries the all-zeros placeholder -- REFUSING TO PUBLISH." >&2
    fail=1
  elif ! grep -qE '^[0-9a-f]{64}$' <<< "$sha"; then
    echo "$SELF: packages[$((i - 1))] ($plat) fileSha256 '$sha' is not 64 lowercase hex digits." >&2
    fail=1
  fi
done <<< "$sha_lines"

if [ "$fail" -ne 0 ]; then
  echo >&2
  echo "$SELF: every packages[].fileSha256 must be a real SHA-256 of that entry's own uploaded .mcpb" >&2
  echo "asset before the registry entry goes out. Registry clients verify this hash on install, so a" >&2
  echo "placeholder or a short/uppercase value silently breaks every install while looking correct." >&2
  exit 1
fi

echo "$SELF: clean ($pkg_count platforms, identical in the build matrix, the platforms=(...) mirror and
$SERVER_JSON's packages[]; one well-formed non-placeholder fileSha256 each) -- no placeholder is going out."
