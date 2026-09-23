#!/usr/bin/env bash
# Guard: every document that ENUMERATES the shipped-off analyses names exactly the set the engine
# ships — no id missing, and no id that used to be in it left behind.
#
# WHAT THIS PROTECTS
# `zzop_rules_graph::DEFAULT_OFF` decides which native analyses a run does not evaluate unless the
# config names them. That decision is invisible in a reply's `findings` (a shipped-off analysis produces
# no key, exactly like one that ran clean), so what a reader has instead is prose: the starter template,
# the rule catalog, VERSIONING, the getting-started page and the site's usage page each spell the three
# ids out. Every copy of one list is another chance to be wrong the day a fourth id lands
# — and each of them is a sentence a user CONFIGURES against, so a stale one does not read as stale, it
# reads as "this id runs by default" and sends them to debug a rule that never ran.
#
# TOTAL ACCOUNTING, BOTH DIRECTIONS. For each document this checks the paragraph that claims the set
# (found by a marker, never by line number) and requires the ids named in it to EQUAL `DEFAULT_OFF`:
#   - an id in the const and missing from the paragraph  -> a user is never told it ships off;
#   - an id in the paragraph and not in the const        -> a user is told to opt into a rule that is
#                                                           already on, and never told about the real one.
# The second direction is the one a "does the doc mention X" grep cannot do, and it is the direction a
# REMOVAL from the set takes.
#
# WHAT IT DOES NOT COVER, stated rather than left to be discovered:
#   - prose that DESCRIBES the set without enumerating it ("three analyses ship off") is not checked for
#     its count. A guard cannot tell a deliberate round number from a stale one, and this file would be
#     the wrong place to guess.
#   - the 61.7% measurement quoted beside the list. It is a corpus reading, re-derived by
#     `scripts/measure/`, and pinning it here would make a re-measurement a documentation failure.
#   - documents that mention ONE of the ids in passing (`docs/NORMALIZED_AST.md` names
#     `dead-candidates` describing an envelope field). Those make no claim about the set, so they carry
#     no marker and are not read — which is why the marker is required rather than the mention.
set -euo pipefail
cd "$(dirname "$0")/.."

fail=0
err() { printf 'check-shipped-off-sync: %s\n' "$*" >&2; fail=1; }

# --- The SSOT: the const in the owning crate ------------------------------------------------------
# Read from the source of truth rather than from any document, so this guard cannot be satisfied by
# editing the same prose it is checking.
src="rules/native/rules-graph/src/lib.rs"
[ -f "$src" ] || { err "missing $src — the shipped-off SSOT moved and this guard did not follow"; exit 1; }

