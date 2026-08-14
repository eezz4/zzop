#!/usr/bin/env bash
# io-key kind-vocabulary guard — fails when the io-key kind list ("http routes, env keys, DB
# tables, topics") drifts between its SSOT and the docs that restate it. The drift class already
# happened once, in packages/cli/README.md specifically: its `zzop endpoint` row shipped without
# "DB tables".
#
# SSOT = the parenthesized list after "cross-layer io key (" in packages/mcp/src/tools/
# definitions.rs's check_endpoint tool description. Token-level only (same idiom as
# check-site-sdk-tokens.sh): prose quality is NOT checked, only that the tokens agree with the SSOT.
#
# TWO checks, and they are different assertions:
#
#   1. packages/README.md's `check_endpoint` table row — REQUIRED to exist and to carry every token
#      (`check_row`). This one file is named on purpose: it is not "a surface we happened to think
#      of", it is a contract that this row exists at all, and it fails loudly ("re-anchor this
#      guard") if the row is renamed or removed. It also keeps at least one anchored site alive in
#      the tree, which the ANCHORS floor below depends on.
#
#   2. ALL-OR-NOTHING over every tracked .md/.html: wherever a doc is TALKING ABOUT io keys, it must
#      name every kind or none of them. Zero tokens passes, all four pass, a partial enumeration
#      FAILS. "The doc says nothing about io-key kinds" and "the doc says all of them" are both
#      truthful states; "the doc says some of them" is the defect, and is the only thing rejected.
#
# WHY THE SURFACE IS DERIVED (2026-07-28). Until today check (2) ran over exactly ONE hand-typed path
# (packages/cli/README.md), sitting next to check (1)'s hand-typed path. Measured that day: a
# sentence naming two of the four kinds, planted in the top-level README.md — the exact shape check
# (2) exists to reject — left this guard GREEN, because README.md was simply not in the list. A
# guard whose scanned set is hand-typed cannot see a surface nobody remembered to add, and this
# script's own header is the proof that nobody does: it spent a release describing a two-README
# world while the body checked one. The subject is now every tracked .md/.html (git ls-files), so a
# new doc is in scope the moment it is tracked, with no list for anyone to remember.
#
# WHY THERE IS A DISCRIMINATOR AND NOT AN ALLOWLIST. Widening a whole-file all-or-nothing rule to the
# full doc set reports 4 sites, and three of them are legitimate prose that merely happens to contain
# some of these very ordinary words:
#   - docs/demo/break-a-route.md — "19 HTTP routes + 30 shared DB tables" (measured demo counts)
#   - site/rules.html            — "the cross-layer IO (HTTP routes, DB tables, ...)" (explicit elision)
#   - site/usage.html            — "backend routes, shared DB tables, route drift" (prose on `zzop cross`).
#                                  That page became a redirect stub on 2026-08-14 and the sentence did not
#                                  move onto the new tab, so this third site no longer exists; the entry
#                                  stays because it is EVIDENCE FOR THE DESIGN CALL below (three sites, an
#                                  allowlist declined), not a roster of today's tree. Anyone re-measuring
#                                  the "4 sites" figure should expect fewer and re-derive it, not read it
#                                  off this line.
# The offer on the table was an allowlist holding those three, and an allowlist that grows is how a
# guard stops guarding — the reason check-vendor-token-literals.sh never got an escape hatch, and the
# same call check-docs-rule-ids.sh made hours earlier when its own widening produced 28 false
# positives. So the rule is anchored on MEANING instead: this vocabulary is the vocabulary OF IO
# KEYS, and text that restates it says so. A site is judged only when it is ANCHORED — by the phrase
# "io key" (the SSOT's own words) or by one of the three surfaces whose description carries the list
# (`check_endpoint`, `zzop endpoint`, `queryIo`/`query_io`). All three false positives above name none
# of those; every real enumeration (packages/README.md, docs/modules/facade.md, docs/modules/mcp.md,
# site/reference.html) names at least one. False positives 3 -> 0 with no allowlist, and the
# historical incident row is still caught: it read "Definitive io-key query: ... a case-insensitive
# substring of any io key — http routes, env keys, DB tables, topics", anchored three times over.
#
# SCOPE OF A SITE. A table row (a leading pipe, or an HTML <tr>/<td> line) is judged ALONE, on its
# own line: a row is a self-contained unit, and the incident this guard exists for was a table row,
# so a row must not be allowed to borrow a missing token from the row above it. Anything else is
# judged over the anchor line plus the next 4 lines, because prose wraps — docs/modules/facade.md
# wraps its enumeration mid-list, and a line-scoped rule would call that partial. Tokens match
# case-insensitively: site/reference.html writes "HTTP routes" for the SSOT's "http routes", which is
# the same kind, and a casing difference is not vocabulary drift.
#
# COST, as measured 2026-07-28. 1.7s wall over the 43 docs / 860 KB the tree held THAT DAY — a
# benchmark record, not a current inventory (the set is derived and has grown since; recount with
# `git ls-files -- '*.md' '*.html' | wc -l`). Which is the two-file version's cost within measurement
# noise (3 runs each: 1.72/1.74/2.38s new, 1.72/2.13/3.19s old). Under msys process creation dominates
# and the widened scan adds no processes: three total (the SSOT grep, git ls-files, ONE awk over every
# doc). Two things follow. There is deliberately NO per-file prefilter — check-docs-rule-ids.sh needs
# one because it spawns per file, but a single awk pass has nothing to save, and a prefilter is one
# more way to under-include silently. And the case folding below uses bash parameter expansion rather
# than `tr`: five `tr` spawns measured 2.6s, more than this entire guard.
set -euo pipefail
cd "$(dirname "$0")/.."

