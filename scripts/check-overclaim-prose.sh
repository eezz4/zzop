#!/usr/bin/env bash
# check-overclaim-prose.sh — guards against the "published prose promises more than the engine
# delivers" defect class. Its sibling check-deploy-facts-prose.sh covers the NUMERIC half of the same
# problem (a count written into prose, one copy changed, the rest rotted). This one covers the
# NON-numeric half: absolute claims — "every finding …", "never misses", "100%", "we guarantee" — which
# no count can falsify and which therefore shipped to the live site unchallenged.
#
# The confirmed instance (commit 26357ad, 2026-07-24, found by reading the deployed site, not by any
# machine): site/index.html said "every finding carries its fix" and "Every finding reports three
# things: the problem, the fix, and the exact exclude config". Both were false — `fix` lives only on a
# DSL rule's `RuleExplain`, so native/structural findings carry a message plus the disable config and no
# `fix` at all. The sentence was hand-fixed; nothing stopped the next one. That is what this guard is.
#
# ---------------------------------------------------------------------------------------------------
# Why this is a CLAIM-SHAPE guard and not a word blacklist
# ---------------------------------------------------------------------------------------------------
# There is no SSOT to derive an absolute claim from the way check-deploy-facts-prose.sh derives every
# number it asserts — "is this sentence true?" is not a shell question. So this guard does the one thing
# a script honestly can: it refuses a SHORT list of claim SHAPES that are indefensible in published
# prose, and requires every surviving occurrence to be vetted once, by name, in the allowlist below.
#
# The design constraint that shaped every pattern here: a guard that cries wolf gets disabled. Three
# concrete consequences, each of which removed a tempting-but-wrong pattern during authoring:
#
#   1. NEVER a bare word. `guarantee` alone matches four honest sentences in this repo today
#      (VERSIONING.md's "there are no backward-compatibility guarantees yet", SECURITY.md's "no
#      guaranteed response time", the `map-async-no-promise-all` rule's catalog/site row
#      "ordering/completion guarantees are lost",
#      docs/modules/facade.md's internal "the default otherwise guarantees a non-empty pack set").
#      Only a POSITIVE guarantee OF completeness is a claim: `we guarantee`, `guarantees no/every/all/
#      complete/zero/100`, `guaranteed to/complete/secure/safe/accurate`. Zero hits today, by design.
#   2. NEVER a bare universal quantifier. `every finding` alone matches eleven sentences today, nine of
#      which are contract statements sitting next to the field table that defines them
#      (docs/modules/facade.md's "Every finding … has this shape", docs/rules/*'s marker/message
#      rules). The claim shape is the quantifier plus a CONTENT-PROMISE VERB — "every finding
#      carries/reports/includes/names/has/provides/…" — which is exactly the shape both shipped
#      overclaims wore, and which "every finding is one of three levels" or "copied verbatim into every
#      finding" do not. Three hits today (see the allowlist).
#   3. `exhaustive` was tried and REMOVED. All six occurrences in the tree are honest technical uses,
#      five of them negations ("not an exhaustive vocabulary", "non-exhaustive", "exhaustive at that
#      level"). A pattern with a 100% false-positive rate is worse than no pattern.
#
# Negation carve-out (the other half of not crying wolf): this repo's doctrine is to DISCLOSE what it
# cannot do, so "zzop does not detect every vulnerability" is the opposite of an overclaim and must not
# fail. A match is skipped when the 32 characters preceding it ON ITS OWN LINE contain a negator (`no`,
# `not`, `never`, `n't`, `without`, `cannot`, `nothing`, `neither`, `nor`, `rather than`, `instead of`).
# The window is short deliberately: long enough for "does not detect every vulnerability", short enough
# that an unrelated earlier "no Node, no npm" in the same sentence does not license a real overclaim
# further along it.
#
# ---------------------------------------------------------------------------------------------------
# Scan surface
# ---------------------------------------------------------------------------------------------------
# TRACKED `*.md` and `*.html` — the published prose surface (README.md, VERSIONING.md, SECURITY.md and
# every docs/ page come in through the `*.md` glob; the marketing site through `*.html`) — PLUS three
# named JSON files whose `description` string is published prose in every sense that matters:
#   server.json                     -> the MCP Registry listing
#   .claude-plugin/plugin.json      -> the Claude Code plugin marketplace listing
#   packages/mcpb/manifest.json     -> the Claude Desktop bundle listing
# Added 2026-08-08. These three are, by reach, the most widely READ sentences this repo publishes —
# they are what someone sees BEFORE deciding to install, which is exactly the moment an absolute claim
# does its damage — and until then a "never misses" in any of them left all 27 guards green. They are
# named individually rather than swept in as `*.json`, because the repo's other JSON is data (rule
# packs, fixtures, lockfiles) where these words carry no promise. Verified by planting "never misses"
# into server.json's description: reported by file and line. `.github/**`
# is excluded: PR/issue templates are contributor instructions, not product claims. `*.rs` is
# deliberately OUT of scope, unlike check-deploy-facts-prose.sh's surface — a doc comment's "guarantees
# a non-empty pack set" is a statement about an internal invariant addressed to a maintainer reading
# the code beside it, not a promise made to a user who cannot see it. Generated/vendored trees are
# excluded by scripts/lib/tracked-grep.sh's standard exclusions.
#
# ---------------------------------------------------------------------------------------------------
# Known-uncovered (documented, not silently ignored)
# ---------------------------------------------------------------------------------------------------
#   - a claim interrupted by punctuation or markup between the quantifier and its verb, e.g.
#     docs/modules/facade.md's "Every finding — from a DSL rule pack or a native analysis alike — has
#     this shape" (an em-dash aside sits where the verb would be) or a `<code>` tag splitting the
#     phrase. Widening the gap to swallow arbitrary text between the two halves would match across
#     unrelated clauses; covering it properly needs a parser, not grep.
#   - the TRUTH of a claim. This guard proves only that each absolute claim in the tree was vetted once
#     and has not been reworded since. If an allowlisted sentence becomes false because the CODE
#     changed underneath it, nothing here notices — that is what the allowlist's per-entry citation of
#     a machine-backed source (a struct field, a contract test) is for on the next read.
#   - a `100%` inside an inline `style=` attribute would fire and need an allowlist entry. No page uses
#     inline styles today (the site ships one external stylesheet); the escape hatch exists if that
#     changes. An HTML `<style>` BLOCK is a different matter and is skipped outright — see below.
#
# ---------------------------------------------------------------------------------------------------
# `<style>` blocks are not prose (2026-07-29)
# ---------------------------------------------------------------------------------------------------
# Lines inside an HTML `<style>` element are excluded from the claim scan. CSS is not a promise made to
# a reader, and `width: 100%` is not the `100%` this guard is looking for. The rule was already in force
# for the site's external stylesheet — `*.css` is not in the scan surface at all, and site/assets/site.css
# carries four `100%` declarations nobody has ever had to allowlist. site/graph.html then landed a
# page-scoped `<style>` block, and the identical declaration in the identical role fired three times
# purely because of which FILE it lived in. Allowlisting it would have recorded a CSS length as a
# "vetted claim about the product", which is worse than the gap: the allowlist's entries are supposed to
# be sentences someone can go and check.
#
# Deliberately NOT extended to `<script>`: a string literal in a script can be user-facing text (a
# tooltip, an empty-state message), and this page's viewer proves it — `showDetail`'s EMPTY constant is
# a sentence shown to a reader. Script bodies stay in scope.
set -euo pipefail
cd "$(dirname "$0")/.."

