#!/usr/bin/env bash
# swc isolation guard — fails when an swc_* crate is depended on outside parser/parser-typescript,
# or swc_core is used in any .rs outside parser/parser-typescript/src/.
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
#     (the latter is where the swc isolation note lives — exempted even though it does not
#     currently declare swc as an actual [workspace.dependencies] entry; the version pin itself
#     lives in parser/parser-typescript/Cargo.toml).
#  2. `swc_core::` or `use swc_...` in any .rs file outside parser/parser-typescript/src/.
#
# Scope: git-TRACKED files only (git ls-files). The working tree also holds gitignored/untracked
# local corpora (cloned third-party repos, benchmark checkouts) whose own swc usage is not ours to
# police — a `grep -r .` over the tree false-positives on them. Anything that could ship must be
# tracked, so tracked-only is exactly the isolation surface (and matches what CI checks out).
#
# Enumeration mechanism (TRACKED-file discovery + grep + the standard target/node_modules/.claude
# exclusions) lives in scripts/lib/tracked-grep.sh, shared with check-syn-isolation.sh /
# check-tree-sitter-isolation.sh / check-ruff-isolation.sh — this script keeps only ITS OWN pattern,
# allowlist, and messages.
#
# No deps beyond git + grep -P (PCRE). Exit 1 on any violation, listing file:line.
set -euo pipefail
cd "$(dirname "$0")/.."

# The MECHANISM -- both floors, the tracked-file enumeration, the allowlist subtraction and the
# reporting -- is scripts/lib/vendor-isolation.sh, shared with the other isolation guards. This file
# keeps what is this vendor's alone: the guarantee above, the two patterns, and the closing sentence.
. ./scripts/lib/vendor-isolation.sh

run_vendor_isolation "swc isolation guard" "swc_*" \
  'swc[_-][A-Za-z0-9_-]*' \
  '\bswc_[a-z0-9_]+::|use\s+swc_|extern\s+crate\s+swc[_-]' \
  'swc must stay confined to parser/parser-typescript (see crates/core/src/lib.rs and the workspace
root Cargo.toml'"'"'s "swc version isolation" note) -- the engine must never hold swc ASTs directly.' \
  "parser/parser-typescript"
