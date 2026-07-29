#!/usr/bin/env bash
# English-only source guard — fails when non-Latin letters (Hangul / CJK / Kana / Cyrillic)
# appear in OSS-facing files (Rust sources, READMEs, manifests, rule-pack JSON).
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
# `jsonc yaml tsx jsx py` are future-proofing (zero tracked files of these types today) — added so
# the first file of one of these types is covered from day one instead of slipping past the guard.
#
# Enumeration mechanism (TRACKED+untracked-not-ignored file discovery + grep + the standard
# target/node_modules/.claude exclusions, plus the PIPESTATUS-based producer-failure hardening) lives
# in scripts/lib/tracked-grep.sh's tracked_and_untracked_files_matching. check-max-file-lines.sh
# scans the same tracked+untracked surface for a different purpose (line counts, not a grep match)
# and is independently hardened via its own wc-based PIPESTATUS capture — it does not call this
# helper. This script keeps only its own glob list, patterns, and messages.
SOURCE_GLOBS=('*.rs' '*.md' '*.toml' '*.json' '*.jsonc' '*.js' '*.mjs' '*.cjs' '*.ts' '*.tsx' '*.jsx' '*.py' '*.html' '*.yml' '*.yaml' '*.sh')

# ## Subject-set floor (2026-07-29)
# Both scans below enumerate through SOURCE_GLOBS, and a helper that matched nothing returns exactly
# the same empty string whether the tree is clean or the pathspec stopped matching. This guard did not
# even derive a count, so "clean." was printed identically for 1600 files and for zero. Measured
# 2026-07-29 by pointing SOURCE_GLOBS at a nonexistent extension in a scratch copy: it printed
# "English-only source guard: clean." / "Internal-path guard: clean." and exited 0. Same class the
# repo closed in check-guards-wired.sh / check-max-file-lines.sh / check-shell-pipe-sigpipe.sh.
#
# SOURCE_GLOBS above is the single owner of the scope: the count and both scans read the same array,
# so widening the glob list cannot leave the assertion behind. The count mirrors the helper's own
# tracked + untracked-but-not-ignored enumeration; it does not re-apply the helper's standard
# target/node_modules/.claude exclusions, which only ever REMOVE files — a floor of "at least one"
# cannot be made false by them.
#
# ONE `git ls-files`, not two, and the count is done in bash (2026-07-29). `--cached --others
# --exclude-standard` is the same union the two-invocation form built — `--others` means "not in the
# index", so the halves are disjoint — verified against the old spelling on this repo the day it
# changed: 1264 paths, identical set and order after `sort -u`. It is the same single-invocation
# enumeration scripts/lib/tracked-grep.sh now uses, so the count and the scans still read one
# mechanism. `git ls-files` measured ~1.2s per invocation on this box and `grep -c` another ~0.6s;
# this guard was performing SIX ls-files for one set of paths (this block, plus the tracked and
# untracked halves inside each of the two scans below).
source_scanned=0
while IFS= read -r _p; do
  [ -n "$_p" ] && source_scanned=$((source_scanned + 1))
done < <(git ls-files --cached --others --exclude-standard -- "${SOURCE_GLOBS[@]}" | sort -u)
if [ "$source_scanned" -eq 0 ]; then
  echo "English-only source guard: FAILED -- enumerated ZERO source files. SOURCE_GLOBS matched nothing,"
  echo "so neither the non-Latin-letter scan nor the .claude/ path scan below read a single byte. This"
  echo "repo ships Rust, Markdown and JSON; a zero here is a broken enumeration, never a clean tree."
  exit 1
fi

# The enumeration call is kept OUTSIDE any `|| true` on purpose: tracked_and_untracked_files_matching's
# own failure must still trip `set -e` and abort loud (see its header comment in tracked-grep.sh).
files=$(tracked_and_untracked_files_matching "$FOREIGN" "${SOURCE_GLOBS[@]}")

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
# scripts/ is self-exempt here: guard machinery must name the very pattern it excludes (this
# file's own grep -v lines, max-file-lines/swc scope filters), which is not a reader-facing
# "see .claude/..." pointer. The Korean check above still covers scripts/. Filtering scripts/ OUT
# of the already-matched result (rather than out of the candidate list before grepping) is
# equivalent here — a candidate excluded before matching and a match excluded after matching land on
# the same final file set — and lets this reuse the same enumeration call shape as the block above.
claude_ref_matches=$(tracked_and_untracked_files_matching '\.claude/' "${SOURCE_GLOBS[@]}")
claude_ref_files=$(grep -v '^scripts/' <<< "$claude_ref_matches" || true)

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