ssot=packages/mcp/src/tools/definitions.rs
[ -f "$ssot" ] || { echo "check-io-key-vocab: missing $ssot" >&2; exit 1; }
vocab="$(grep -oP 'cross-layer io key \(\K[^)]+' "$ssot" | head -n1 || true)"
[ -n "$vocab" ] || { echo "check-io-key-vocab: SSOT anchor 'cross-layer io key (' not found in $ssot — re-anchor this guard." >&2; exit 1; }

fail=0
check_row() { # $1 = file, $2 = table-row anchor (PCRE)
  local file="$1" anchor="$2" row row_lc tok tok_lc
  row="$(grep -P "$anchor" "$file" | head -n1 || true)"
  if [ -z "$row" ]; then
    echo "check-io-key-vocab: no table row matching '$anchor' in $file — re-anchor this guard." >&2
    fail=1
    return
  fi
  # `${var,,}` and not `tr` — see COST in the header.
  row_lc="${row,,}"
  local IFS=','
  for tok in $vocab; do
    tok="${tok# }"
    tok="${tok% }"
    tok_lc="${tok,,}"
    case "$row_lc" in
      *"$tok_lc"*) ;;
      *) echo "check-io-key-vocab: $file's row lacks io-key kind token '$tok' (SSOT: $ssot says \"$vocab\")." >&2; fail=1 ;;
    esac
  done
}

check_row packages/README.md '^\|\s*`check_endpoint`'

# --- Derived surface: every tracked .md / .html ---------------------------------------------------
docs=()
while IFS= read -r d; do docs+=("$d"); done < <(git ls-files -- '*.md' '*.html')
# An empty subject set is a broken enumeration, not a repo with no docs — the sibling failure this
# repo has already paid for twice (a scan root pointing at a deleted directory, green while reading
# nothing). Load-bearing now that the surface is a glob rather than a literal path. A tracked doc
# that is missing from the worktree is the other end of the same failure: awk exits 2 on it and
# `set -e` turns that into a red, so a doc can never be silently skipped.
if [ "${#docs[@]}" -eq 0 ]; then
  echo "check-io-key-vocab: FAILED — ZERO docs to scan. The tracked-doc enumeration matched" >&2
  echo "  nothing, so no doc was checked against the SSOT. This repo ships docs; a zero here is a" >&2
  echo "  broken scan, never a clean tree." >&2
  exit 1