default_off="$(awk '
  /pub const DEFAULT_OFF/ { collecting = 1 }
  collecting {
    line = line $0
    if (/;[[:space:]]*$/) { collecting = 0; done = 1 }
  }
  END {
    if (!done) exit 1
    # Everything inside the outermost brackets, split on quotes.
    sub(/^.*\[/, "", line)
    sub(/\].*$/, "", line)
    n = split(line, parts, /"/)
    for (i = 2; i <= n; i += 2) print parts[i]
  }
' "$src" | sort -u)"

if [ -z "$default_off" ]; then
  err "could not read \`pub const DEFAULT_OFF\` out of $src. This guard reads the const rather than a"
  err "hand list on purpose, so an unreadable const is a hard failure — a silent skip here would let"
  err "every document below drift with nothing watching."
  exit 1
fi

count="$(printf '%s\n' "$default_off" | wc -l | tr -d ' ')"
if [ "$count" -lt 1 ]; then
  err "DEFAULT_OFF parsed to an empty set — refusing to certify every claiming document against nothing"
  exit 1
fi

# --- The id universe, so "an id this paragraph names" is decidable --------------------------------
# Native analysis ids come from the catalog's own native table — the same extraction
# `check-docs-rule-ids.sh` uses, and the reason this guard can tell "names a stale shipped-off id" from
# "happens to contain a word". Without a universe the second direction is unimplementable.
catalog="docs/rules/catalog.md"
[ -f "$catalog" ] || { err "missing $catalog"; exit 1; }
native_ids="$(awk '
  /^## Native analyses/ { mode = "native"; next }
  /^###? /              { mode = ""; next }
  /^\| `[a-z0-9]/ {
    if (mode != "native") next
    id = $0
    sub(/^\| `/, "", id)
    sub(/`.*/, "", id)
    print id
  }
' "$catalog" | sort -u)"

if [ -z "$native_ids" ]; then
  err "the native-analysis table in $catalog read as empty — the universe this guard decides against"
  err "is gone, so every check below would pass vacuously"
  exit 1
fi

# Every shipped-off id must be a real catalogued analysis. A typo in the const would otherwise make
# this whole guard check every claiming document against a name nothing ships.
#
# `set -e` HAZARD, and the reason it is worth a comment: a `grep` that legitimately finds nothing is the
# NORMAL case in every loop below, and under `set -e` a non-zero last command in a pipeline or a command
# substitution aborts the whole script — with no message, since the failing command is a silent `-q`
# grep. That is exactly how a guard stops guarding while still being wired in: it exits 1, CI shows a
# red line with no text, and the next person deletes it as flaky. Every such spot here is therefore
# closed with an explicit `|| true` / `if` rather than left to luck.
# A HERESTRING, not `printf | grep -q`: under `pipefail` a `-q` grep exits at its first match and
# SIGPIPEs the producer, which flips the pipeline's verdict on large input. `check-shell-pipe-sigpipe.sh`
# owns that rule and caught this line before it was ever committed.
while IFS= read -r id; do
  if ! grep -qxF "$id" <<< "$native_ids"; then
    err "DEFAULT_OFF names \`$id\`, which is not in $catalog's native-analysis table — either the id is"
    err "  misspelled in $src, or it ships off without being documented as an analysis at all"
  fi
done <<EOF
$default_off
EOF

# --- The documents, each with the marker that finds its claiming paragraph ------------------------
# A marker is a phrase the CLAIM itself carries, not a heading or a line number, so ordinary editing of
# the surrounding prose does not move it. Adding a seventh document is a row here.
# Claim/document tallies, COUNTED rather than typed. The OK line below used to end with a literal
# "6 documents, 8 claims" -- a hand count in the guard's own verdict, and it went wrong the moment a
# row was added (2026-09-13, review ledger V178). A guard that hand-counts its own population is
# committing the defect it exists to find, one level up.
claims=0
docs_seen=""
check_doc() {
  claims=$((claims + 1))
  case " $docs_seen " in
    *" $1 "*) ;;
    *) docs_seen="$docs_seen $1" ;;
  esac
  doc="$1"; marker="$2"; occurrence="${3:-0}"
  if [ ! -f "$doc" ]; then
    err "$doc is missing — it enumerated the shipped-off set, so either restore it or drop its row here"
    return
  fi

  # ONE awk pass, not one grep per id. It finds the paragraph containing the marker (blank-line
  # delimited for prose; for a comment block like `config-template.jsonc` that is the run of comment
  # lines around the claim — both collapse to "the lines a reader reads as one statement") and prints
  # which catalogued ids it names. The per-id loop this replaces spawned ~360 processes and took over
  # four minutes on Git Bash; a guard that slow gets taken out of the pre-commit hook, which is the same
  # outcome as not writing it.
  #
  # Ids are matched LONGEST FIRST and consumed, so an id that contains another as a substring cannot
  # report both. Nothing in the shipped-off set overlaps today, but the universe holds pairs that do
  # (`schema-usage` inside no other, but `unreachable` sits inside no id only by luck), and an
  # over-report here would fire the "names ids that do NOT ship off" arm — a false red on the direction
  # that is meant to catch a real removal.
  named="$(awk -v m="$marker" -v ids="$native_ids" -v occ="$occurrence" '
    function boundary(line) { return line !~ /[^][:space:]{}(),[]/ }
    # A translation key opening a template literal (`ko: `", "en: `") STARTS a paragraph rather than
    # ending one: the opener line carries the first words of its own claim, so it must be inside the
    # paragraph, and the NEXT opener must be outside it. Without this the two languages of a site page
    # sit in one paragraph and the guard vouches only for the PAIR — an id dropped from one language
    # while the other still names it reads as green, which is the realistic regression (someone edits
    # the English and not the Korean). Measured 2026-09-05: deleting `unreachable` from the English
    # claim of the reference page left this guard green.
    function opener(line) { return line ~ /^[[:space:]]*[a-z][a-z]:[[:space:]]*`/ }
    BEGIN {
      n = split(ids, raw, "\n")
      # Insertion-sort the ids by descending length so the longest match wins.
      for (i = 1; i <= n; i++) {
        if (raw[i] == "") continue
        pos = ++cnt
        while (pos > 1 && length(id[pos - 1]) < length(raw[i])) { id[pos] = id[pos - 1]; pos-- }
        id[pos] = raw[i]
      }
    }
    { buf[NR] = $0 }
    # `occ` selects WHICH occurrence of the marker anchors the paragraph: 0 (the default, and what
    # every prose document uses) means the last, any n>0 means the nth. A bilingual page states the
    # same claim once per language with the SAME ascii marker, so "1" is the Korean lane and "2" the
    # English one — and the marker stays ascii, which `check-english-source.sh` requires of this file.
    index($0, m) { seen++; if (occ + 0 == 0 || seen == occ + 0) hit = NR }
    END {
      if (!hit) exit 1
      # A paragraph ends at a BOUNDARY line: blank (prose and comment blocks) or brackets-only
      # (`],`, `{`, `},` — a source file where the claim is one entry of a literal, as
      # `site-src/content/usage.mjs` is). Without the second case that file has no blank line inside its
      # whole content array, so the "paragraph" was the entire array and the guard reported every id the
      # page mentions anywhere as a stale shipped-off claim. Markdown prose never has a brackets-only
      # line, so the widening costs the other five documents nothing.
      start = hit; stop = hit
      while (start > 1  && !boundary(buf[start - 1]) && !opener(buf[start]))    start--
      while (stop  < NR && !boundary(buf[stop  + 1]) && !opener(buf[stop + 1])) stop++
      para = ""
      for (i = start; i <= stop; i++) para = para buf[i] "\n"
      for (i = 1; i <= cnt; i++) {
        if (index(para, id[i]) == 0) continue
        print id[i]
        gsub(id[i], "", para)          # consume, so a shorter contained id is not also reported
      }
    }
  ' "$doc" | sort -u || true)"

  if [ -z "$named" ] && ! grep -qF -- "$marker" "$doc"; then
    err "$doc no longer contains its marker (\"$marker\"). Either the claim was reworded — re-point the"
    err "  marker in this guard — or it was deleted, in which case drop the row. A marker that matches"
    err "  nothing must never pass silently: that is the shape in which this guard stops guarding."
    return
  fi

  missing="$(comm -23 <(printf '%s\n' "$default_off") <(printf '%s\n' "$named") || true)"
  extra="$(comm -13 <(printf '%s\n' "$default_off") <(printf '%s\n' "$named") || true)"

  if [ -n "$missing" ]; then
    err "$doc's shipped-off paragraph does not name: $(printf '%s' "$missing" | tr '\n' ' ')"
    err "  A reader configuring against this file is never told those analyses do not run."
  fi
  if [ -n "$extra" ]; then
    err "$doc's shipped-off paragraph names ids that do NOT ship off: $(printf '%s' "$extra" | tr '\n' ' ')"
    err "  Telling a user to opt into an analysis that is already on is the failure a removal from"
    err "  DEFAULT_OFF leaves behind, and it is invisible in a run."
  fi
}

check_doc "crates/config/src/config-template.jsonc" "THREE ANALYSES SHIP OFF"
check_doc "docs/rules/catalog.md"                   "ids below ship OFF"
check_doc "docs/getting-started.md"                 "analyses ship OFF, and this is where you turn one on"
check_doc "VERSIONING.md"                           "Whether a rule runs BY DEFAULT is not a settled property"
# CHANGELOG.md is deliberately NOT a row here (2026-09-23, review ledger V293). Its copy of this claim
# lived in `## Unreleased`, and cutting a release ROLLS THAT SECTION AWAY -- the file says so in its own
# words, and v0.33.0's tagged copy proves it: an 11-line Unreleased holding only the two standing
# paragraphs. So the pin was durable and its surface was not, and the first release to fold after the pin
# was added is the release that discovered it: the fold deleted the marker and this guard failed the
# commit that performed the documented ceremony. A changelog entry describes a release; it is not a
# standing statement of what ships off today, which is what every other row below is.
# The WIRE copy, added 2026-09-13 (review ledger V178). This constant ships inside every analyze-shaped
# reply and names the same three ids as the documents above -- and it was the one copy no guard read.
# TWO guards each watched part of this claim: the legend file has a test forbidding hand COUNTS in one
# constant, and this guard watched the id LIST in five documents. The sentence carrying both fell
# between them. A claim with two guards and no coverage is worse than one with none, because each
# guard makes the other look unnecessary.
check_doc "crates/facade/src/output/native_analyses_legend.rs" "61.7%"
# The two site pages state their claim ONCE PER LANGUAGE, and each language is its own paragraph (see
# `opener` above), so each needs its own row — a language with no row is a language with no guard. The
# marker is the claim's own measurement, identical in both lanes, and the third column picks which
# occurrence anchors the paragraph: 1 = the Korean lane (it comes first), 2 = the English one.
check_doc "site-src/content/usage.mjs"              "61.7%" 1
check_doc "site-src/content/usage.mjs"              "61.7%" 2
check_doc "site-src/content/reference.page.mjs"    "61.7%" 1
check_doc "site-src/content/reference.page.mjs"    "61.7%" 2
# The CONTRACT copy, added 2026-09-13 (review ledger V201). `surface-parity.json` states the same
# three ids and the same 61.7% while describing what `shippedOff` means, and it was the ninth claim
# site with no guard on it -- found by the completeness sweep below rather than by anyone reading.
check_doc "docs/contracts/surface-parity.json"     "61.7%"
# The SITE copy, added 2026-09-13 (ledger V206). The public rules table listed all three ids at `info`
# and carried no shipped-off wording at all, which is the one page a reader treats as "what zzop checks".
# Both language lanes, for the reason the two rows below carry: a language with no row is a language with
# no guard.
check_doc "site-src/content/rules.page.mjs"         "61.7%" 1
check_doc "site-src/content/rules.page.mjs"         "61.7%" 2

# ## COMPLETENESS: the roster above is hand-kept, so something has to notice a claim that never
# joined it.
#
# Every row above pairs a FILE with the phrase that anchors the claim inside it, which is why the
# roster cannot simply be a glob -- the anchors are per-document knowledge. What CAN be derived is
# whether any document in the population makes this claim WITHOUT being listed, and that is this
# sweep. It is the gap that hid `surface-parity.json`: a hand roster fails by omission, silently,
# and the guard reports OK over the documents it happens to know.
#
# The population is bounded to where a USER-FACING claim can live -- published docs, the contract
# documents, the site content sources, the starter config and the facade output constants. It is
# deliberately NOT all of `git ls-files`: the three ids appear in 65 tracked files, nearly all of
# them engine source that IMPLEMENTS the analyses or fixture configs that switch them on, and a
# needle that wide reports noise instead of a claim (the same trade `check-shell-mute-floor.sh`
# records: "the inverse needle is the honest one and it is unusable; this one is narrow and true").
#
# The needle is "names all three ids". On this population that is 10 files, 9 of them real claim
# sites; the tenth is exempted by name below, with its reason, because a check with no channel for a
# legitimate exception reports a VIOLATION rather than "unknown" and violations get "repaired".
# ## The population, and a floor that watches THE NEEDLE rather than the repo
#
# Built class by class so the floor can assert what each class CONTRIBUTED. The first version of this
# floor asked git whether each pathspec still matched anything — which is a question about the repo, not
# about this enumeration, so narrowing the list right here left it perfectly green (drilled 2026-09-13,
# ledger V213). A floor has to read the same expression it is protecting, or it is guarding something
# else that happens to be nearby.
#
# Derived, not a threshold: this repo cannot be in a state with no markdown, no contract documents, no
# site content, no starter config and no facade output constants. A zero from any one class is a broken
# needle, never a fact about the tree — and a zero-only floor on the TOTAL cannot see it, because the
# other four keep the total comfortably non-zero. That shape has now been found three times in this
# fleet (V205, V212, and here), so it is the rule rather than the exception.
claim_population=""
for claim_spec in '*.md' 'docs/contracts/*.json' 'site-src/content/*.mjs' \
  'crates/config/src/config-template.jsonc' 'crates/facade/src/output/*.rs'; do
  claim_class="$(git ls-files -- "$claim_spec" || true)"
  if [ -z "$claim_class" ]; then
    err "the claim-completeness pathspec '$claim_spec' contributed NOTHING to the population."
    err "  Every other class would still report a healthy total while this one stopped being read."
    fail=1
  fi
  claim_population="$claim_population$claim_class
"
done
claim_population="$(printf '%s' "$claim_population" | sort -u | grep -v '^$' || true)"
if [ -z "$claim_population" ]; then
  err "the claim-completeness population enumerated ZERO files -- the needle is broken, not the tree"
  fail=1
fi

# NOT a claim site, and the reason it names these ids anyway:
#   rules/README.md -- lists what the `rules-graph` crate CONTAINS ("Examples: rules-graph (circular,
#   unreachable, dead-candidates, unimported-export)"). It is a membership list for a crate, says
#   nothing about defaults, and would be wrong to carry a shipped-off paragraph.
claim_exempt="rules/README.md"

while IFS= read -r cand; do
  [ -n "$cand" ] || continue
  [ -f "$cand" ] || continue
  case " $claim_exempt " in *" $cand "*) continue ;; esac
  names_all=1
  for id in $default_off; do
    grep -q -- "$id" "$cand" 2>/dev/null || { names_all=0; break; }
  done
  [ "$names_all" -eq 1 ] || continue
  case " $docs_seen " in
    *" $cand "*) ;;
    *)
      err "$cand names every shipped-off id and is NOT in this guard's roster."
      err "  Either it makes the claim -- add a check_doc row with the phrase that anchors it -- or"
      err "  it merely lists the ids, in which case add it to \`claim_exempt\` WITH THE REASON."
      fail=1
      ;;
  esac
done < <(printf '%s\n' "$claim_population")

if [ "$fail" -ne 0 ]; then
  printf 'check-shipped-off-sync: FAILED — the engine ships { %s } off.\n' \
    "$(printf '%s' "$default_off" | tr '\n' ' ')" >&2
  exit 1
fi

doc_count="$(printf '%s' "$docs_seen" | tr ' ' '\n' | grep -c . || true)"
printf 'check-shipped-off-sync: OK (%s ids, %s documents, %s claims)\n' "$count" "$doc_count" "$claims"
