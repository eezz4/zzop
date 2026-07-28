#!/usr/bin/env bash
# server.json integrity-hash guard — two modes, one invariant: `packages[].fileSha256` is either
# ABSENT (nothing is claimed) or a well-formed, non-placeholder hash written by the publish job from
# that entry's own asset. A 64-hex value that verifies nothing is the state this guard makes
# unreachable.
#
# ## What --stamped does NOT prove (stated so the green line is not read as more than it is)
# It does not re-hash the .mcpb bytes. In the publish job the hash and the identifier URL are written
# by the SAME loop iteration, so re-running `sha256sum` there would only recompute the number the loop
# just computed — it cannot disagree. What can silently go wrong is the loop's POSITIONAL indexing
# missing or misaligning an entry, and that is exactly what the correspondence + one-per-package
# checks below cover. So: this guard proves "a real hash was written for every package, by a loop that
# still lines up with packages[]" — not "these bytes hash to this value".
#
# ## Why this exists
# `server.json` is the MCP-registry entry (`io.github.eezz4/zzop`). Its five `packages[]` point at
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
# ## The second hole this closes (the reason a guard, not just a deletion)
# That stamping loop is POSITIONAL: it walks a hardcoded `platforms=(...)` array and writes
# `.packages[$i]`. Nothing checks that the array and `packages[]` still describe the same five
# assets in the same order. A sixth package entry, a reordering, or a renamed platform would be
# published with the WRONG hash — or with no hash written at all — and `set -euo pipefail` cannot
# see it, because `jq` assigning past the end of an array is not an error. That is a silent failure
# in the one field whose entire job is to be non-silent. Both modes below assert the correspondence.
#
# ## Modes
#   (default)   The COMMITTED file. Fails if any `fileSha256` is present at all, and asserts the
#               packages[]-to-`platforms=(...)` correspondence. Wired into .githooks/pre-commit and
#               ci.yml's guards job like every other scripts/check-*.sh.
#   --stamped [path]
#               A file that has just been stamped, run as the PUBLISH BLOCKER immediately before
#               `mcp-publisher publish`. Fails unless EVERY package carries a 64-lowercase-hex
#               `fileSha256` that is not all-zeros, one per package, plus the same correspondence
#               check. `path` defaults to server.json (an explicit path exists so the mode is
#               testable against a fixture without mutating the tree).
#
# No deps beyond grep/sed/awk — deliberately NOT jq, which is absent on this project's Windows dev
# box while every other guard runs there.
set -euo pipefail
cd "$(dirname "$0")/.."

SELF=check-server-json-hashes
PREBUILD=.github/workflows/prebuild.yml
ZEROS=0000000000000000000000000000000000000000000000000000000000000000

MODE=committed
SERVER_JSON=server.json
if [ "${1:-}" = "--stamped" ]; then
  MODE=stamped
  SERVER_JSON="${2:-server.json}"
elif [ $# -gt 0 ]; then
  echo "$SELF: unknown argument '$1' (usage: $0 [--stamped [path]])" >&2
  exit 1
fi

abort() { echo "$SELF: $*" >&2; exit 1; }

[ -f "$SERVER_JSON" ] || abort "$SERVER_JSON -- missing (moved/renamed?). This guard cannot run without it."
[ -f "$PREBUILD" ] || abort "$PREBUILD -- missing. The stamping loop this guard checks against lives there."

# --- The two orderings that must agree ------------------------------------------------------------
# `packages[]`, in file order, reduced to the platform token its own identifier URL names
# (.../zzop-mcp-<platform>.mcpb). One line per package; a package whose identifier does not have that
# shape yields the literal `?` so it can be reported rather than silently dropped.
pkg_platforms="$(sed -n 's/.*"identifier"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$SERVER_JSON" \
  | sed -e 's#.*/zzop-mcp-\([A-Za-z0-9._-]*\)\.mcpb$#\1#' -e 't' -e 's#.*#?#')"

# The stamping loop's own array, in its own order.
loop_platforms="$(sed -n 's/^[[:space:]]*platforms=(\(.*\))[[:space:]]*$/\1/p' "$PREBUILD" | tr ' ' '\n')"

[ -n "$loop_platforms" ] || abort \
  "could not read the 'platforms=(...)' array out of $PREBUILD -- the publish job's stamping loop was
  renamed or reshaped, so this guard can no longer prove server.json's packages[] still line up with
  what CI stamps. Re-point this extraction at the new spelling; do not delete the check."
[ -n "$pkg_platforms" ] || abort \
  "extracted 0 package identifiers from $SERVER_JSON -- the file layout changed and every check below
  would vacuously pass. Fix the extraction rather than trusting a green run."

if [ "$pkg_platforms" != "$loop_platforms" ]; then
  echo "$SELF: $SERVER_JSON's packages[] and $PREBUILD's stamping loop disagree." >&2
  echo "  packages[] (by identifier URL):" >&2
  printf '    %s\n' $pkg_platforms >&2
  echo "  platforms=(...) in the publish job:" >&2
  printf '    %s\n' $loop_platforms >&2
  echo >&2
  echo "  That loop writes .packages[\$i] by POSITION. While these two lists differ, a release would" >&2
  echo "  stamp a hash onto the wrong asset, or leave an entry unstamped, with no error anywhere." >&2
  exit 1
fi

pkg_count="$(printf '%s\n' "$pkg_platforms" | grep -c .)"
bad="$(printf '%s\n' "$pkg_platforms" | grep -c '^?$' || true)"
[ "$bad" -eq 0 ] || abort \
  "$bad package identifier(s) in $SERVER_JSON do not end in /zzop-mcp-<platform>.mcpb -- the asset
  naming changed and the correspondence above can no longer be derived."

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
  echo "$SELF: clean ($pkg_count packages, no fileSha256 claimed, packages[] matches the stamping loop)."
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

echo "$SELF: clean ($pkg_count packages, one well-formed non-placeholder fileSha256 each, packages[]
still aligned with the stamping loop) -- no placeholder is going out."