fi

# Matched against the LOWERCASED line padded with a space on each side, so the [^a-z0-9] edges stand
# in for word boundaries (POSIX awk has no \b). Keep in sync with the header's discriminator list.
anchor_re='[^a-z0-9]io[ _-]?keys?[^a-z0-9]|check_endpoint|zzop endpoint|queryio|query_io'

scan="$(awk -v vocab="$vocab" -v anchor="$anchor_re" -v W=4 '
  function flush(   i, j, hi, win, t, present, missing, seen, miss, istable, padded) {
    for (i = 1; i <= n; i++) {
      padded = " " line[i] " "
      if (padded !~ anchor) continue
      anchors++
      # A table row is judged alone; prose gets a lookahead window. Header: SCOPE OF A SITE.
      istable = (line[i] ~ /^[ \t]*[|]/) || (line[i] ~ /<t[rdh][ >]/)
      if (istable) {
        win = line[i]
      } else {
        hi = i + W; if (hi > n) hi = n
        win = ""
        for (j = i; j <= hi; j++) win = win " " line[j]
      }
      present = 0; missing = 0; seen = ""; miss = ""
      for (t = 1; t <= ntok; t++) {
        if (index(win, tok[t]) > 0) { present++; seen = seen " [" rawtok[t] "]" }
        else { missing++; miss = miss " [" rawtok[t] "]" }
      }
      if (present > 0 && missing > 0) printf "HIT\t%s:%d\t%s\t%s\n", f, i, seen, miss
    }
  }
  BEGIN {
    ntok = split(vocab, rawtok, ",")
    for (t = 1; t <= ntok; t++) {
      gsub(/^[ \t]+|[ \t]+$/, "", rawtok[t])
      tok[t] = tolower(rawtok[t])
    }
    anchors = 0; n = 0
  }
  FNR == 1 && n > 0 { flush() }
  FNR == 1 { f = FILENAME; n = 0; delete line }
  { line[++n] = tolower($0) }
  END { if (n > 0) flush(); printf "ANCHORS\t%d\n", anchors }
' "${docs[@]}")"

# One pass over the scan output, in bash: the ANCHORS trailer and the HIT lines come out of the same
# awk, and re-reading them with a second awk cost more than the scan itself under msys.
anchors=0
while IFS=$'\t' read -r kind col2 seen miss; do
  case "${kind:-}" in
    ANCHORS) anchors="$col2" ;;
    HIT)
      echo "check-io-key-vocab: $col2 enumerates the io-key kind vocabulary only PARTIALLY —" >&2
      echo "  present:$seen / MISSING:$miss (SSOT: $ssot says \"$vocab\")." >&2
      echo "  This is the exact defect that shipped once (an endpoint row without 'DB tables')." >&2
      echo "  Either name every kind at this site or name none of them." >&2
      fail=1
      ;;
  esac
done <<< "$scan"

# Zero anchored sites means the discriminator stopped matching anything — the docs renamed "io key",
# or this regex rotted. Either way the scan read every doc and judged none of them, which reads as
# green. check_row above proves at least one anchored site exists (packages/README.md's row names
# both "io key" and check_endpoint), so a zero here is a broken discriminator, never a clean tree.
if [ "$anchors" -eq 0 ]; then
  echo "check-io-key-vocab: FAILED — ZERO anchored io-key sites across ${#docs[@]} docs. The" >&2
  echo "  discriminator ('$anchor_re') matched nothing, so every doc was read and none was judged." >&2
  echo "  packages/README.md's check_endpoint row alone should anchor — re-anchor this guard." >&2
  exit 1
fi

if [ "$fail" -ne 0 ]; then
  echo "check-io-key-vocab: FAILED — sync the doc vocabulary with the SSOT ($vocab)." >&2
  exit 1
fi
echo "check-io-key-vocab: OK (vocabulary: $vocab; ${#docs[@]} docs, $anchors anchored sites)"
