#!/usr/bin/env bash
# English-only source guard — fails when non-Latin letters (Hangul / CJK / Kana / Cyrillic)
# appear in OSS-facing files (Rust sources, READMEs, manifests, rule-pack JSON, and everything else
# this repo ships).
#
# Policy: code that ships is the explanation; comments and docs are English. Korean internal
# design notes are allowed ONLY under .claude/ (not published). This guard enforces that split.
#
# No deps beyond grep -P (PCRE). Exit 1 on any violation, listing file:line.
set -euo pipefail
cd "$(dirname "$0")/.."
. ./scripts/lib/tracked-grep.sh

# Non-Latin letter scripts. Punctuation / symbols (— · → ★) are intentionally NOT flagged.
FOREIGN='[\x{AC00}-\x{D7A3}\x{1100}-\x{11FF}\x{3130}-\x{318F}\x{3040}-\x{30FF}\x{4E00}-\x{9FFF}\x{0400}-\x{04FF}]'

# Scope = tracked files PLUS untracked-but-not-ignored ones: OSS-facing means "will ship in the
# repo". Untracked NEW files must be scanned too — a fresh file passes a tracked-only scan before
# its first `git add` and the violation lands in the commit (happened 2026-07-14 with a new test
# file). Ignored paths (local dogfood corpora of third-party repos with legitimate i18n text) stay
# out via scripts/lib/tracked-grep.sh's standard exclusions. CI runs on a clean checkout, where this
# reduces to tracked-only.
#
# Enumeration mechanism (TRACKED+untracked-not-ignored file discovery + grep + the standard
# target/node_modules/.claude exclusions, plus the PIPESTATUS-based producer-failure hardening) lives
# in scripts/lib/tracked-grep.sh's tracked_and_untracked_files_matching. check-max-file-lines.sh
# scans the same tracked+untracked surface for a different purpose (line counts, not a grep match)
# and is independently hardened via its own wc-based PIPESTATUS capture — it does not call this
# helper. This script keeps only its own scope, patterns, and messages.
#
# ## The scope is a DENY-list now, not an allow-list (2026-08-01)
#
# This was `SOURCE_GLOBS=('*.rs' '*.md' ... '.githooks/*')` — an extension ALLOW-list, i.e. a hand
# list of subjects, and it drifted the way every hand list of subjects drifts. Measured the day it
# changed: three of its entries (`*.cjs`, `*.jsx`, `*.yaml`) matched ZERO tracked files — they were
# added as future-proofing — while 22 tracked files matched no entry at all, including all SIX `.java`
# files (`cases/trees/java-svc/`, `examples/adapters/java-imports-adapter/`) and the one `.prisma`
# schema. Those are benchmark SOURCES this repo ships; the guard simply could not see them. The list
# had been patched twice before (`*.txt` and `.githooks/*` on 2026-08-01, after Korean rationale tails
# shipped inside the tracked-and-published scripts/policy-census.txt), which is the tell: an allow-list
# is repaired one incident at a time, and each repair is the announcement that it was wrong.
#
# So the subject is now DERIVED — `git ls-files --cached --others --exclude-standard -- '*'`, i.e.
# every file this repo ships — and anything that must stay out is SUBTRACTED below with a reason. A
# new source extension is covered on the day it lands rather than the day someone remembers this file.
#
# What this costs, stated rather than discovered later: a BINARY asset (none today — this repo ships
# no image, font or archive) would be grepped, and random bytes can match a UTF-8 letter range. That
# direction is fail-SAFE — a loud false red someone must look at, never a silent hole — which is why
# it is left to be handled by an exemption entry when it first happens, instead of pre-emptively
# carving out extensions that do not exist. Pre-emptive carve-outs are exactly what the allow-list was.
SUBJECT_PATHSPEC=('*')

# NON-SOURCE EXEMPTIONS — `<prefix>|<reason>`, subtracted from the derived subject above. A path is
# exempt when it starts with <prefix>. A reason is mandatory: an exemption without one is where a
# missing subject hides.
#
# EMPTY, and that is the measured state rather than an oversight: every one of the 1,388 files this
# repo ships is text, and all 1,388 are clean of both patterns below as of 2026-08-01.
NON_SOURCE_EXEMPTIONS=()

