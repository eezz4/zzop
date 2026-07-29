#!/usr/bin/env bash
# check-asset-name-prose.sh — guards against the "release-asset name drifted in prose" defect
# class: docs/**/*.md, site/*.html, and README.md repeat the literal names of the binaries the
# release workflow uploads (e.g. `zzop-cli-linux-x64-gnu`, `zzop-mcp-darwin-arm64.mcpb`), and
# those copies silently drift from what prebuild.yml actually builds/names once someone renames a
# platform or restructures the packaging steps. This class leaked twice in the recent site rework
# (2026-07 site rework) and nothing mechanical caught it before this guard.
#
# SSOT (parsed at run time, never hardcoded): .github/workflows/prebuild.yml's `build` job matrix.
# Each `include:` entry pairs a `platform:` value with an `os:` value; this script extracts both,
# derives the platform whose `os:` mentions "windows" (today `win32-x64-msvc`, from
# `os: windows-latest`), and reconstructs the valid asset-name set exactly the way the matrix's own
# "Collect the zzop + zzop-mcp binaries" step (bash `case "${{ matrix.target }}" in *windows*) ...`)
# and the `release` job's "Build per-platform MCPB bundles" step (`zzop-mcp-$plat.mcpb`, no
# platform ever gets a `.exe` .mcpb) name them:
#   - zzop-cli-<platform>[.exe]   (.exe iff <platform> is the windows one)
#   - zzop-mcp-<platform>[.exe]   (.exe iff <platform> is the windows one)
#   - zzop-mcp-<platform>.mcpb    (every platform, never .exe)
# Adding a platform to the matrix (or renaming one) changes what this script accepts on the very
# next run, with no edit here — the matrix block is re-parsed every invocation, never cached.
#
# Scan surface: TRACKED *.md and *.html files anywhere in the repo (README.md is a *.md file at the
# repo root, so it is included by the same glob; it is not special-cased). `.github/**` is
# deliberately EXCLUDED — every occurrence there is `${{ matrix.platform }}`/`${{ matrix.target }}`
# template interpolation or a shell case arm, not literal prose, and would be a pure false positive
# (a real drift there would already break the release itself, which is a different failure mode
# this guard does not need to also catch). `*.js`/`*.mjs`/`*.rs`/`*.json`/`*.toml` source files are
# excluded for the same reason: they build asset names from matrix vars or CLI args
# (packages/cli/scripts/place-artifacts.mjs, packages/cli/scripts/sync-versions.mjs) rather than
# spelling them out in prose.
#
# Rule: every token matching `zzop-(cli|mcp)-[A-Za-z0-9][A-Za-z0-9._-]*` in a scanned file must
# either be a member of the derived valid set, or fall into one of these allowed non-asset forms:
#   - literal placeholders `zzop-cli-<platform>` / `zzop-mcp-<platform>` — these need NO special
#     handling: the rule's own extraction regex requires an alphanumeric right after the second
#     dash, so `<` breaks the match before it starts. Every scanned file today (README.md,
#     VERSIONING.md, packages/README.md, docs/getting-started.md, docs/modules/mcp.md,
#     packages/cli/README.md, packages/mcpb/README.md, site/usage.html via its
#     `&lt;platform&gt;` entity encoding) uses exactly this placeholder form and is unaffected.
#   - glob forms ending in a literal `*` (e.g. `zzop-mcp-linux-*`) — unlike the placeholder case,
#     `-` IS in the extraction regex's continuation class, so a trailing glob star does not stop
#     the match on its own; this script therefore captures one optional trailing `*`
#     (`\*?`, tacked onto the rule regex only for this classification, not widening what a
#     "found token" is) and treats any token ending in `*` as an intentional wildcard, always
#     allowed. No scanned file uses this form today; the case is handled ahead of need because the
#     spec calls it out explicitly.
#   - the Cargo package name `zzop-cli-bin` (packages/cli-bin, built via `cargo build -p
#     zzop-cli-bin`) — this is the one real false positive the raw rule regex produces: it matches
#     `zzop-cli-` + `[A-Za-z0-9]` = `bin`, but `zzop-cli-bin` is a *crate name*, never a release
#     asset name (the asset is `zzop-cli-<platform>[.exe]`, built FROM that crate). It appears this
#     way in CONTRIBUTING.md, packages/README.md, docs/modules/mcp.md,
#     examples/adapters/adapter-kit/README.md, packages/cli/README.md, and README.md — always after
#     `-p ` or inside backticks naming the package, never as a downloadable file. Allowlisted by
#     exact string match; a different, genuinely wrong asset-shaped token must still fail.
# Any other token is a hard failure: report the file, line, and wrong token rather than silently
# widening this list — a real prose typo (e.g. `zzop-cli-linux-x64-musl`, a platform the matrix
# does not build) is exactly what this guard exists to catch, not hide.
#
# Known-uncovered shapes (documented, not silently ignored):
#   - an asset name split across a Markdown line wrap or an HTML tag boundary (e.g.
#     `zzop-cli-<code>linux-x64-gnu</code>`) — no scanned surface writes asset names that way today;
#     covering it needs a real parser, not grep.
#   - `.mcpb`/binary names embedded in a code fence as a shell variable expansion
#     (`zzop-mcp-$plat.mcpb`, as prebuild.yml itself writes it) — that shape is `.github/`-only
#     today and out of scope by design (see "Scan surface" above); if a doc ever quotes that exact
#     workflow snippet verbatim, `$plat` breaks the extraction regex the same way `<platform>` does
#     (`$` is not in the continuation class), so it is naturally skipped, not silently mis-scanned.
set -euo pipefail
cd "$(dirname "$0")/.."

