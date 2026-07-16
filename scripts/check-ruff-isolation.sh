#!/usr/bin/env bash
# ruff isolation guard — fails when a ruff_* crate is depended on, or ruff_ types/APIs are used, outside
# parser/parser-python-3.
#
# Architecture guarantee: the engine never holds ruff ASTs; ruff is confined to parser-python-3, which
# projects source into the Common IR (see crates/core/src/lib.rs's module doc: "swc / external-parser
# types never leak in" — the same guarantee, extended to ruff) and mirrors the swc isolation discipline
# check-swc-isolation.sh enforces for parser-typescript. This script is the regression guard for that
# guarantee, on the Python side.
#
# Two checks:
#  1. Cargo.toml dependency lines declaring a `ruff_<name>` (or `ruff-<name>`) crate, in any Cargo.toml
#     except parser/parser-python-3/Cargo.toml and the workspace root Cargo.toml (exempted for the same
#     reason check-swc-isolation.sh exempts the root Cargo.toml for swc: not itself a dependency
#     declaration site today, but a legitimate place for a future pin/isolation note).
#  2. `use ruff_...` or `ruff_python_...::` in any .rs file outside parser/parser-python-3/src/.
#
# Scope: git-TRACKED files only (git ls-files), for the same reason as check-swc-isolation.sh —
# the working tree also holds gitignored/untracked local corpora (cloned third-party repos,
# benchmark checkouts) whose own ruff usage is not ours to police. Anything that could ship must
# be tracked, so tracked-only is exactly the isolation surface (and matches what CI checks out).
#
# No deps beyond git + grep -P (PCRE). Exit 1 on any violation, listing file:line.
set -euo pipefail
cd "$(dirname "$0")/.."

violations=0

echo "ruff isolation guard: checking Cargo.toml dependency declarations..."
DEP_PATTERN='^\s*ruff[_-][A-Za-z0-9_-]*\s*='
cargo_files=$(git ls-files -z -- 'Cargo.toml' '*/Cargo.toml' \
  | xargs -0 -r grep -lP "$DEP_PATTERN" -- 2>/dev/null \
  | grep -v '/target/' \
  | grep -v '^\.claude/' \
  | grep -v -x 'Cargo.toml' \
  | grep -v -x 'parser/parser-python-3/Cargo.toml' || true)

if [ -n "$cargo_files" ]; then
  echo "ruff isolation guard: ruff_* dependency declared outside parser-python-3:"
  while IFS= read -r f; do
    grep -nP "$DEP_PATTERN" "$f" | sed "s|^|  ${f#./}:|"
  done <<< "$cargo_files"
  violations=1
fi

echo "ruff isolation guard: checking .rs source usage..."
USE_PATTERN='ruff_python_[A-Za-z0-9_]*::|use\s+ruff_'
rs_files=$(git ls-files -z -- '*.rs' \
  | xargs -0 -r grep -lP "$USE_PATTERN" -- 2>/dev/null \
  | grep -v '/target/' \
  | grep -v '^\.claude/' \
  | grep -v '^parser/parser-python-3/src/' || true)

if [ -n "$rs_files" ]; then
  echo "ruff isolation guard: ruff usage found outside parser-python-3/src:"
  while IFS= read -r f; do
    grep -nP "$USE_PATTERN" "$f" | sed "s|^|  ${f#./}:|"
  done <<< "$rs_files"
  violations=1
fi

if [ "$violations" -ne 0 ]; then
  echo
  echo "ruff must stay confined to parser/parser-python-3 (see crates/core/src/lib.rs's isolation note,"
  echo "and check-swc-isolation.sh's identical discipline for swc/parser-typescript) -- the engine must"
  echo "never hold ruff ASTs directly."
  exit 1
fi

echo "ruff isolation guard: clean."
