#!/usr/bin/env bash
# check-mcp-registry-limits.sh — `server.json`'s own field lengths must satisfy the MCP registry
# schema it declares, checked HERE rather than by the registry after a release is already tagged.
#
# ## The defect this exists for (2026-09-23, review ledger V292)
#
# v0.35.0 shipped with a rewritten `description`. Every other surface took it fine -- the npm
# packages, the GitHub release, the Claude Code plugin manifests, the site. The MCP registry did not:
#
#   422 Unprocessable Entity — {"message":"expected length <= 100","location":"body.description"}
#
# The string was 187 characters. `publish-mcp-registry` is the LAST job of the release workflow, so
# the tag, the release, its 18 assets and all six npm packages were already published and immovable
# by the time anything said a word. The release was half-delivered: every channel carried 0.35.0 and
# the registry stayed on 0.34.0, which is exactly the shape working-agreements §6.2 calls a
# half-release. Nothing local could have caught it -- the limit lived only in a remote schema.
#
# It is also the plainest possible instance of this repo's own rule that a constraint only the remote
# knows is not a constraint the repo can keep: the 14 previously registered versions all carried a
# 94-character description and passed, so the limit had never been TESTED, only accidentally obeyed.
#
# ## Why the schema is vendored and the numbers are not written here
#
# A guard that hard-codes `100` is a second copy of a fact the registry owns, and §5.5 is the record
# of what those cost. So `docs/contracts/mcp-server.schema.json` is a byte copy of the schema that
# `server.json` itself names in its `$schema`, and every limit below is READ OUT OF IT. This file
# contains no length number at all; grep it and see.
#
# Refresh the copy with the URL `server.json` declares:
#   curl -fsSL "$(jq -r '."$schema"' server.json)" -o docs/contracts/mcp-server.schema.json
#
# The first assertion is that the vendored copy's `$id` still equals that declaration. That is what
# keeps the vendoring from rotting into a lie: bump `$schema` and this guard fails until the copy is
# refreshed, instead of quietly validating against last year's rules.
#
# ## What this guard does NOT see (stated, not discovered later)
#
# It checks the TOP-LEVEL string fields of `server.json` against `$defs.ServerDetail.properties`.
# It does not walk `packages[]`, `remotes[]` or `icons[]`, it does not evaluate `pattern`, `enum` or
# `required`, and it does not validate the document against the schema as a schema -- that would want
# a JSON-Schema validator this repo does not carry. It covers the field that broke a release and its
# siblings of the same shape. A violation inside `packages[]` still reaches the registry unannounced.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

SCHEMA=docs/contracts/mcp-server.schema.json
SERVER=server.json

command -v jq > /dev/null 2>&1 || { echo "check-mcp-registry-limits: jq is not installed -- this guard cannot run." >&2; exit 1; }
for f in "$SCHEMA" "$SERVER"; do
  [ -f "$f" ] || { echo "check-mcp-registry-limits: missing $f -- re-anchor this guard." >&2; exit 1; }
done

declared="$(jq -r '."$schema" // empty' "$SERVER")"
vendored="$(jq -r '."$id" // empty' "$SCHEMA")"
if [ -z "$declared" ] || [ -z "$vendored" ] || [ "$declared" != "$vendored" ]; then
  echo "check-mcp-registry-limits: the vendored schema is not the one server.json declares." >&2
  echo "  server.json \$schema : ${declared:-<absent>}" >&2
  echo "  vendored   \$id      : ${vendored:-<absent>}" >&2
  echo "  Validating against a schema the document does not claim proves nothing. Refresh the copy:" >&2
  echo "    curl -fsSL \"\$(jq -r '.[\"\$schema\"]' server.json)\" -o $SCHEMA" >&2
  exit 1
fi

# Every top-level string field of server.json that ServerDetail constrains, judged against the copy.
# `checked` is printed so a silently-narrowed subject shows up as a number that fell, not as silence.
report="$(jq -r --slurpfile s "$SCHEMA" '
  (($s[0]["$defs"] // $s[0].definitions).ServerDetail.properties) as $p
  | [ to_entries[]
      | select(.value | type == "string")
      | . as $f
      | ($p[$f.key] // {}) as $c
      | select(($c.maxLength // null) != null or ($c.minLength // null) != null)
      | { field: $f.key, len: ($f.value | length), max: $c.maxLength, min: $c.minLength }
    ] as $rows
  | ( [ $rows[] | select((.max != null and .len > .max) or (.min != null and .len < .min)) ] ) as $bad
  | "CHECKED \($rows | length)",
    ( $bad[] | "BAD \(.field) length=\(.len) max=\(.max // "-") min=\(.min // "-")" )
' "$SERVER")"

checked="$(printf '%s\n' "$report" | awk '$1 == "CHECKED" { print $2 }')"
bad="$(printf '%s\n' "$report" | awk '$1 == "BAD"')"

if [ -z "${checked:-}" ]; then
  echo "check-mcp-registry-limits: extracted ZERO constrained fields from $SCHEMA." >&2
  echo "  An empty subject is a broken guard, never a schema with no limits. The extraction reads" >&2
  echo "  \$defs.ServerDetail.properties and keeps the entries carrying maxLength or minLength." >&2
  exit 1
fi

if [ -n "$bad" ]; then
  echo "check-mcp-registry-limits: server.json violates the schema it declares:" >&2
  printf '%s\n' "$bad" | sed 's/^BAD /  /' >&2
  echo "  The MCP registry rejects this with 422 at the LAST job of the release workflow, after the" >&2
  echo "  tag, the release assets and the npm packages are already published and immovable." >&2
  echo "  Shorten the field in $SERVER. The limit belongs to the registry, not to this repo -- read it" >&2
  echo "  with: jq '.\"\$defs\".ServerDetail.properties' $SCHEMA" >&2
  exit 1
fi

echo "check-mcp-registry-limits: OK ($checked constrained top-level field(s) within the declared schema)."
