#!/usr/bin/env bash
# Guards against a silently stale zzop-cache: every parser crate under parser/*/ that defines a
# PARSER_FINGERPRINT const bakes its extraction-shape version into the cache key (see each crate's
# own PARSER_FINGERPRINT doc comment for the scheme). If a change touches that crate's src/** but
# never touches the fingerprint's own line, an old cache entry keyed on the unbumped fingerprint
# would keep being served as "still valid" even though what the crate extracts has changed.
#
# Escape hatch: a commit message in the diff range containing `[no-projection-change: <crate-dir>]`
# (e.g. `[no-projection-change: parser-java]`) skips that crate — for changes that provably do not
# alter extraction output (docs, comments, internal refactors with identical results).
#
# Diff range: ${FINGERPRINT_DIFF_RANGE:-origin/main...HEAD}, overridable via env. CI computes this
# against the PR base (or the previous commit on a direct push) — see .github/workflows/ci.yml.
# Local runs commonly lack a fetched origin/main; that degrades gracefully (skip with a notice,
# exit 0) rather than failing a guard the developer has no way to satisfy.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

range="${FINGERPRINT_DIFF_RANGE:-origin/main...HEAD}"

# Pull the left side out of "A...B" or "A..B" so we can check it resolves before trusting the range.
base_ref="${range%%...*}"
base_ref="${base_ref%%..*}"

if ! git rev-parse --verify --quiet "${base_ref}^{commit}" >/dev/null; then
  echo "check-parser-fingerprint-bump: notice — '$base_ref' does not resolve locally (no fetched origin/main?); skipping."
  exit 0
fi

if ! changed_files="$(git diff --name-only "$range" -- 2>&1)"; then
  echo "check-parser-fingerprint-bump: notice — could not diff range '$range':"
  echo "  $changed_files"
  echo "  skipping."
  exit 0
fi

commit_messages="$(git log --format=%B "$range" -- 2>/dev/null || true)"

fail=0
for crate_dir in parser/*/; do
  crate="${crate_dir%/}"
  crate_name="$(basename "$crate")"
  [ -d "$crate/src" ] || continue

  fp_file="$(grep -rlE '^[[:space:]]*pub const PARSER_FINGERPRINT' "$crate/src" 2>/dev/null | head -n1 || true)"
  [ -z "$fp_file" ] && continue

  crate_changed="$(printf '%s\n' "$changed_files" | grep -F "$crate/src/" || true)"
  [ -z "$crate_changed" ] && continue

  if printf '%s\n' "$commit_messages" | grep -qF "[no-projection-change: $crate_name]"; then
    echo "check-parser-fingerprint-bump: $crate_name — src/** changed but skipped via [no-projection-change: $crate_name] marker."
    continue
  fi

  fp_diff="$(git diff -U0 "$range" -- "$fp_file" 2>/dev/null | grep -E '^[+-][[:space:]]*pub const PARSER_FINGERPRINT' || true)"
  if [ -z "$fp_diff" ]; then
    echo "check-parser-fingerprint-bump: $crate_name — src/** changed in $range but PARSER_FINGERPRINT (in $fp_file) was not bumped." >&2
    echo "  Stale-cache risk: zzop-cache keys cached analysis results by this fingerprint; an unbumped fingerprint" >&2
    echo "  means a change to what/how this crate extracts could keep being served from a stale cache entry." >&2
    echo "  Fix: bump PARSER_FINGERPRINT (e.g. append a new '+label-vN' segment, or bump an existing segment's version)." >&2
    echo "  Escape hatch: if this change provably does not alter extraction output, add '[no-projection-change: $crate_name]'" >&2
    echo "  to a commit message in the range." >&2
    fail=1
  fi
done

if [ "$fail" -ne 0 ]; then
  echo "check-parser-fingerprint-bump: FAILED." >&2
  exit 1
fi

echo "check-parser-fingerprint-bump: OK (checked range $range)"
