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
# Scope: git-TRACKED files only (git ls-files), for the same reason as check-swc-isolation.sh /
# check-ruff-isolation.sh — the working tree also holds gitignored/untracked local corpora (cloned
# third-party repos, benchmark checkouts) whose own syn usage is not ours to police, and `syn::` is
# ubiquitous in real Rust crates (a `grep -r .` over the tree false-positives on every one of them).
# Anything that could ship must be tracked, so tracked-only is exactly the isolation surface (and
# matches what CI checks out).
#
# Enumeration mechanism (TRACKED-file discovery + grep + the standard target/node_modules/.claude
# exclusions) lives in scripts/lib/tracked-grep.sh, shared with check-tree-sitter-isolation.sh /
# check-swc-isolation.sh / check-ruff-isolation.sh — this script keeps only ITS OWN pattern,
# allowlist, and messages.
#
# No deps beyond git + grep -P (PCRE). Exit 1 on any violation, listing file:line.
set -euo pipefail
cd "$(dirname "$0")/.."

# The MECHANISM -- both floors, the tracked-file enumeration, the allowlist subtraction and the
# reporting -- is scripts/lib/vendor-isolation.sh, shared with the other isolation guards. This file
# keeps what is this vendor's alone: the guarantee above, the two patterns, and the closing sentence.
. ./scripts/lib/vendor-isolation.sh

run_vendor_isolation "syn isolation guard" "syn/proc-macro2" \
  'syn|proc-macro2' \
  '\bsyn::[A-Za-z_]|use\s+syn(::|;|\s)|extern\s+crate\s+syn|\bproc_macro2::|use\s+proc_macro2' \
  'syn must stay confined to parser/parser-rust -- the engine must never hold syn ASTs directly.' \
  "parser/parser-rust"
