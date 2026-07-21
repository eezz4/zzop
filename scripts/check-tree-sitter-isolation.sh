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
ALLOWLIST=(
  "parser/parser-go"
  "parser/parser-java-21"
  "parser/parser-csharp"
)

# Two checks, run once per allowlisted crate:
#  1. Cargo.toml dependency lines declaring `tree-sitter` or `tree-sitter-<grammar>`, in any Cargo.toml
#     except an allowlisted crate's own and the workspace root Cargo.toml (exempted for the same reason
#     check-swc-isolation.sh/check-ruff-isolation.sh/check-syn-isolation.sh exempt the root Cargo.toml:
#     not itself a dependency declaration site today, but a legitimate place for a future pin/isolation
#     note).
#  2. `use tree_sitter` or `tree_sitter::` in any .rs file outside an allowlisted crate's own src/.
#
# Scope: git-TRACKED files only (git ls-files), for the same reason as check-swc-isolation.sh /
# check-ruff-isolation.sh / check-syn-isolation.sh — the working tree also holds gitignored/
# untracked local corpora (cloned third-party repos, benchmark checkouts) whose own tree-sitter
# usage is not ours to police, and `tree_sitter::` is ubiquitous in real Rust crates (a
# `grep -r .` over the tree false-positives on every one of them). Anything that could ship must
# be tracked, so tracked-only is exactly the isolation surface (and matches what CI checks out).
#
# Enumeration mechanism (TRACKED-file discovery + grep + the standard target/node_modules/.claude
# exclusions) lives in scripts/lib/tracked-grep.sh, shared with check-syn-isolation.sh /
# check-swc-isolation.sh / check-ruff-isolation.sh — this script keeps only ITS OWN pattern,
# allowlist, and messages.
#
# No deps beyond git + grep -P (PCRE). Exit 1 on any violation, listing file:line.
set -euo pipefail
cd "$(dirname "$0")/.."
. ./scripts/lib/tracked-grep.sh

violations=0

for dir in "${ALLOWLIST[@]}"; do
  [ -d "$dir" ] || { echo "tree-sitter isolation guard: stale ALLOWLIST entry '$dir' -- directory does not exist (crate renamed/moved?)." >&2; exit 1; }
done

echo "tree-sitter isolation guard: checking Cargo.toml dependency declarations..."
# `(-[a-z0-9]+)*` (not `?`): grammar crate names can be multi-segment (`tree-sitter-c-sharp`) — a
# single-suffix pattern would let such a dependency slip past the guard (opus review F3).
DEP_PATTERN='^\s*(tree-sitter(-[a-z0-9]+)*)\s*='
# The enumeration call is kept OUTSIDE the `|| true` below on purpose: tracked_files_matching's own
# failure must still trip `set -e` and abort loud (see its header comment); only the root-Cargo.toml
# exclusion and the per-crate allowlist loop below are safe to swallow via `|| true`.
cargo_matches=$(tracked_files_matching "$DEP_PATTERN" 'Cargo.toml' '*/Cargo.toml')
cargo_files=$(grep -v -x 'Cargo.toml' <<< "$cargo_matches" || true)

for dir in "${ALLOWLIST[@]}"; do
  cargo_files=$(echo "$cargo_files" | grep -v -x "$dir/Cargo.toml" || true)
done

if [ -n "$cargo_files" ]; then
  echo "tree-sitter isolation guard: tree-sitter dependency declared outside the allowlisted parser crates:"
  while IFS= read -r f; do
    [ -n "$f" ] || continue
    grep -nP "$DEP_PATTERN" "$f" | sed "s|^|  ${f#./}:|"
  done <<< "$cargo_files"
  violations=1
fi

echo "tree-sitter isolation guard: checking .rs source usage..."
USE_PATTERN='\btree_sitter::[A-Za-z_]|use\s+tree_sitter(::|;|\s)'
rs_files=$(tracked_files_matching "$USE_PATTERN" '*.rs')

for dir in "${ALLOWLIST[@]}"; do
  rs_files=$(echo "$rs_files" | grep -v "^$dir/src/" || true)
done

if [ -n "$rs_files" ]; then
  echo "tree-sitter isolation guard: tree_sitter usage found outside the allowlisted parser crates' src/:"
  while IFS= read -r f; do
    [ -n "$f" ] || continue
    grep -nP "$USE_PATTERN" "$f" | sed "s|^|  ${f#./}:|"
  done <<< "$rs_files"
  violations=1
fi

if [ "$violations" -ne 0 ]; then
  echo
  echo "tree-sitter must stay confined to the allowlisted parser crates (see crates/core/src/lib.rs's"
  echo "isolation note, and check-swc-isolation.sh/check-ruff-isolation.sh/check-syn-isolation.sh's"
  echo "identical discipline for swc/ruff/syn) -- the engine must never hold tree-sitter CSTs directly."
  exit 1
fi

echo "tree-sitter isolation guard: clean."
