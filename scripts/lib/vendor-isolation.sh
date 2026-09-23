#!/usr/bin/env bash
# VENDOR ISOLATION, once — the body three guards were carrying a copy of each.
#
# The architecture guarantee is one sentence repeated per frontend: a third-party parser's types never
# leave the crate that projects them into the Common IR (`crates/core/src/lib.rs`'s module doc, "swc /
# external-parser types never leak in"). Three guards enforce it for three vendors, and a fourth —
# `check-tree-sitter-isolation.sh` — enforces it for the shared grammar runtime across several crates.
#
# 🔴 The three single-crate ones were the SAME SCRIPT with four values changed (2026-09-13, external
# review round 18, ledger V201). Measured after stripping comments and blank lines: swc against ruff
# differed in 10 of 46 code lines, swc against syn in 14 — and every one of those differences was a
# vendor token, an allowed path, or a message naming the vendor. Three copies of one body is three
# places a fix has to land, and the copies had already drifted in their prose.
#
# ## What a caller supplies, and nothing else
#   $1  label          the guard's own name in every message ("swc isolation guard")
#   $2  vendor         the vendor's name as PROSE, for the two violation headings ("swc", "syn")
#   $3  vendor_re      a regex fragment matching the vendor's CRATE NAMES, e.g. `swc[_-][A-Za-z0-9_-]*`
#                      or `syn|proc-macro2`. The declaration FORMS are composed from it below.
#   $4  use_pattern    how it appears in .rs source
#   $5  closing        the sentence printed after a violation, which says WHERE the vendor must stay
#                      and why. Per-vendor because the reason differs (an swc upgrade's re-verification
#                      scope, a grammar runtime's per-language crate, ...).
#   $6+ allowlist      one or more crate directories, e.g. `parser/parser-typescript`. Plural from the
#                      start: `check-tree-sitter-isolation.sh` already needs three, and writing the
#                      single-crate case as a special case is how the fourth guard ends up outside this
#                      file again.
#
# 🔴 BOTH PATTERNS ARE PARAMETERS, and the first draft of this file got that wrong. Deriving them from a
# vendor token (`${vendor}[_-]...`) fits swc and ruff and is FALSE for syn, whose crates are `syn` and
# `proc-macro2` — bare names, no suffix, two of them. A derivation that fits two of three vendors would
# have quietly changed what the syn guard scans while every test stayed green, which is the shape this
# repo keeps paying for: the shared thing here is the MECHANISM (floors, enumeration, allowlist
# subtraction, reporting), and the patterns are per-vendor knowledge that only looks shared.
#
# ## What stays with each caller
# Its header — the guarantee it enforces, what it measured, and the incidents that shaped it. That is
# knowledge about a vendor, not about the mechanism, and folding it here would put four vendors' history
# in one file nobody reads per-vendor.
#
# ## The floors, and why they are here rather than in the callers
# Both scans take their file list from a git pathspec, and a pathspec that stops matching (a crate
# moved, a glob typo) returns the SAME empty string a genuinely clean tree returns — so the guard prints
# "clean." having read nothing. That class was measured in this repo on 2026-07-29 and is the reason the
# floor exists at all; keeping it beside the scan means a caller cannot forget it, which is exactly what
# three hand-copied bodies made possible.

# shellcheck source=scripts/lib/tracked-grep.sh
. "$(dirname "${BASH_SOURCE[0]}")/tracked-grep.sh"

