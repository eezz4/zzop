#!/usr/bin/env bash
# English-only source guard — fails when non-Latin letters (Hangul / CJK / Kana / Cyrillic)
# appear in OSS-facing files (Rust sources, READMEs, manifests, rule-pack JSON, and everything else
# this repo ships).
#
# Policy: code that ships is the explanation; comments and docs are English. Korean internal
# design notes are allowed ONLY under .claude/ (not published). This guard enforces that split.
# The one shipped exception is the site's Korean EDITION and the sentence data it is built from —
# both registered, with their reasons and their expiry conditions, in FOREIGN_LETTER_EXEMPTIONS below.
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

# The non-Latin scan's exemption list. It stood DELETED from 2026-08-09 until 2026-08-14, and the
# deletion was right for that window: what sat here before was an EMPTY array (`NON_SOURCE_EXEMPTIONS=()`)
# justified by "all 1,388 shipped files are clean" — a count that had silently drifted 207 files stale
# while the empty slot stood, which is exactly what an empty slot invites (the next excuse sits down in
# it without anyone re-deriving the justification). The instruction left behind was to re-introduce the
# array only when a genuinely exempt path landed, in `<prefix>|<reason>` form, wired through `is_exempt`
# + `assert_exemption_is_live`. That path landed on 2026-08-14, so the array is back with TWO entries
# and no spare slot — a third one has to arrive with its own reason or not at all.
#
# What landed: scripts/gen-site.mjs builds the public site as TWO editions, en and ko, from one set of
# `{ko, en}` sentence pairs. Korean is no longer only an internal note; one PUBLISHED page is written in
# it. The generator was shaped around this guard rather than the other way round (see its own header):
# the shell strings, the nav labels, the graph viewer's UI text were moved OUT of the generator and the
# stylesheet and INTO the pair data, precisely so that the exemption could be two narrow paths instead
# of "site/ and the things that build it".
#
# Both entries are kept as tight as they can be stated, because the failure that matters here is not a
# missing exemption (loud) but an over-wide one (silent): the moment `site/` or `site-src/` were excused
# wholesale, `site/index.html` — the ENGLISH edition, the page most readers see — would stop being
# checked, and a half-translated slot would ship with this guard green. The English edition is covered by
# this guard exactly as before, and that is the property to re-verify if either line below is ever widened.
FOREIGN_LETTER_EXEMPTIONS=(
  "site-src/content/|the ONLY place a {ko, en} sentence pair may live, and a pair is data, not prose in a shipped file: scripts/gen-site.mjs reads the ko side into site/ko/index.html and the en side into site/index.html, and refuses to write an English page containing a single Hangul character (its own HANGUL check, which is what keeps the English edition inside this guard). Nothing here is served to a reader as-is. VOID THIS ENTRY if Hangul appears in any OTHER build input — in \"scripts/gen-site.mjs\", in \"scripts/site/render.mjs\", or in the stylesheet — because then this stops being an exemption for a data format and becomes one for a directory; or if the build ever stops requiring both sides of a pair, since the en side is the only thing standing between this Korean and the English page."
  "site/ko/index.html|the GENERATED Korean edition, and generated is the whole reason it is excusable: it is byte-for-byte the output of the site generator (scripts/gen-site.mjs — spelled WITHOUT its interpreter on purpose, so this sentence cannot be read as an invocation site by check-guards-wired.sh) over the pairs above, which scripts/check-site-generated.sh re-derives and compares on every commit — so no Hangul can enter this file except through a {ko, en} pair that also produced an English sentence. It is committed because GitHub Pages serves site/ as files. Its English twin site/index.html is deliberately NOT exempt and must stay that way; that asymmetry is the guard. VOID THIS ENTRY if this page is ever hand-edited or stops being generated (the exemption would then cover hand-written Korean on a published page, which is the thing this guard exists to stop), or if site/ stops being one-language-per-file."
)

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

# The enumeration call is kept OUTSIDE any `|| true` on purpose: tracked_and_untracked_files_matching's
# own failure must still trip `set -e` and abort loud (see its header comment in tracked-grep.sh).
raw_foreign=$(tracked_and_untracked_files_matching "$FOREIGN" "${SUBJECT_PATHSPEC[@]}")

# Both directions, same rule the .claude/-reference list follows below: assert against the paths that
# ACTUALLY matched, so an entry excusing a file that no longer carries a non-Latin letter fails. That is
# what makes "the Korean site was deleted / renamed" a RED here instead of a silently inherited licence
# for whatever lands under `site/ko/` next.
for entry in "${FOREIGN_LETTER_EXEMPTIONS[@]}"; do
  assert_exemption_is_live "FOREIGN_LETTER_EXEMPTIONS" "${entry%%|*}" "${entry#*|}" "$raw_foreign"
done

files=""
exempted=0
while IFS= read -r f; do
  [ -n "$f" ] || continue
  if is_exempt "$f" "${FOREIGN_LETTER_EXEMPTIONS[@]}"; then
    exempted=$((exempted + 1))
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
  echo "OSS-facing files (comments / docs) must be English. Korean is allowed under .claude/, and on the"
  echo "site's Korean lane only — see FOREIGN_LETTER_EXEMPTIONS in this script for the two exempt paths,"
  echo "the reason each is exempt, and what would void it. Widening one of those entries is not a fix:"
  echo "site/index.html is the ENGLISH edition and stays inside this scan on purpose."
  exit 1
fi
echo "English-only source guard: clean ($source_scanned files scanned, $exempted exempt via FOREIGN_LETTER_EXEMPTIONS)."

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