# shellcheck source=scripts/lib/tracked-grep.sh
. ./scripts/lib/tracked-grep.sh

SELF=check-overclaim-prose

# ---------------------------------------------------------------------------------------------------
# Allowlist — absolute claims that are TRUE and machine-backed, vetted individually.
# ---------------------------------------------------------------------------------------------------
# Format: `<tracked path>|<normalized claim>`, where the normalized claim is the matched text
# lowercased with runs of whitespace collapsed — i.e. exactly what this guard prints when it fails, so
# a legitimate new claim is allowlisted by pasting the reported line. The claim text INCLUDES the
# quantifier's object (up to three words past the verb), so rewording the predicate — the 26357ad
# defect, `carries a rule id` -> `carries its fix` — produces a different key and fails again. Path is
# part of the key: vetting a claim on one page does not license the same sentence on another, where its
# surrounding qualifications may not exist.
#
# An allowlist entry that matches NOTHING on a run is itself a failure (see the stale check at the
# bottom). An allowlist rots exactly like the prose it exempts; this is what stops it accumulating
# permissions for sentences that no longer exist.
ALLOWLIST=(
  # crates/core/src/finding.rs's `Finding` declares `rule_id`/`severity`/`file`/`line` as non-Option
  # fields, so every serialized finding does carry all four. Documented shape: docs/modules/facade.md's
  # "Every finding ... has this shape" table.
  "README.md|every finding carries a rule id"

  # The disable hint is appended to every DSL finding by crates/engine/src/pipeline/findings.rs's
  # `append_hints` (pinned by crates/engine/src/tests.rs's
  # `dsl_finding_message_carries_the_config_disable_hint_for_its_own_id`), and every native rule file
  # that builds a Finding is required to name `disabled_rules` or call `zzop_core::disable_hint` by
  # crates/engine/tests/rule_contracts/native_messages.rs. That test documents itself as a file-level
  # co-occurrence proxy rather than a semantic proof — the claim is backed, but by a pragmatic check,
  # which is why it is vetted here rather than treated as self-evident.
  "site/index.html|every finding names the exact config"
)