# Runs both checks for one vendor. Returns 0 clean, 1 on any violation; prints its own verdict line.
run_vendor_isolation() {
  local label="$1" vendor="$2" vendor_re="$3" use_pattern="$4" closing="$5"
  shift 5
  local allowlist=("$@")
  if [ "${#allowlist[@]}" -eq 0 ]; then
    echo "$label: FAILED -- no allowlisted crate was passed, so every use of $vendor would be a" >&2
    echo "  violation and the first run would fail for the wrong reason." >&2
    return 1
  fi

  local violations=0
  # The glob arrays are the SINGLE owner of each scan's scope: the count and the scan below read the
  # same array, so widening a scope cannot leave its assertion behind on the old one.
  local CARGO_GLOBS=('Cargo.toml' '*/Cargo.toml')
  local RS_GLOBS=('*.rs')

  local cargo_scanned rs_scanned
  cargo_scanned="$(git ls-files -- "${CARGO_GLOBS[@]}" | grep -c . || true)"
  rs_scanned="$(git ls-files -- "${RS_GLOBS[@]}" | grep -c . || true)"
  if [ "$cargo_scanned" -eq 0 ] || [ "$rs_scanned" -eq 0 ]; then
    echo "$label: FAILED -- enumerated $cargo_scanned Cargo.toml file(s) and $rs_scanned .rs file(s)."
    echo "A zero on either axis means that pathspec matched nothing, so this run proved nothing about"
    echo "where $vendor is used. An empty subject set is a broken guard, never a clean tree."
    return 1
  fi
  # BOTH axes, not just the .rs one. Every workspace member has a Cargo.toml, so the same derived
  # floor applies to the Cargo scan — and without it a narrowed CARGO_GLOBS prints "clean (1 Cargo.toml
  # + 1425 .rs files scanned)" with the 1 in plain sight. Measured that way on 2026-09-13 (ledger V205)
  # by an external review, ONE FILE over from this repo's own header calling that shape "a floor that
  # only catches ZERO" — written the same day, by the same hand, for the shell population.
  assert_workspace_members_scanned "$label" "${RS_GLOBS[@]}"
  assert_workspace_members_scanned "$label" "${CARGO_GLOBS[@]}"

  # ## The FORMS a Cargo dependency can take — workspace knowledge, owned here
  # The caller supplies only the NAME (`vendor_re`); the shapes it can be written in are a property of
  # Cargo and of this workspace's house style, identical for every vendor. Splitting it that way is
  # what stops a blind spot from living in one guard: before 2026-09-13 each caller hand-wrote a whole
  # pattern and all three had the SAME holes.
  #
  # 🔴 The holes were measured by an external review (ledger V204) with a manifest that `cargo metadata`
  # accepts: a root `[workspace.dependencies]` pin plus `ruff_text_size.workspace = true` in
  # `crates/engine`, plus real use in `crates/engine/src/` — and ALL FOUR isolation guards printed
  # "clean". This workspace declares 60+ dependency lines in the `.workspace = true` form, so the one
  # spelling the guards could not see was the one the repo actually uses.
  #
  # `check-dep-closure.sh` does not cover the gap either: `zzop-engine` already reaches that crate
  # transitively, and a direct edge does not move a reachability census.
  local dep_pattern
  dep_pattern="$(printf '%s' \
    "^(?!\s*#)\s*(${vendor_re})\s*=" \
    "|^(?!\s*#)\s*(${vendor_re})\.workspace\s*=" \
    "|^(?!\s*#)\s*\[[a-z.-]*dependencies\.(${vendor_re})\]" \
    "|^(?!\s*#).*package\s*=\s*\"(${vendor_re})\"")"

  echo "$label: checking Cargo.toml dependency declarations..."
  # The enumeration call is kept OUTSIDE the `|| true` below on purpose: tracked_files_matching's own
  # failure must still trip `set -e` and abort loud (see its header); only the allowlisted
  # false-positives are safe to swallow.
  local cargo_matches cargo_files crate
  cargo_matches=$(tracked_files_matching "$dep_pattern" "${CARGO_GLOBS[@]}")
  # The ROOT manifest is scanned like any other. It used to be dropped by name, on the grounds that it
  # is "not itself a dependency declaration site" — but `[workspace.dependencies]` is exactly that, and
  # it is where the review's planted pin lived. Its own comment says the pin belongs in one crate
  # (`Cargo.toml`, "swc version isolation"), so a vendor appearing there is a violation, not an
  # exemption. Vendor names in that file today appear only inside `#` comments, which the negative
  # lookahead above excludes.
  cargo_files="$cargo_matches"
  for crate in "${allowlist[@]}"; do
    cargo_files=$(printf '%s\n' "$cargo_files" | grep -v -x "$crate/Cargo.toml" || true)
  done

  local f
  if [ -n "$cargo_files" ]; then
    echo "$label: $vendor dependency declared outside ${allowlist[*]}:"
    while IFS= read -r f; do
      [ -n "$f" ] || continue
      grep -nP "$dep_pattern" "$f" | sed "s|^|  ${f#./}:|"
    done < <(printf '%s\n' "$cargo_files")
    violations=1
  fi

  echo "$label: checking .rs source usage..."
  local rs_matches rs_files
  rs_matches=$(tracked_files_matching "$use_pattern" "${RS_GLOBS[@]}")
  rs_files="$rs_matches"
  for crate in "${allowlist[@]}"; do
    rs_files=$(printf '%s\n' "$rs_files" | grep -v "^$crate/src/" || true)
  done

  if [ -n "$rs_files" ]; then
    echo "$label: $vendor usage found outside ${allowlist[*]}/src:"
    while IFS= read -r f; do
      [ -n "$f" ] || continue
      grep -nP "$use_pattern" "$f" | sed "s|^|  ${f#./}:|"
    done < <(printf '%s\n' "$rs_files")
    violations=1
  fi

  if [ "$violations" -ne 0 ]; then
    echo
    echo "$closing"
    return 1
  fi

  echo "$label: clean ($cargo_scanned Cargo.toml + $rs_scanned .rs files scanned)."
  return 0
}
