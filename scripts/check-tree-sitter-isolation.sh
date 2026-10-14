#!/usr/bin/env bash
# tree-sitter isolation guard — fails when a tree-sitter/tree-sitter-<grammar> crate is depended on, or
# tree-sitter types/APIs are used, outside the allowlisted tree-sitter-based parser crates.
#
# Architecture guarantee: the engine never holds tree-sitter CSTs; tree-sitter is confined to the parser
# crates in ALLOWLIST below, each of which projects source into the Common IR (see
# crates/core/src/lib.rs's module doc: "swc / external-parser types never leak in" — the same guarantee,
# extended to tree-sitter) and mirrors the swc/ruff/syn isolation discipline
# check-swc-isolation.sh/check-ruff-isolation.sh/check-syn-isolation.sh enforce for their own frontends.
# This script is the regression guard for that guarantee, on the tree-sitter side.
#
# Multi-crate allowlist (not a single hardcoded path like the syn/ruff/swc guards): tree-sitter is the
# shared runtime underneath every GRAMMAR-based frontend this workspace adds, one crate per language
# (parser-go today; a future parser-java-21, parser-c-sharp, ... each get their own tree-sitter-<grammar>
# crate). Adding the next one is a one-line ALLOWLIST edit below, nothing else in this script changes.

# Multi-crate allowlist: tree-sitter is the only vendor confined to more than one crate, one per
# grammar. Adding the next grammar is a one-line edit here and nothing else.
ALLOWLIST=(
  "parser/parser-go"
  "parser/parser-java-21"
  "parser/parser-csharp"
)

set -euo pipefail
cd "$(dirname "$0")/.."

# A stale entry must fail LOUDLY rather than silently shrinking the allowlist — an allowlist that
# names a moved crate exempts nothing and the guard still prints clean.
for dir in "${ALLOWLIST[@]}"; do
  [ -d "$dir" ] || {
    echo "tree-sitter isolation guard: stale ALLOWLIST entry '$dir' -- directory does not exist" >&2
    echo "  (crate renamed or moved?). An allowlist entry that matches nothing exempts nothing." >&2
    exit 1
  }
done

# Isolation is half of what the allowlist is for; the other half is the DEPTH CAP, and until
# 2026-10-12 nothing checked it. A grammar frontend hands tree-sitter a file of arbitrary nesting, and
# an unguarded recursive walk takes the PROCESS down rather than returning `degraded` -- so every crate
# on this list must arm zzop_core::cst_depth::exceeds_max_depth. Measured when this check was written:
# the three allowlisted crates each call it exactly once (parser/parser-*/src/util.rs) and the needle
# had 0 hits anywhere under scripts/, .githooks/ or .github/ -- i.e. a fourth grammar could join by a
# one-line edit above and parse with no depth guard at all, and both guards that WOULD go red
# (check-dep-closure, this script's isolation half) are satisfied by that same one line and neither
# says the word "depth".
# The trailing '\(' matters: a bare substring needle also matches 'exceeds_max_depth_RENAMED', so a
# suffix rename passes both halves of this check silently. Found by this check's own invalidation
# drill. Not \b -- git grep's default engine is POSIX ERE and has no word boundary.
HELPER='zzop_core::cst_depth::exceeds_max_depth\('
# Floor value: if the shared helper itself is gone, every per-crate needle below reads 0 and this
# check would blame the crates for a rename in core. A needle that matches nothing is not a clean
# report, so say which of the two happened.
if ! git grep -qE 'pub fn exceeds_max_depth[<(]' -- crates/core/src/cst_depth.rs; then
  echo "tree-sitter isolation guard: the shared depth cap 'exceeds_max_depth' is gone from" >&2
  echo "  crates/core/src/cst_depth.rs. This check's needle is dead -- 0 hits below would mean" >&2
  echo "  'not looking', not 'nothing wrong'. Point HELPER at the new name." >&2
  exit 1
fi
for dir in "${ALLOWLIST[@]}"; do
  if ! git grep -qE "$HELPER" -- "$dir"; then
    echo "tree-sitter isolation guard: '$dir' is allowlisted for tree-sitter but never arms the" >&2
    echo "  CST depth cap ($HELPER)." >&2
    echo "  A grammar frontend without it does not return 'degraded' on a pathologically nested file --" >&2
    echo "  it overflows the stack and takes the process down. Call it from this crate's tree walk," >&2
    echo "  the way parser/parser-go/src/util.rs does." >&2
    echo "  And if this is a NEW language, re-take the census too: DEEPEST_REAL_CST_DEPTH_MEASURED in" >&2
    echo "  crates/core/src/cst_depth.rs is the deepest real file over the languages measured so far," >&2
    echo "  and a new grammar is a new population for that bound." >&2
    exit 1
  fi
done

. ./scripts/lib/vendor-isolation.sh

run_vendor_isolation "tree-sitter isolation guard" "tree-sitter" \
  'tree-sitter(-[a-z0-9]+)*' \
  '\btree_sitter(_[a-z0-9]+)*::|use\s+tree_sitter|extern\s+crate\s+tree[_-]sitter' \
  'tree-sitter must stay confined to the grammar-backed parser crates -- the engine never holds a
tree-sitter tree, and a grammar upgrade'"'"'s re-verification scope is one parser crate rather than the
whole workspace.' \
  "${ALLOWLIST[@]}"
