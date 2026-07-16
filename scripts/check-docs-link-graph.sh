#!/usr/bin/env bash
# Docs link-graph guard — fails when a documentation page or example is orphaned from its hub.
#
# The drift class (bitten 2026-07-16): docs/modules/mcp.md shipped in v0.16.0 but the docs hub
# (docs/README.md) never gained a row, and examples/README.md's index listed only 6 of 9 entries
# (auth-overlay-adapter, oazapfts-adapter, adapter-kit missing) — new pages were un-discoverable
# from the surface readers actually start at.
#
# Two containment checks (asymmetric, token-level — same idiom as the other sync guards):
#   1. every tracked docs/**/*.md (except the hub itself) must be referenced by its docs-relative
#      path (e.g. `modules/mcp.md`, `adapters/README.md`) somewhere in docs/README.md.
#   2. every entry directly under examples/ (directory or file, except README.md) must be
#      referenced by name somewhere in examples/README.md.
# Hub prose quality is NOT checked — only that a reference exists at all.
#
# No deps beyond git + grep. Exit 1 on any orphan, listing them.
set -euo pipefail
cd "$(dirname "$0")/.."

fail=0

# --- Check 1: docs/**/*.md all referenced from docs/README.md ---
hub=docs/README.md
[ -f "$hub" ] || { echo "check-docs-link-graph: missing $hub" >&2; exit 1; }
orphans=""
while IFS= read -r f; do
  rel="${f#docs/}"
  [ "$rel" = "README.md" ] && continue
  grep -qF "$rel" "$hub" || orphans="$orphans $rel"
done < <(git ls-files -- 'docs/**/*.md' 'docs/*.md')
if [ -n "$orphans" ]; then
  echo "check-docs-link-graph: docs pages not referenced from docs/README.md (orphaned from the hub):" >&2
  printf '    %s\n' $orphans >&2
  fail=1
fi

# --- Check 2: examples/* entries all referenced from examples/README.md ---
exhub=examples/README.md
[ -f "$exhub" ] || { echo "check-docs-link-graph: missing $exhub" >&2; exit 1; }
orphans=""
while IFS= read -r entry; do
  [ "$entry" = "README.md" ] && continue
  grep -qF "$entry" "$exhub" || orphans="$orphans $entry"
done < <(git ls-files -- 'examples/*' 'examples/**' | sed 's|^examples/||; s|/.*||' | sort -u)
if [ -n "$orphans" ]; then
  echo "check-docs-link-graph: examples entries not referenced from examples/README.md:" >&2
  printf '    %s\n' $orphans >&2
  fail=1
fi

if [ "$fail" -ne 0 ]; then
  echo "check-docs-link-graph: FAILED — add the missing hub reference (or remove the orphan)." >&2
  exit 1
fi
echo "check-docs-link-graph: OK (docs + examples hubs reference every entry)"
