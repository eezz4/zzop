#!/usr/bin/env bash
# release-version propagation guard — every COMMITTED version string on the release surface must equal
# `Cargo.toml`'s `[workspace.package] version`, which is this repo's single version SSOT.
#
# WHY THIS EXISTS (2026-07-29, paid for once). v0.26.0 was audited, tagged and pushed with `server.json`
# still declaring 0.25.0 in six places. Two CI jobs caught it — `prebuild`'s `meta` job on main and its
# `verify-plugin-version` job on the tag — and both run AFTER the push, so the published tag was already
# immovable (working-agreements §6.2) by the time anything said a word. The local gate fleet could not
# see it: nothing before this file compared a committed version against the SSOT, and the pre-tag audit
# checklist (§6.5) has no version-propagation lens. A check that only exists downstream of an irreversible
# step is not a guard for that step.
#
# This is checkable on EVERY commit, not just release commits, which is what makes it a pre-commit guard
# rather than a checklist item: `plugin.json` and `server.json` are wrong the instant they disagree with
# Cargo.toml, whatever the commit is about. The bump and its propagation land together or the commit fails.
#
# ## Subject A — committed `"version": "<semver>"` strings in tracked JSON
# Discovered, never listed. A hand list of "files carrying the version" is the same second-copy-of-a-fact
# this repo keeps paying for; a new manifest would join the release surface without joining the list, and
# the guard would vouch for it by never looking. Two exclusions, both principled:
#   * `0.0.0` — the publish-time PLACEHOLDER. `.github/workflows/prebuild.yml` rewrites every
#     `packages/cli/**/package.json` and `packages/mcpb/manifest.json` version at build time, so their
#     committed value is deliberately not a version and must not be made to track one.
#   * `examples/` — sample packages a user copies into their own tree (`adapter-kit` is at 0.1.0). Their
#     version is their own artifact's, unrelated to zzop's release.
# Integer `"version": 1` fields (the adapter envelope's schema version) are not semver strings and never
# match the extraction — a different fact with a different lifecycle, correctly invisible here.
#
# ## Subject B — `releases/download/v<semver>/` asset URLs in tracked files
# `server.json`'s five `packages[].identifier` URLs carry the version a SECOND time, in the path. Bumping
# the `version` fields and leaving the URLs behind points the MCP registry at the previous release's
# assets — schema-valid, hash-checkable, and wrong. `.github/` is out of scope: prebuild.yml builds these
# URLs from `$VERSION` at runtime and its only literal `v0.25.0` is prose in a comment about a past release.
#
# Both subjects fail closed on an EMPTY set: an extraction that stops matching would otherwise report a
# clean tree while reading nothing, which is this repo's own twice-paid false-green class.
#
# No deps beyond git + sed + grep + awk.
set -euo pipefail
cd "$(dirname "$0")/.."

SELF=check-release-version-propagation
PLACEHOLDER=0.0.0

abort() { echo "$SELF: $*" >&2; exit 1; }

# --- The SSOT -------------------------------------------------------------------------------------
# Section-aware: `version = ` also appears under `[workspace.dependencies]`, so a bare grep would read
# whichever one came first. Only `[workspace.package]`'s counts.
VERSION="$(awk '
  /^\[/ { in_wp = ($0 ~ /^\[workspace\.package\]/); next }
  in_wp && /^[[:space:]]*version[[:space:]]*=/ {
    v = $0; sub(/^[^=]*=[[:space:]]*"/, "", v); sub(/".*$/, "", v)
    print v; exit
  }
' Cargo.toml)"

[ -n "$VERSION" ] || abort \
  "could not read [workspace.package] version out of Cargo.toml -- the version SSOT moved or was
  reshaped, so every comparison below would be against an empty string and pass vacuously. Re-point
  this extraction at the new spelling; do not delete the check."

violations=0

# --- Subject A ------------------------------------------------------------------------------------
# `git ls-files` output, filtered rather than pathspec-excluded, so the exclusion reads next to its
# reason. `grep -H` forces the filename prefix even when xargs hands grep a single path.
json_lines="$(git ls-files -z -- '*.json' \
  | xargs -0 -r grep -Hn '"version"[[:space:]]*:[[:space:]]*"[0-9][0-9.]*[0-9A-Za-z.+-]*"' -- 2>/dev/null \
  | grep -v '^examples/' \
  || true)"

a_count="$(printf '%s\n' "$json_lines" | grep -c . || true)"
[ "$a_count" -gt 0 ] || abort \
  "extracted 0 committed JSON version strings -- the release manifests were renamed, reshaped, or the
  extraction broke. Every check in subject A would vacuously pass. Fix the extraction rather than
  trusting a green run."

while IFS= read -r line; do
  [ -n "$line" ] || continue
  where="${line%%:*}"
  rest="${line#*:}"
  lineno="${rest%%:*}"
  v="$(printf '%s' "$line" | sed 's/.*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/')"
  [ "$v" = "$PLACEHOLDER" ] && continue
  if [ "$v" != "$VERSION" ]; then
    echo "$SELF: $where:$lineno declares version \"$v\"; the workspace version is \"$VERSION\"." >&2
    violations=$((violations + 1))
  fi
done <<< "$json_lines"

# --- Subject B ------------------------------------------------------------------------------------
url_lines="$(git grep -n 'releases/download/v[0-9][0-9.]*' -- . 2>/dev/null \
  | grep -v '^\.github/' \
  || true)"

b_count="$(printf '%s\n' "$url_lines" | grep -c . || true)"
[ "$b_count" -gt 0 ] || abort \
  "found 0 'releases/download/v<version>' asset URLs outside .github/ -- server.json's packages[]
  identifiers were reshaped or renamed and subject B now proves nothing. Fix the extraction rather
  than trusting a green run."

while IFS= read -r line; do
  [ -n "$line" ] || continue
  where="${line%%:*}"
  rest="${line#*:}"
  lineno="${rest%%:*}"
  # One line can carry only one asset URL in this tree, but read them all rather than assuming it.
  for u in $(printf '%s' "$line" | grep -o 'releases/download/v[0-9][0-9.]*[0-9A-Za-z.+-]*' | sed 's#.*/v##'); do
    if [ "$u" != "$VERSION" ]; then
      echo "$SELF: $where:$lineno points at release assets for v$u; the workspace version is v$VERSION." >&2
      violations=$((violations + 1))
    fi
  done
done <<< "$url_lines"

if [ "$violations" -ne 0 ]; then
  echo >&2
  echo "  Cargo.toml's [workspace.package] version is the SSOT. Every committed version on the release" >&2
  echo "  surface tracks it in the SAME commit -- .claude-plugin/plugin.json's \"version\", server.json's" >&2
  echo "  top-level \"version\", each packages[].version, and each packages[].identifier download URL." >&2
  echo >&2
  echo "  A version left behind is not caught anywhere upstream of a push: the jobs that notice it" >&2
  echo "  (prebuild's 'meta' and 'verify-plugin-version') run after the tag exists, and a published tag" >&2
  echo "  never moves. That is why this runs on every commit." >&2
  echo >&2
  echo "  The publish-time placeholder \"$PLACEHOLDER\" is exempt (prebuild.yml rewrites those at build" >&2
  echo "  time); so is anything under examples/, whose versions are the sample packages' own." >&2
  exit 1
fi

echo "$SELF: clean (workspace $VERSION; $a_count committed JSON version string(s), $b_count asset URL line(s) checked)."
