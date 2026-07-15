#!/usr/bin/env bash
# swc isolation guard — fails when an swc_* crate is depended on, or swc_core is used, outside
# parser/parser-typescript.
#
# Architecture guarantee: the engine never holds swc ASTs; swc is confined to parser-typescript,
# which projects source into the Common IR (see crates/core/src/lib.rs's module doc: "swc /
# external-parser types never leak in") and the workspace root Cargo.toml's "swc version
# isolation" note (an swc upgrade's re-verification scope is one crate, not the whole workspace).
# This script is the regression guard for that guarantee.
#
# Two checks:
#  1. Cargo.toml dependency lines declaring an `swc_<name>` (or `swc-<name>`) crate, in any
#     Cargo.toml except parser/parser-typescript/Cargo.toml and the workspace root Cargo.toml
#     (the latter is where the swc pin/isolation note lives, per DESIGN.md — exempted even though
#     it does not currently declare swc as an actual [workspace.dependencies] entry).
#  2. `swc_core::` or `use swc_...` in any .rs file outside parser/parser-typescript/src/.
#
# No deps beyond grep -P (PCRE). Exit 1 on any violation, listing file:line.
set -euo pipefail
cd "$(dirname "$0")/.."

violations=0

echo "swc isolation guard: checking Cargo.toml dependency declarations..."
DEP_PATTERN='^\s*swc[_-][A-Za-z0-9_-]*\s*='
cargo_files=$(grep -rlP "$DEP_PATTERN" . --include='Cargo.toml' 2>/dev/null \
  | grep -v '/target/' \
  | grep -v '^\./\.claude/' \
  | grep -v -x './Cargo.toml' \
  | grep -v -x './parser/parser-typescript/Cargo.toml' || true)

if [ -n "$cargo_files" ]; then
  echo "swc isolation guard: swc_* dependency declared outside parser-typescript:"
  while IFS= read -r f; do
    grep -nP "$DEP_PATTERN" "$f" | sed "s|^|  ${f#./}:|"
  done <<< "$cargo_files"
  violations=1
fi

echo "swc isolation guard: checking .rs source usage..."
USE_PATTERN='swc_core::|use\s+swc_'
rs_files=$(grep -rlP "$USE_PATTERN" . --include='*.rs' 2>/dev/null \
  | grep -v '/target/' \
  | grep -v '^\./\.claude/' \
  | grep -v '^\./parser/parser-typescript/src/' || true)

if [ -n "$rs_files" ]; then
  echo "swc isolation guard: swc_core usage found outside parser-typescript/src:"
  while IFS= read -r f; do
    grep -nP "$USE_PATTERN" "$f" | sed "s|^|  ${f#./}:|"
  done <<< "$rs_files"
  violations=1
fi

if [ "$violations" -ne 0 ]; then
  echo
  echo "swc must stay confined to parser/parser-typescript (see crates/core/src/lib.rs and the"
  echo "workspace root Cargo.toml's \"swc version isolation\" note) -- the engine must never hold"
  echo "swc ASTs directly."
  exit 1
fi

echo "swc isolation guard: clean."
