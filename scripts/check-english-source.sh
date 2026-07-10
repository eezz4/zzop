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

# Scope = git-tracked files only: OSS-facing means "ships in the repo". A filesystem-wide grep
# also sweeps untracked local corpora (dogfood checkouts of third-party repos, which legitimately
# contain i18n text) — those are not ours to police. CI runs on a clean checkout, so tracked-only
# scanning is identical there.
files=$(git ls-files -- '*.rs' '*.md' '*.toml' '*.json' '*.js' '*.mjs' '*.cjs' '*.ts' \
  | xargs -d '\n' grep -lP "$FOREIGN" 2>/dev/null \
  | grep -v '/\.claude/' | grep -v '/target/' | grep -v '/node_modules/' || true)

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
claude_ref_files=$(git ls-files -- '*.rs' '*.md' '*.toml' '*.json' '*.js' '*.mjs' '*.cjs' '*.ts' \
  | xargs -d '\n' grep -lP '\.claude' 2>/dev/null \
  | grep -v '/\.claude/' | grep -v '/target/' | grep -v '/node_modules/' || true)

if [ -n "$claude_ref_files" ]; then
  echo "English-only source guard: .claude/ path references found in OSS files:"
  while IFS= read -r f; do
    grep -nP '\.claude' "$f" | sed "s|^|  ${f#./}:|"
  done <<< "$claude_ref_files"
  echo
  echo "OSS-facing files must not reference .claude/ paths — summarize the rationale inline instead."
  exit 1
fi
echo "Internal-path guard: clean."
