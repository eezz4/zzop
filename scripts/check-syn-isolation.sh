#!/usr/bin/env bash
# syn isolation guard — fails when a syn/proc-macro2 crate is depended on, or syn types/APIs are used,
# outside parser/parser-rust.
#
# Architecture guarantee: the engine never holds syn ASTs; syn is confined to parser-rust, which projects
# source into the Common IR (see crates/core/src/lib.rs's module doc: "swc / external-parser types never
# leak in" — the same guarantee, extended to syn) and mirrors the swc/ruff isolation discipline
# check-swc-isolation.sh/check-ruff-isolation.sh enforce for parser-typescript/parser-python-3. This script
# is the regression guard for that guarantee, on the Rust side.
#
# Two checks:
#  1. Cargo.toml dependency lines declaring `syn` or `proc-macro2`, in any Cargo.toml except
#     parser/parser-rust/Cargo.toml and the workspace root Cargo.toml (exempted for the same reason
#     check-swc-isolation.sh/check-ruff-isolation.sh exempt the root Cargo.toml: not itself a dependency
#     declaration site today, but a legitimate place for a future pin/isolation note).
#  2. `use syn` or `syn::` in any .rs file outside parser/parser-rust/src/.
#
# No deps beyond grep -P (PCRE). Exit 1 on any violation, listing file:line.
set -euo pipefail
cd "$(dirname "$0")/.."

violations=0

echo "syn isolation guard: checking Cargo.toml dependency declarations..."
DEP_PATTERN='^\s*(syn|proc-macro2)\s*='
cargo_files=$(grep -rlP "$DEP_PATTERN" . --include='Cargo.toml' 2>/dev/null \
  | grep -v '/target/' \
  | grep -v -x './Cargo.toml' \
  | grep -v -x './parser/parser-rust/Cargo.toml' || true)

if [ -n "$cargo_files" ]; then
  echo "syn isolation guard: syn/proc-macro2 dependency declared outside parser-rust:"
  while IFS= read -r f; do
    grep -nP "$DEP_PATTERN" "$f" | sed "s|^|  ${f#./}:|"
  done <<< "$cargo_files"
  violations=1
fi

echo "syn isolation guard: checking .rs source usage..."
USE_PATTERN='\bsyn::[A-Za-z_]|use\s+syn(::|;|\s)'
rs_files=$(grep -rlP "$USE_PATTERN" . --include='*.rs' 2>/dev/null \
  | grep -v '/target/' \
  | grep -v '^\./parser/parser-rust/src/' || true)

if [ -n "$rs_files" ]; then
  echo "syn isolation guard: syn usage found outside parser-rust/src:"
  while IFS= read -r f; do
    grep -nP "$USE_PATTERN" "$f" | sed "s|^|  ${f#./}:|"
  done <<< "$rs_files"
  violations=1
fi

if [ "$violations" -ne 0 ]; then
  echo
  echo "syn must stay confined to parser/parser-rust (see crates/core/src/lib.rs's isolation note, and"
  echo "check-swc-isolation.sh/check-ruff-isolation.sh's identical discipline for swc/parser-typescript"
  echo "and ruff/parser-python-3) -- the engine must never hold syn ASTs directly."
  exit 1
fi

echo "syn isolation guard: clean."
