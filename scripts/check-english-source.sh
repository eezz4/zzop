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

# Non-Latin letter scripts. Punctuation / symbols (— · → ★) are intentionally NOT flagged.
FOREIGN='[\x{AC00}-\x{D7A3}\x{1100}-\x{11FF}\x{3130}-\x{318F}\x{3040}-\x{30FF}\x{4E00}-\x{9FFF}\x{0400}-\x{04FF}]'

# Scope = tracked files PLUS untracked-but-not-ignored ones: OSS-facing means "will ship in the
# repo". Untracked NEW files must be scanned too — a fresh file passes a tracked-only scan before
# its first `git add` and the violation lands in the commit (happened 2026-07-14 with a new test
# file). Ignored paths (local dogfood corpora of third-party repos with legitimate i18n text) stay
# out via --exclude-standard. CI runs on a clean checkout, where this reduces to tracked-only.
# `jsonc yaml tsx jsx py` are future-proofing (zero tracked files of these types today) — added so
# the first file of one of these types is covered from day one instead of slipping past the guard.
list_source_files() {
  { git ls-files -- '*.rs' '*.md' '*.toml' '*.json' '*.jsonc' '*.js' '*.mjs' '*.cjs' '*.ts' '*.tsx' '*.jsx' '*.py' '*.html' '*.yml' '*.yaml' '*.sh'
    git ls-files --others --exclude-standard -- '*.rs' '*.md' '*.toml' '*.json' '*.jsonc' '*.js' '*.mjs' '*.cjs' '*.ts' '*.tsx' '*.jsx' '*.py' '*.html' '*.yml' '*.yaml' '*.sh'
  } | sort -u
}
# git ls-files emits root-relative paths (no leading ./), so exclusions must anchor with (^|/).
files=$(list_source_files \
  | xargs -d '\n' grep -lP "$FOREIGN" 2>/dev/null \
  | grep -vE '(^|/)\.claude/' | grep -vE '(^|/)target/' | grep -vE '(^|/)node_modules/' || true)

if [ -n "$files" ]; then
  echo "English-only source guard: non-Latin letters found in OSS files:"
  while IFS= read -r f; do
    grep -nP "$FOREIGN" "$f" | sed "s|^|  ${f#./}:|"
  done <<< "$files"
  echo
  echo "OSS-facing files (comments / docs) must be English. Korean is allowed only under .claude/."
  exit 1
fi
echo "English-only source guard: clean."

# Internal-path guard: OSS-facing files must never point readers at .claude/ — those paths are not
# published, so any "see .claude/context/..." reference is a broken pointer for anyone outside this
# repo's working tree. Rationale belongs inline (summarized) or in docs/, not linked by internal path.
# The pattern requires the trailing slash on purpose: `.claude-plugin/` (Claude Code's PUBLIC,
# tracked plugin-manifest directory, added 2026-07-17) is a legitimate reference and must not trip
# a guard about the PRIVATE untracked `.claude/` tree.
# scripts/ is self-exempt here: guard machinery must name the very pattern it excludes (this
# file's own grep -v lines, max-file-lines/swc scope filters), which is not a reader-facing
# "see .claude/..." pointer. The Korean check above still covers scripts/.
claude_ref_files=$(list_source_files \
  | grep -v '^scripts/' \
  | xargs -d '\n' grep -lP '\.claude/' 2>/dev/null \
  | grep -vE '(^|/)\.claude/' | grep -vE '(^|/)target/' | grep -vE '(^|/)node_modules/' || true)

if [ -n "$claude_ref_files" ]; then
  echo "English-only source guard: .claude/ path references found in OSS files:"
  while IFS= read -r f; do
    grep -nP '\.claude/' "$f" | sed "s|^|  ${f#./}:|"
  done <<< "$claude_ref_files"
  echo
  echo "OSS-facing files must not reference .claude/ paths — summarize the rationale inline instead."
  exit 1
fi
echo "Internal-path guard: clean."
