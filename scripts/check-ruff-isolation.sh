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
# Enumeration mechanism (TRACKED-file discovery + grep + the standard target/node_modules/.claude
# exclusions) lives in scripts/lib/tracked-grep.sh, shared with check-syn-isolation.sh /
# check-tree-sitter-isolation.sh / check-swc-isolation.sh — this script keeps only ITS OWN pattern,
# allowlist, and messages.
#
# No deps beyond git + grep -P (PCRE). Exit 1 on any violation, listing file:line.
set -euo pipefail
cd "$(dirname "$0")/.."

# The MECHANISM -- both floors, the tracked-file enumeration, the allowlist subtraction and the
# reporting -- is scripts/lib/vendor-isolation.sh, shared with the other isolation guards. This file
# keeps what is this vendor's alone: the guarantee above, the two patterns, and the closing sentence.
. ./scripts/lib/vendor-isolation.sh

run_vendor_isolation "ruff isolation guard" "ruff_*" \
  'ruff[_-][A-Za-z0-9_-]*' \
  '\bruff_[a-z0-9_]+::|use\s+ruff_|extern\s+crate\s+ruff[_-]' \
  'ruff must stay confined to parser/parser-python-3 -- the engine must never hold ruff ASTs
directly.' \
  "parser/parser-python-3"
