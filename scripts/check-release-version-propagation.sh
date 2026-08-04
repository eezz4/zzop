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
# the guard would vouch for it by never looking. Every exclusion below is a claim that the field is not
# the release version at all — never that checking it would be inconvenient (the count is not stated:
# it said "Two" over four bullets until 2026-08-01, the same stale-count class §5.5 records):
#   * `0.0.0` — the publish-time PLACEHOLDER. `.github/workflows/prebuild.yml` rewrites every
#     `packages/cli/**/package.json` and `packages/mcpb/manifest.json` version at build time, so their
#     committed value is deliberately not a version and must not be made to track one.
#   * `examples/` — sample packages a user copies into their own tree (`adapter-kit` is at 0.1.0). Their
#     version is their own artifact's, unrelated to zzop's release.
#   * Normalized-AST ENVELOPES, recognised by their own bytes (subject A0 below) — their `"version"`
#     is a DIFFERENT FACT on a different schedule. `zzop_core::NORMALIZED_AST_CONTRACT_VERSION`'s rule
#     (2026-07-31 user ruling) is that it moves only when the envelope SHAPE moves, explicitly NOT
#     every release: "bumping it every release would be the defect this replaces — a number that
#     appears to describe the shape while actually describing the calendar." This guard bumped
#     `docs/contracts/example-envelope.json` to 0.28.0 anyway (caught in that release's pre-tag audit),
#     which would have had adapter authors copying an example that every 0.27.x engine rejects for an
#     unchanged shape. Semver-shaped is not the same fact as release-tracking. Envelopes are not left
#     unguarded — they move to the CONTRACT-version axis, bound by
#     `crates/engine/tests/rule_contracts/envelope_contract_version.rs`.
#   * npm lockfiles (`package-lock.json`, any depth) — every `"version"` in one is a THIRD PARTY's
#     version (playwright's, its transitive deps'), pinned on purpose and never meant to track this
#     repo's release. First hit: scripts/site-render-check/ (2026-07-30). The *manifests* beside them
#     stay in scope — a `package.json` whose `version` field is ours must still propagate — but a
#     dependency pin like `"playwright": "1.54.1"` never matches the extraction anyway (the key is not
#     `version`), so manifests carrying only pins are naturally invisible.
# Integer `"version": 1` fields (the adapter envelope's schema version) are not semver strings and never
# match the extraction — a different fact with a different lifecycle, correctly invisible here.
#
# ## Subject A0 — which files are envelopes, answered by CONTENT
# An envelope is any tracked JSON whose bytes declare `"format": "zzop-normalized-ast"`, which is the
# envelope contract's own self-identification (`zzop_core::NormalizedEnvelope::format`, and what
# `validate_envelope` reads to decide the same question at runtime). Deliberately NOT a path list: this
# guard already carried one (`docs/contracts/example-envelope.json`, by name) while `cases/`'s two
# envelopes sat in subject A being forced to declare the RELEASE version — two envelopes under two rules,
# green only because the two numbers happened to be equal. The moment the contract legitimately lags the
# release, which is its normal state, a path list would have made those files declare a contract version
# that does not exist. A content test classifies correctly wherever the file sits, and adding an envelope
# anywhere in the tree needs no edit here (working-agreements §5.5 — the hand-maintained subject set is
# the recurring cost).
#
# ## Subject B — `releases/download/v<semver>/` asset URLs in tracked files
# `server.json`'s five `packages[].identifier` URLs carry the version a SECOND time, in the path. Bumping
# the `version` fields and leaving the URLs behind points the MCP registry at the previous release's
# assets — schema-valid, hash-checkable, and wrong. `.github/` is out of scope: prebuild.yml builds these
# URLs from `$VERSION` at runtime and its only literal `v0.25.0` is prose in a comment about a past release.
#
# Every subject fails closed on an EMPTY set, A0 included: an extraction that stops matching would
# otherwise report a clean tree while reading nothing, which is this repo's own twice-paid false-green
# class. A0's floor guards the opposite direction from A's and B's — if the envelope classifier stopped
# matching, the envelopes would silently rejoin subject A and be dragged onto the release calendar again.
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

# --- Subject A0: classify the envelopes by content ------------------------------------------------
# Their `"version"` belongs to the CONTRACT axis, not this one — see the header. Nothing about the path
# is consulted; the file says what it is.
envelope_files="$(git ls-files -z -- '*.json' \
  | xargs -0 -r grep -l '"format"[[:space:]]*:[[:space:]]*"zzop-normalized-ast"' -- 2>/dev/null \
  || true)"

env_count="$(printf '%s\n' "$envelope_files" | grep -c . || true)"
[ "$env_count" -gt 0 ] || abort \
  "classified 0 Normalized-AST envelopes -- the '\"format\": \"zzop-normalized-ast\"' self-identification
  was reshaped or every envelope left the tree. With no envelope set, every envelope in this repo falls
  back into subject A and gets forced onto the RELEASE version, which is the exact defect this
  classification removes. Fix the extraction rather than trusting a green run."

# --- Subject A ------------------------------------------------------------------------------------
# `git ls-files` output, filtered rather than pathspec-excluded, so the exclusion reads next to its
# reason. `grep -H` forces the filename prefix even when xargs hands grep a single path.
#
# The envelope hold-out is the `awk` stage: the classified paths are read as a lookup table and the
# `<path>` field of each `<path>:<lineno>:<text>` line is compared to it as a WHOLE STRING. Not a regex
# — a path spliced into one would need every ERE metacharacter escaped, and a guard that silently
# mis-escapes one path stops holding that envelope out while still printing a clean count. `NR==FNR`
# is safe here only because the A0 floor above already refused an empty list.
json_lines="$(git ls-files -z -- '*.json' \
  | xargs -0 -r grep -Hn '"version"[[:space:]]*:[[:space:]]*"[0-9][0-9.]*[0-9A-Za-z.+-]*"' -- 2>/dev/null \
  | grep -v '^examples/' \
  | grep -v 'package-lock\.json:' \
  | awk -F: 'NR == FNR { if (NF) envelope[$0] = 1; next } !($1 in envelope)' \
      <(printf '%s\n' "$envelope_files") - \
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
  echo "  time); so is anything under examples/, whose versions are the sample packages' own; so is every" >&2
  echo "  Normalized-AST envelope, whose \"version\" is the envelope SHAPE contract and is bound instead by" >&2
  echo "  crates/engine/tests/rule_contracts/envelope_contract_version.rs." >&2
  exit 1
fi

echo "$SELF: clean (workspace $VERSION; $a_count committed JSON version string(s), $b_count asset URL line(s) checked; $env_count envelope(s) held out to the contract axis)."
