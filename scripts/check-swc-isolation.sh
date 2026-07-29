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
. ./scripts/lib/tracked-grep.sh

violations=0

# ## Subject-set floor (2026-07-29)
# Both scans below take their file list from a git pathspec, and a pathspec that stops matching (a
# crate moved, a glob typo, a rename) makes tracked_files_matching return the SAME empty string it
# returns on a genuinely clean tree — so without this, "clean." is printed identically whether 24
# Cargo.toml files and 912 .rs files were read or zero were. Measured 2026-07-29 by redirecting both
# pathspecs in a scratch copy: it printed "swc isolation guard: clean." and exited 0. That is the
# repo's own twice-paid class (a scan root pointing at nothing, green while reading nothing), and the
# sibling of the "a guard reads its own copy" class removed from sixteen guards on 2026-07-28.
#
# The glob arrays are the SINGLE owner of each scan's scope: the count and the scan below read the
# same array, so widening a scope cannot leave its assertion behind on the old one.
CARGO_GLOBS=('Cargo.toml' '*/Cargo.toml')
RS_GLOBS=('*.rs')
cargo_scanned="$(git ls-files -- "${CARGO_GLOBS[@]}" | grep -c . || true)"
rs_scanned="$(git ls-files -- "${RS_GLOBS[@]}" | grep -c . || true)"
if [ "$cargo_scanned" -eq 0 ] || [ "$rs_scanned" -eq 0 ]; then
  echo "swc isolation guard: FAILED -- enumerated $cargo_scanned Cargo.toml file(s) and $rs_scanned .rs"
  echo "file(s). A zero on either axis means that pathspec matched nothing, so this run proved nothing"
  echo "about where swc is used. An empty subject set is a broken guard, never a clean tree."
  exit 1
fi

echo "swc isolation guard: checking Cargo.toml dependency declarations..."
DEP_PATTERN='^\s*swc[_-][A-Za-z0-9_-]*\s*='
# The enumeration call is kept OUTSIDE the `|| true` below on purpose: tracked_files_matching's own
# failure must still trip `set -e` and abort loud (see its header comment); only its allowlisted
# false-positives (this guard's own Cargo.toml, parser-typescript's own) are safe to swallow via
# `|| true`.
cargo_matches=$(tracked_files_matching "$DEP_PATTERN" "${CARGO_GLOBS[@]}")
cargo_files=$(grep -v -x 'Cargo.toml' <<< "$cargo_matches" \
  | grep -v -x 'parser/parser-typescript/Cargo.toml' || true)

if [ -n "$cargo_files" ]; then
  echo "swc isolation guard: swc_* dependency declared outside parser-typescript:"
  while IFS= read -r f; do
    grep -nP "$DEP_PATTERN" "$f" | sed "s|^|  ${f#./}:|"
  done <<< "$cargo_files"
  violations=1
fi

echo "swc isolation guard: checking .rs source usage..."
USE_PATTERN='swc_core::|use\s+swc_'
rs_matches=$(tracked_files_matching "$USE_PATTERN" "${RS_GLOBS[@]}")
rs_files=$(grep -v '^parser/parser-typescript/src/' <<< "$rs_matches" || true)

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

echo "swc isolation guard: clean ($cargo_scanned Cargo.toml + $rs_scanned .rs files scanned)."