# ---------------------------------------------------------------------------------------------------
# Claim shapes
# ---------------------------------------------------------------------------------------------------
# A. Absolute coverage — a static analyzer cannot honestly say any of these about its own detection.
absolutes='never misses|misses nothing|catches everything|finds everything'
absolutes="$absolutes|nothing (gets|slips) (past|through|by)"
absolutes="$absolutes|(no|zero) false (positive|negative)s?"
absolutes="$absolutes|100 ?%|100 percent"
absolutes="$absolutes|always (detects|catches|finds|prevents|reports)"
absolutes="$absolutes|bullet-?proof|fool-?proof|airtight"
absolutes="$absolutes|completely (secure|safe|accurate|reliable)|fully secure"
absolutes="$absolutes|every (vulnerability|security issue|bug|exploit|attack)"
absolutes="$absolutes|all (vulnerabilities|security issues|bugs|exploits)"

# B. Positive guarantees OF completeness (never the bare word — see the header).
guarantees='we guarantee|guarantees? (that )?(no|zero|every|all|complete|100)'
guarantees="$guarantees|guaranteed (to|complete|secure|safe|accurate)"

# C. A universal quantifier over the output plus a content-promise verb, capturing up to three words
#    past the verb so the allowlist key names WHAT is promised, not just that something is.
qverb='carries|carry|reports|report|includes|include|names|name|has|have|contains|contain|provides|provide|comes with|come with|ships with|ship with'
universals="(every|each|all) (findings?|rules?) ($qverb)( +[A-Za-z][A-Za-z'-]*){0,3}"

claim_pattern="$absolutes|$guarantees|$universals"

# The prefilter is a cheap PCRE superset of the above (bare `guarantee`, bare quantifier+noun): it only
# decides which files the precise ERE pass below reads, so over-matching here costs nothing but a file
# read, while under-matching would silently skip a real hit.
prefilter='(?i)never misses|misses nothing|catches everything|finds everything|nothing (gets|slips)'
prefilter="$prefilter|(no|zero) false (positive|negative)|100 ?%|100 percent"
prefilter="$prefilter|always (detects|catches|finds|prevents|reports)|bullet-?proof|fool-?proof|airtight"
prefilter="$prefilter|completely (secure|safe|accurate|reliable)|fully secure"
prefilter="$prefilter|every (vulnerability|security issue|bug|exploit|attack)"
prefilter="$prefilter|all (vulnerabilities|security issues|bugs|exploits)|guarantee"
prefilter="$prefilter|(every|each|all) (findings?|rules?)"