# shellcheck source=scripts/lib/tracked-grep.sh
. ./scripts/lib/tracked-grep.sh

PREBUILD=.github/workflows/prebuild.yml
[ -f "$PREBUILD" ] || { echo "check-asset-name-prose: missing $PREBUILD" >&2; exit 1; }

# --- Derive the valid platform set + which platform is Windows, from prebuild.yml's build-job matrix ---
# Scoped tightly to the `matrix:` block (from the `matrix:` key through the line before `runs-on:`)
# so step bodies (which also mention `platform`/`target` as `${{ matrix.* }}` interpolations) are
# never accidentally parsed as matrix entries.
matrix_block="$(awk '
  /^  build:/ { inbuild = 1 }
  inbuild && /^      matrix:/ { active = 1; next }
  active && /^    runs-on:/ { exit }
  active { print }
' "$PREBUILD")"

[ -n "$matrix_block" ] || {
  echo "check-asset-name-prose: could not locate the build job's matrix block in $PREBUILD -- parse is broken" >&2
  exit 1
}

platforms=()
windows_platform=""
current_os=""
while IFS= read -r line; do
  case "$line" in
    *'- target:'*)
      current_os=""
      ;;
    *'os:'*)
      current_os="$(printf '%s' "$line" | sed -E 's/^[[:space:]]*os:[[:space:]]*//')"
      ;;
    *'platform:'*)
      plat="$(printf '%s' "$line" | sed -E 's/^[[:space:]]*platform:[[:space:]]*//')"
      platforms+=("$plat")
      case "$current_os" in
        *windows*) windows_platform="$plat" ;;
      esac
      ;;
  esac
done <<< "$matrix_block"

[ "${#platforms[@]}" -gt 0 ] || {
  echo "check-asset-name-prose: extracted 0 platforms from $PREBUILD's matrix -- parse is broken" >&2
  exit 1
}
[ -n "$windows_platform" ] || {
  echo "check-asset-name-prose: no matrix entry's os: mentions 'windows' -- cannot tell which platform gets .exe" >&2
  exit 1
}

valid_ids=()
for plat in "${platforms[@]}"; do
  if [ "$plat" = "$windows_platform" ]; then
    valid_ids+=("zzop-cli-$plat.exe" "zzop-mcp-$plat.exe")
  else
    valid_ids+=("zzop-cli-$plat" "zzop-mcp-$plat")
  fi
  valid_ids+=("zzop-mcp-$plat.mcpb")