# The `.claude/`-reference scan's own exemption list, same shape and same rules. Kept separate from
# the list above because the two scans ask different questions and a path that must skip one has no
# reason to skip the other.
CLAUDE_REF_EXEMPTIONS=(
  "scripts/|guard machinery must NAME the very pattern it excludes — this file's own grep lines, and the max-file-lines / isolation scope filters. That is not a reader-facing \"see .claude/...\" pointer. The non-Latin scan above still covers scripts/, which is where the real risk in this directory lives."
)

# ## Both directions of every exemption (working-agreements 5.5)
#
# An exemption that matches nothing is not harmless: the file it excused may have been renamed or the
# reason may have expired, and a path returning under the same prefix is then already excused with
# nobody having re-read why. So a stale entry FAILS instead of quietly doing nothing.
assert_exemption_is_live() {
  local label="$1" prefix="$2" reason="$3" haystack="$4"
  if ! grep -q "^${prefix}" <<< "$haystack"; then
    echo "English-only source guard: FAILED -- $label exempts '${prefix}', which matches no path in" >&2
    echo "  the set it is subtracted from. Reason on file: $reason" >&2
    echo "  Delete the entry: an exemption that outlives its subject silently excuses whatever lands" >&2
    echo "  under that prefix next." >&2
    exit 1
  fi
}

# Is <path> covered by one of the `<prefix>|<reason>` entries in the named array?
is_exempt() {
  local path="$1"; shift
  local entry
  for entry in "$@"; do
    case "$path" in "${entry%%|*}"*) return 0 ;; esac
  done
  return 1
}

# ## Subject-set floor (2026-07-29, widened 2026-08-01)
#
# A helper that matched nothing returns exactly the same empty string whether the tree is clean or the
# pathspec stopped matching. This guard did not even derive a count once, so "clean." was printed
# identically for 1600 files and for zero. Measured 2026-07-29 by pointing the scope at a nonexistent
# extension in a scratch copy: it printed "English-only source guard: clean." / "Internal-path guard:
# clean." and exited 0.
#
# SUBJECT_PATHSPEC above is the single owner of the scope: the count and both scans read the same
# array, so widening it cannot leave the assertion behind. The count mirrors the helper's own
# tracked + untracked-but-not-ignored enumeration, and applies the helper's standard
# target/node_modules/.claude exclusions so the number printed is the number actually read.
#
# ONE `git ls-files`, not two, and the count is done in bash (2026-07-29). `--cached --others
# --exclude-standard` is the same union the two-invocation form built — `--others` means "not in the
# index", so the halves are disjoint. `git ls-files` measured ~1.2s per invocation on this box and
# `grep -c` another ~0.6s; this guard was performing SIX ls-files for one set of paths.
subject_paths=""
subject_paths="$(git ls-files --cached --others --exclude-standard -- "${SUBJECT_PATHSPEC[@]}" \
  | sort -u \
  | { grep -vE '(^|/)(target|node_modules|\.claude)/' || true; })"