negators="no|not|never|n't|without|cannot|nothing|neither|nor|rather than|instead of"

candidate_files="$(tracked_files_matching "$prefilter" '*.md' '*.html' 'server.json' '.claude-plugin/plugin.json' 'packages/mcpb/manifest.json' ':!:.github/**')" \
  || { echo "$SELF: file enumeration failed" >&2; exit 1; }

# Counted in bash rather than by a `grep -c .` on the end of the pipe (2026-07-29): on this box any
# external command costs ~0.6s of fork/exec whatever it does, and counting lines is something the
# shell already reading them can do for free. The enumeration itself is untouched — same pathspec,
# same `git ls-files`, same number.
total_files=0
while IFS= read -r _p; do
  [ -n "$_p" ] && total_files=$((total_files + 1))
done < <(git ls-files -- '*.md' '*.html' 'server.json' '.claude-plugin/plugin.json' 'packages/mcpb/manifest.json' ':!:.github/**')

# ## Subject-set floor (2026-07-29) — added even though the collapse is currently caught elsewhere
# This guard was reported as printing "clean (0 files scanned)" on an empty scan surface. Measured, and
# the report is WRONG as the tree stands: with the globs redirected in a scratch copy it went RED, via
# the stale-ALLOWLIST check at the bottom — every vetted claim stops matching when nothing is read, so
# two "stale ALLOWLIST entry ... matched nothing" failures fire and `fail=1`.
#
# That net is real but CONTINGENT, and on the one variable a maintainer is most free to change: the
# size of ALLOWLIST. Measured in the same session with ALLOWLIST emptied AND the globs redirected —
# "check-overclaim-prose: clean (0 files scanned; 0 vetted claims)." exit 0. So the guard's honesty
# about reading nothing currently rests on there happening to be two entries in an allowlist whose
# stated purpose is to SHRINK toward zero as prose gets fixed. That is the same shape as
# check-max-file-lines.sh's note about its own stale-baseline net (empty ratchet, empty net), and it is
# why the floor is asserted directly rather than left to a side effect.
if [ "$total_files" -eq 0 ]; then
  echo "$SELF: FAILED -- enumerated ZERO tracked *.md/*.html files outside .github/**. The published" >&2
  echo "  prose surface is empty, so no absolute claim was read and none was vetted. An empty subject" >&2
  echo "  set is a broken guard, never clean prose." >&2
  exit 1
fi

files=()
while IFS= read -r f; do
  [ -n "$f" ] && files+=("$f")
done <<< "$candidate_files"

matches=""
if [ "${#files[@]}" -gt 0 ]; then
  matches="$(grep -HnoiE "$claim_pattern" -- "${files[@]}" || true)"
fi

# Which allowlist entries actually fired this run — index-parallel to ALLOWLIST.
used=()
for _ in "${ALLOWLIST[@]}"; do used+=(0); done

# The negation window is tested with bash's own ERE engine instead of a `grep -qiE` per match, and
# the matched line is read out of a per-file line cache instead of a `sed -n "${lineno}p"` per match
# (2026-07-29). Two processes per match, and the shell can answer both questions itself: `mapfile` is
# a builtin reading through a redirect (no fork), and `[[ $s =~ $re ]]` is the same POSIX ERE grep
# was given. Case-insensitivity is done by lowercasing the window with `${window,,}` — the same
# parameter-expansion-not-`tr` choice check-io-key-vocab.sh's header records — and every negator is
# already lowercase, so `-i` had nothing else to fold.
#
# The cache holds ONE file at a time, which is enough: `grep -H` emits its matches grouped by file,
# so a file is loaded once and every match in it is served from that load.
cached_file=""
cached_lines=()
# Index-parallel to cached_lines: 1 where that line sits inside a `<style>` element. Built in the same
# pass that loads the file, so a skipped CSS declaration costs no extra process (see the header for why
# CSS is out of scope at all). Boundary lines are marked too — a claim sharing a line with `<style>` or
# `</style>` is CSS-adjacent enough that reading it as prose would be the same category error.
cached_in_style=()
negators_re="(^|[^A-Za-z])($negators)([^A-Za-z]|$)"