done
valid_ids_joined="$(printf '%s\n' "${valid_ids[@]}")"

# The one real non-asset token the raw rule regex also matches — see header comment.
extra_allowlist="zzop-cli-bin"

is_valid_asset() { # $1 = candidate token (no trailing '*')
  grep -qxF "$1" <<< "$valid_ids_joined"
}

# --- Scan tracked *.md / *.html (README.md included via the *.md glob), excluding .github/** ------------
rule_pattern='zzop-(cli|mcp)-[A-Za-z0-9][A-Za-z0-9._-]*'
classify_pattern="${rule_pattern}\\*?"

candidate_files="$(tracked_files_matching "$rule_pattern" '*.md' '*.html' ':!:.github/**')" \
  || { echo "check-asset-name-prose: file enumeration failed" >&2; exit 1; }

# Total scanned SURFACE count for the closing line is every in-scope tracked file, not just the
# ones tracked_files_matching returned (which is only files containing >=1 rule-pattern hit) — a
# clean run should report how much ground it covered, not how much it found.
total_files="$(git ls-files -- '*.md' '*.html' ':!:.github/**' | grep -c . || true)"

# Subject-set floor (2026-07-29). The count above was already derived and PRINTED, but never compared
# to zero — so a pathspec that stopped matching produced the literal line "check-asset-name-prose:
# clean (0 files scanned)." and exit 0. Measured 2026-07-29 by redirecting the globs in a scratch copy.
# Printing a number is not asserting on it; the whole point of the count is that a green states how
# much ground was covered, and zero ground is a broken enumeration, never a clean tree.
if [ "$total_files" -eq 0 ]; then
  echo "check-asset-name-prose: FAILED -- enumerated ZERO tracked *.md/*.html files outside .github/**." >&2
  echo "  The scan surface is empty, so no prose was read and no asset name was checked against" >&2
  echo "  $PREBUILD's matrix. An empty subject set is a broken guard, never a clean tree." >&2
  exit 1
fi

fail=0

if [ -n "$candidate_files" ]; then
  while IFS= read -r file; do
    [ -n "$file" ] || continue

    matches="$(grep -noE "$classify_pattern" "$file" || true)"
    while IFS=: read -r lineno token; do
      [ -n "$lineno" ] || continue

      case "$token" in
        *'*')
          continue # glob form — always allowed, see header comment
          ;;
      esac

      # The continuation class above includes '.' and '-' (both are real characters INSIDE asset names:
      # `...-gnu`, `....mcpb`), so a name ending a sentence or a clause captures the punctuation into the
      # token -- `download zzop-cli-linux-x64-gnu.` would otherwise be reported as a nonexistent asset and
      # fail this guard on prose that is entirely correct. Strip TRAILING '.'/'-' only; no valid asset name
      # ends in either, so this can never mask a real drift.
      while [ "${token%[.-]}" != "$token" ]; do
        token="${token%[.-]}"
      done

      [ "$token" = "$extra_allowlist" ] && continue
      is_valid_asset "$token" && continue

      echo "check-asset-name-prose: $file:$lineno: token \"$token\" is not a release asset $PREBUILD builds" >&2
      echo "  (valid asset names: zzop-cli-<platform>[.exe] / zzop-mcp-<platform>[.exe] / zzop-mcp-<platform>.mcpb," >&2
      echo "   platforms: ${platforms[*]}, windows platform: $windows_platform)" >&2
      fail=1
    done <<< "$matches"
  done <<< "$candidate_files"
fi

if [ "$fail" -ne 0 ]; then
  echo "check-asset-name-prose: FAILED -- fix the offending token(s) above, or if the drift is real report it (do not widen this guard's allowlist to hide it)." >&2
  exit 1
fi

echo "check-asset-name-prose: clean ($total_files files scanned)."