source_scanned=0
while IFS= read -r _p; do
  [ -n "$_p" ] || continue
  if [ ${#NON_SOURCE_EXEMPTIONS[@]} -gt 0 ] && is_exempt "$_p" "${NON_SOURCE_EXEMPTIONS[@]}"; then
    continue
  fi
  source_scanned=$((source_scanned + 1))
done <<< "$subject_paths"

if [ "$source_scanned" -eq 0 ]; then
  echo "English-only source guard: FAILED -- enumerated ZERO source files. SUBJECT_PATHSPEC matched"
  echo "nothing, so neither the non-Latin-letter scan nor the .claude/ path scan below read a single"
  echo "byte. This repo ships Rust, Markdown and JSON; a zero here is a broken enumeration, never a"
  echo "clean tree."
  exit 1
fi

# The COLLAPSE floor, one axis past "did it match anything": a scope that still matches one stray file
# clears a zero-check while covering none of the workspace. Derived from Cargo.toml's [workspace]
# members, so it needs no baseline and widens with the tree (see tracked-grep.sh's own header for the
# 2026-07-31 measurement that motivated it).
assert_workspace_members_scanned "English-only source guard" "${SUBJECT_PATHSPEC[@]}"

if [ ${#NON_SOURCE_EXEMPTIONS[@]} -gt 0 ]; then
  for entry in "${NON_SOURCE_EXEMPTIONS[@]}"; do
    assert_exemption_is_live "NON_SOURCE_EXEMPTIONS" "${entry%%|*}" "${entry#*|}" "$subject_paths"
  done
fi

# The enumeration call is kept OUTSIDE any `|| true` on purpose: tracked_and_untracked_files_matching's
# own failure must still trip `set -e` and abort loud (see its header comment in tracked-grep.sh).
raw_foreign=$(tracked_and_untracked_files_matching "$FOREIGN" "${SUBJECT_PATHSPEC[@]}")
files=""
while IFS= read -r f; do
  [ -n "$f" ] || continue
  if [ ${#NON_SOURCE_EXEMPTIONS[@]} -gt 0 ] && is_exempt "$f" "${NON_SOURCE_EXEMPTIONS[@]}"; then
    continue
  fi
  files="${files}${f}"$'\n'
done <<< "$raw_foreign"
files="$(printf '%s' "$files")"

if [ -n "$files" ]; then
  echo "English-only source guard: non-Latin letters found in OSS files:"
  while IFS= read -r f; do
    grep -nP "$FOREIGN" "$f" | sed "s|^|  ${f#./}:|"
  done <<< "$files"
  echo
  echo "OSS-facing files (comments / docs) must be English. Korean is allowed only under .claude/."
  exit 1
fi
echo "English-only source guard: clean ($source_scanned files scanned)."

# Internal-path guard: OSS-facing files must never point readers at .claude/ — those paths are not
# published, so any "see .claude/context/..." reference is a broken pointer for anyone outside this
# repo's working tree. Rationale belongs inline (summarized) or in docs/, not linked by internal path.
# The pattern requires the trailing slash on purpose: `.claude-plugin/` (Claude Code's PUBLIC,
# tracked plugin-manifest directory, added 2026-07-17) is a legitimate reference and must not trip
# a guard about the PRIVATE untracked `.claude/` tree.
#
# Filtering the exempt prefixes OUT of the already-matched result (rather than out of the candidate
# list before grepping) is equivalent here — a candidate excluded before matching and a match excluded
# after matching land on the same final file set — and lets this reuse the same enumeration call shape
# as the block above. It is also what makes the both-directions check below cheap and meaningful: the
# exemption is asserted against the paths that ACTUALLY matched, so an entry excusing a file that no
# longer references .claude/ fails.
claude_ref_matches=$(tracked_and_untracked_files_matching '\.claude/' "${SUBJECT_PATHSPEC[@]}")
for entry in "${CLAUDE_REF_EXEMPTIONS[@]}"; do
  assert_exemption_is_live "CLAUDE_REF_EXEMPTIONS" "${entry%%|*}" "${entry#*|}" "$claude_ref_matches"
done

claude_ref_files=""
while IFS= read -r f; do
  [ -n "$f" ] || continue
  is_exempt "$f" "${CLAUDE_REF_EXEMPTIONS[@]}" && continue
  claude_ref_files="${claude_ref_files}${f}"$'\n'
done <<< "$claude_ref_matches"
claude_ref_files="$(printf '%s' "$claude_ref_files")"

if [ -n "$claude_ref_files" ]; then
  echo "English-only source guard: .claude/ path references found in OSS files:"
  while IFS= read -r f; do
    grep -nP '\.claude/' "$f" | sed "s|^|  ${f#./}:|"
  done <<< "$claude_ref_files"
  echo
  echo "OSS-facing files must not reference .claude/ paths — summarize the rationale inline instead."
  exit 1
fi
echo "Internal-path guard: clean ($source_scanned files scanned)."