fail=0
while IFS= read -r row; do
  [ -n "$row" ] || continue
  file="${row%%:*}"
  rest="${row#*:}"
  lineno="${rest%%:*}"
  text="${rest#*:}"

  # Normalize: lowercase, collapse whitespace runs. Pure bash, no subprocess.
  claim="${text,,}"
  while [ "$claim" != "${claim//  / }" ]; do claim="${claim//  / }"; done
  claim="${claim#"${claim%%[![:space:]]*}"}"
  claim="${claim%"${claim##*[![:space:]]}"}"

  # Negation carve-out — see the header. `${line%%"$text"*}` is everything on the line before this
  # match; only its last 32 characters are consulted.
  if [ "$file" != "$cached_file" ]; then
    mapfile -t cached_lines < "$file"
    cached_file="$file"
    cached_in_style=()
    _in_style=0
    _n=0
    while [ "$_n" -lt "${#cached_lines[@]}" ]; do
      _l="${cached_lines[$_n],,}"
      case "$_l" in *"<style"*) _in_style=1 ;; esac
      cached_in_style[$_n]=$_in_style
      case "$_l" in *"</style"*) _in_style=0 ;; esac
      _n=$((_n + 1))
    done
  fi
  # A CSS declaration is not a claim. Checked before the negation window so a `<style>` line never
  # consults the allowlist either — an entry vetting a length would be noise in a list of sentences.
  if [ "${cached_in_style[$((lineno - 1))]:-0}" = 1 ]; then
    continue
  fi
  # mapfile is 0-indexed; grep line numbers are 1-based. The `:-` is not decoration: under `set -u`
  # an out-of-range index is an "unbound variable" abort, and the `sed -n "${lineno}p"` this replaced
  # simply yielded the empty string. That can only happen if the file changed between the grep above
  # and this read, and the two spellings should not disagree about what to do when it does.
  line="${cached_lines[$((lineno - 1))]:-}"
  prefix="${line%%"$text"*}"
  if [ "$prefix" != "$line" ]; then
    # NOT `${prefix: -32}`: bash yields the EMPTY string when a negative offset exceeds the string's
    # length, so a prefix shorter than the window would silently disable the carve-out entirely.
    start=0
    [ "${#prefix}" -gt 32 ] && start=$(( ${#prefix} - 32 ))
    window="${prefix:$start}"
    if [[ ${window,,} =~ $negators_re ]]; then
      continue
    fi
  fi

  key="$file|$claim"
  allowed=0
  i=0
  while [ "$i" -lt "${#ALLOWLIST[@]}" ]; do
    if [ "${ALLOWLIST[$i]}" = "$key" ]; then
      allowed=1
      used[$i]=1
      break
    fi
    i=$((i + 1))
  done
  [ "$allowed" -eq 1 ] && continue

  echo "$SELF: $file:$lineno: absolute claim \"$claim\"" >&2
  echo "    -> fix the prose, or (if it is TRUE and machine-backed) add \"$key\" to this guard's" >&2
  echo "       ALLOWLIST with a comment citing what backs it." >&2
  fail=1
done <<< "$matches"

# Stale allowlist entries: the prose they exempted is gone or reworded, so the permission must go too.
i=0
while [ "$i" -lt "${#ALLOWLIST[@]}" ]; do
  if [ "${used[$i]}" -eq 0 ]; then
    echo "$SELF: stale ALLOWLIST entry \"${ALLOWLIST[$i]}\" matched nothing -- remove it (the claim it" >&2
    echo "    exempted was deleted or reworded; a standing exemption for prose that no longer exists" >&2
    echo "    silently pre-approves whatever is written there next)." >&2
    fail=1
  fi
  i=$((i + 1))
done

if [ "$fail" -ne 0 ]; then
  echo "$SELF: FAILED -- published prose must not promise more than the engine delivers." >&2
  exit 1
fi

echo "$SELF: clean ($total_files files scanned; ${#ALLOWLIST[@]} vetted claims)."
