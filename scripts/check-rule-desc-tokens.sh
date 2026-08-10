#!/usr/bin/env bash
# check-rule-desc-tokens.sh — guards the ONE column check-rules-catalog-sync.sh reads nothing in:
# the Detects/description prose of docs/rules/catalog.md.
#
# That guard pins id / severity / matcher-type / `.rs` path between the two surfaces. Neither it nor
# anything else compares the description against the RULE ITSELF, so a description could name an API
# the matcher has never contained and stay green through every release. It did:
#
#   docs/rules/catalog.md + site/rules.html, `stacktrace-to-response`: the description named
#   `.getMessage()` as one of the patterns the rule matches. The live matcher's `leak` arm is
#   `printStackTrace\s*\(|getStackTrace(?:AsString)?\s*\(` and nothing else, and the rule's OWN message
#   says `.getMessage()` is deliberately NOT flagged. The claim was false against the matcher for as
#   long as the row existed, survived every release, and was found by a human reading the two texts
#   side by side (2026-07-26).
#
# ---------------------------------------------------------------------------------------------------
# THE SUBJECT IS THE CATALOG ONLY NOW — site/rules.html was DROPPED (2026-08-09)
# ---------------------------------------------------------------------------------------------------
# This guard used to scan both surfaces. It cannot any more, because there is no longer a second
# surface to scan: scripts/gen-site-rules.mjs writes every site row from the catalog row
# (`<td>${r.detects}</td>`, gen-site-rules.mjs:213 — the description cell is copied, not restated) and
# check-rules-catalog-sync.sh's check 1 asserts the committed page IS that output. So a site
# description is a pure function of a catalog description, and every finding the site pass could
# produce was the SAME finding the catalog pass already produced one file earlier. Measured before the
# cut: 1194 tokens checked, catalog rows 143, site rows 143, zero findings unique to either.
#
# Watching a derived value is not free, which is the actual reason it is gone rather than merely
# redundant. The site row regex pinned GENERATOR-controlled markup (`<tr><td><code>id</code></td>` and
# a `<code>`-wrapped severity cell). A markup change in the generator would have dropped rows_site
# below nrules and turned this guard RED over a file it does not own, with the fix living in a
# generator this guard says nothing about — a failure report pointing at the wrong file. That is the
# ghost a check over a derived value becomes, and deleting it is the same move check-rules-catalog-sync
# made on the same day for the same reason.
#
# What is NOT lost: the site's own prose (intro paragraphs, matcher glossary, custom-pack example) is
# still hand-written, and it was never in scope here — this guard only ever read RULE ROWS. The
# glossary specifically is now covered by check-matcher-glossary-sync.sh.
#
# ---------------------------------------------------------------------------------------------------
# What is checked, and what is deliberately NOT
# ---------------------------------------------------------------------------------------------------
# NOT checked: whether the description MEANS the same thing as the rule's `message`. A description is a
# human summary; byte-pinning it to the message would be wrong (the summary exists precisely to be
# shorter) and would be routed around within a release. There is no shell question there.
#
# Checked: the one part of a description that is not a summary but a QUOTATION — a backticked token
# that is code. Two invariants, both mechanical:
#
#   A. VOCABULARY. A code-shaped backticked token in a rule's description must appear somewhere in that
#      rule's own JSON — its matcher (with `${fragment}` references expanded, since the real matcher is
#      only visible after expansion) or its `message`. The message counts because descriptions
#      legitimately quote FIX advice, which lives in the message and not in the matcher
#      (`map-async-no-promise-all` tells you to wrap in `Promise.all`; `Promise` appears nowhere in its
#      matcher, correctly). Matching is case-sensitive: the packs are. Matcher-ONLY was measured first
#      and rejected: 127 misses across the two surfaces, nearly all of them fix advice of exactly that
#      kind. A guard with that hit rate gets disabled, and it would be wrong on the merits.
#
#   B. POLARITY. Invariant A alone would NOT have caught the defect above — the `stacktrace-to-response`
#      message mentions `.getMessage()`, in order to say it is not flagged, so the token was "present in
#      the message" the whole time. Measured against the pre-fix text, so this is not a hypothesis. So:
#      when a token is ABSENT from the matcher and EVERY occurrence of it in the message sits next to a
#      negator, the description must carry a negator next to it too. The rule's own message is the only
#      thing that can say "this token names something the rule does not do"; a description that repeats
#      the token while dropping the negation converts a documented non-behavior into a claimed behavior.
#      That is exactly the shape of the shipped defect, and exactly the shape of its fix.
#
# A "negation" here is NOT plain English negation — it is a MATCHING-POLARITY denial: a negator
# (`no`/`not`/`NOT`/`never`/`n't`/`without`/`unlike`/`exclud*`/`exempt`) followed, inside the same
# clause, by a verb about FIRING (`fire`/`flagged`/`matched`/`reported`/`triggered`/`detected`/
# `scanned`/`checked`). The window is NEG_WINDOW characters on either side of the token, clipped at
# sentence boundaries — the same bounded-window idiom check-overclaim-prose.sh uses, and for the same
# reason (a sentence-scoped test a script can actually run). Bounded on BOTH sides because the real
# texts put it on both: "with no `AbortController` visible" precedes, "`.getMessage()` is deliberately
# NOT matched" follows.
#
# The narrowing above is not decoration. Each clause of it was added to kill a MEASURED false red on
# this tree, and the plain-negation version had all three:
#   1. `[Nn]o` with no trailing non-letter matched inside "Note" ("Note honestly: ... the check ...")
#      and read an honest sentence as a denial (`stream-open-no-close-in-loop`, `EMFILE`).
#   2. bare `flag`/`check`/`scan` in the verb list matched the NOUNS "no exclusive-create flag" and
#      "the check is stale" (`fs-check-then-use`, `fs.access`/`fs.stat`). Only firing/participle forms
#      survive; `flag` as a noun is far more common in these texts than `flag` as the verb.
#   3. an unclipped window let a denial in the NEXT SENTENCE about a DIFFERENT token vouch for this one
#      ("use `Object.keys(x).length === 0`). Loose `x == []` is deliberately NOT matched" —
#      `always-constant-comparison`). Hence the `. ` clip, which no code token in these texts contains.
#
# ---------------------------------------------------------------------------------------------------
# Which tokens count as code — the narrow end, on purpose
# ---------------------------------------------------------------------------------------------------
# A guard that cries wolf gets disabled, so the token class is the SMALLEST one that contains the
# defect: a bare identifier, optionally `.`-prefixed and/or `.`-joined, optionally with an `@`
# annotation prefix, optionally ending in `(` or `()`, with no other characters at all. Every atom of
# it must be >= 3 characters to be looked up (`.on(`, `.map` and friends carry no evidence — `on` and
# `map` appear in almost any regex — so they are skipped rather than rubber-stamped).
#
# Everything else backticked in a description is deliberately out of scope, by class:
#   * anything with whitespace, a quote, `<`, `>`, `=`, `,`, `[`, `{` or `+` — INVENTED ILLUSTRATIONS
#     (`"<div>" + userVar`, `requireAdmin(handler)`, `value="/debug"`, `Extract<T, '1'>`,
#     `httpOnly: false`). These are hypothetical user code written to show a shape. They are not
#     quotations of the rule and there is nothing in the rule for them to match.
#   * anything with `/` — path illustrations (`src/config/db.ts`, `scripts?/`) and pack-qualified rule
#     cross-references (`sql/nplus1`). Same reason.
#   * `${...}` — a fragment REFERENCE. The fragment name is what the pack file spells; its expansion is
#     what the matcher means. Checking the name against the expanded matcher would fail every time.
#   * tokens with a `-` — bare rule-id cross-references (`cors-reflected-origin-credentials`). Rule ids
#     are already pinned by check-docs-rule-ids.sh and check-rules-catalog-sync.sh.
#   * pure punctuation (`!==`, `%`, `..`) — no identifier to look up.
# Each of these is an exclusion of a CLASS with a reason, not a loosening of the check: within the
# class that is checked, a miss is a failure.
#
# Individual vetted exceptions live in ALLOWLIST below, which is itself checked for stale entries.
#
# ---------------------------------------------------------------------------------------------------
# Coverage: the two row counts must AGREE, and the OK line prints them
# ---------------------------------------------------------------------------------------------------
# Everything above describes what happens to a row that is IN SCOPE. The row regexes (see "pass 2")
# pin the catalog column spellings, and the catalog is hand-maintained (the only generator in this
# chain is scripts/gen-site-rules.mjs, which derives site/rules.html FROM the catalog) — so the way
# this guard fails is not by mis-judging a row, it is by quietly not seeing one a hand edit respelled. Both the END-block assertion and the
# success line therefore carry the census: pack rules and catalog rows. See the END block for
# the measured demonstration (20 rows dropped out of scope while the guard still printed OK).
set -euo pipefail
cd "$(dirname "$0")/.."

CATALOG=docs/rules/catalog.md
SHARED=crates/core/src/dsl/shared_fragments.json

for f in "$CATALOG" "$SHARED"; do
  [ -f "$f" ] || { echo "check-rule-desc-tokens: missing $f" >&2; exit 1; }
done

# `<rule id>|<token>|<why>` — vetted misses. Every entry must still BE a miss (stale entries fail).
ALLOWLIST=(
  "config-file-secret|.yml|the matcher spells this extension \`ya?ml\` (one alternation covering .yaml and .yml), so the literal substring \`yml\` cannot appear; the claim is true, only unmatchable by substring"
  "taint-flow|message|names the rule's own \`message\` FIELD (\"see the rule's own \`message\` for the three documented precision limits\"), a pointer to documentation rather than matched vocabulary"
  "protected-path-no-auth-evidence|views.AdminView|an invented Django handler name, quoted to illustrate a token that would OVER-CLEAR the route if the guard vocabulary were widened; it survives the dotted-identifier shape filter that catches the other illustrations"
  "dev-path-no-guard-hint|views.DebugView|same illustration, same sentence, on the sibling rule"
)

allow_arg=""
for e in "${ALLOWLIST[@]}"; do allow_arg="$allow_arg$e"$'\n'; done

awk -v allow="$allow_arg" -v shared="$SHARED" -v catalog="$CATALOG" '
BEGIN {
  NEG_WINDOW = 90
  # A MATCHING-POLARITY denial, not plain English negation: a negator followed, within the same
  # clause, by a verb about FIRING. See the header for why the plain-negation form was measured and
  # rejected.
  # The trailing [^A-Za-z] after the negator is load-bearing: without it `No` matched inside `Note`
  # ("Note honestly: ... the check ...") and silently read an honest sentence as a denial.
  NEG = "(^|[^A-Za-z])([Nn]o|[Nn]ot|NOT|[Nn]ever|n\047t|without|unlike|exclud[a-z]*|exempt)[^A-Za-z]" \
        "[^.;]{0,45}(fire|fires|firing|flagged|flagging|matched|matches|reported|triggered|detected|scanned|checked)"
  n = split(allow, al, "\n")
  for (i = 1; i <= n; i++) {
    if (al[i] == "") continue
    p = index(al[i], "|"); id = substr(al[i], 1, p - 1)
    rest = substr(al[i], p + 1)
    q = index(rest, "|"); tk = substr(rest, 1, q - 1)
    allowed[id "\t" tk] = 1
  }
}


# The window is clipped at SENTENCE boundaries as well as at NEG_WINDOW characters. Measured need:
# `always-constant-comparison`\047s message says "use `.length === 0` / `Object.keys(x).length === 0`).
# Loose `x == []` is deliberately NOT matched" — the denial is a different sentence about a different
# token, and an unclipped 90-character window read it as denying `Object.keys`. A sentence end is `. `
# (period + space), which no code token in these texts contains; `fs.access` and `.length` survive it.
function clip_after(s,   p) { p = match(s, /\. /); return (p ? substr(s, 1, p - 1) : s) }
function clip_before(s,   last, off) {
  off = 1; last = 0
  while (match(substr(s, off), /\. /)) { last = off + RSTART; off = last + 1 }
  return (last ? substr(s, last) : s)
}

# --- negation test: is any occurrence of `core` in `text` NOT next to a negator? ---
# Returns 1 when at least one occurrence stands un-negated (the token is claimed positively there).
function claimed_positively(text, core,   off, seg, pos, before, after, b0, blen) {
  off = 1
  while (1) {
    seg = substr(text, off)
    pos = index(seg, core)
    if (pos == 0) return 0
    pos = off + pos - 1
    b0 = pos - NEG_WINDOW; if (b0 < 1) b0 = 1
    blen = pos - b0
    before = clip_before(substr(text, b0, blen))
    after = clip_after(substr(text, pos + length(core), NEG_WINDOW))
    if (before !~ NEG && after !~ NEG) return 1
    off = pos + length(core)
  }
}

# ---------------- pass 1a: shared fragments (flat map, no "fragments" wrapper) ----------------
FILENAME == shared {
  if (match($0, /^  "[^"]+"[ \t]*:[ \t]*"/)) { grab_fragment(); }
  next
}

# ---------------- pass 1b: pack JSON ----------------
FILENAME ~ /rules\/dsl\/[^\/]+\/[^\/]+\.json$/ {
  if (FNR == 1) { infrag = 0; cur = "" }
  if ($0 ~ /^  "fragments":[ \t]*\{/) { infrag = 1; next }
  if (infrag) {
    if ($0 ~ /^  \},?[ \t]*$/) { infrag = 0; next }
    if (match($0, /^    "[^"]+"[ \t]*:[ \t]*"/)) grab_fragment()
    next
  }
  if ($0 ~ /^      "id": "/) {
    line = $0; sub(/^      "id": "/, "", line); sub(/".*$/, "", line)
    cur = line; order[++nrules] = cur
    next
  }
  if (cur == "") next
  if ($0 ~ /^      "message": /) {
    line = $0; sub(/^      "message": "/, "", line); sub(/",?[ \t]*$/, "", line)
    msg[cur] = line
    next
  }
  mat[cur] = mat[cur] " " $0
  next
}

function grab_fragment(   q1, rest, q2, name, val) {
  q1 = index($0, "\""); rest = substr($0, q1 + 1)
  q2 = index(rest, "\""); name = substr(rest, 1, q2 - 1)
  val = substr(rest, q2 + 1)
  sub(/^[ \t]*:[ \t]*"/, "", val); sub(/",?[ \t]*$/, "", val)
  frag[name] = val
}

# ---------------- pass 2: the two description surfaces ----------------
FILENAME == catalog {
  if ($0 !~ /^\| `[a-z0-9][a-z0-9\/_-]*` \| [a-z]+ \| [a-z][a-z-]*[a-z] \| /) next
  line = $0
  sub(/^\| `/, "", line)
  id = line; sub(/`.*$/, "", id)
  sub(/^[^|]*\| *[^|]*\| *[^|]*\| */, "", line)
  rows_catalog++
  scan_cell(FILENAME ":" FNR, id, line)
  next
}

function scan_cell(loc, id, cell,   off, pos, tok, plain) {
  plain = cell
  off = 1
  while (match(substr(plain, off), /`[^`]+`/)) {
    pos = off + RSTART - 1
    tok = substr(plain, pos + 1, RLENGTH - 2)
    off = pos + RLENGTH
    judge(loc, id, tok, plain, pos)
  }
}

function judge(loc, id, tok, cell, pos,   core, a, rest, blob, b0, blen, before, after, negd_desc) {
  # --- token class (see header): bare/dotted identifier, optional @ or . prefix, optional ( or () ---
  if (tok !~ /^[.@]?[A-Za-z_$][A-Za-z0-9_$]*(\.[A-Za-z_$][A-Za-z0-9_$]*)*(\(\)?)?$/) return
  if (!(id in mat)) { print "check-rule-desc-tokens: " loc ": rule id `" id "` is not in any pack" > "/dev/stderr"; fail = 1; return }

  checked++
  # Every atom >= 3 chars must be findable; shorter atoms carry no evidence.
  rest = tok; miss = ""
  while (match(rest, /[A-Za-z_$][A-Za-z0-9_$]*/)) {
    a = substr(rest, RSTART, RLENGTH)
    rest = substr(rest, RSTART + RLENGTH)
    if (length(a) < 3) continue
    if (index(expanded(id), a) == 0 && index(msg[id], a) == 0) miss = miss (miss == "" ? "" : ",") a
  }
  if (miss != "") {
    if ((id "\t" tok) in allowed) { hit[id "\t" tok] = 1; return }
    print "check-rule-desc-tokens: " loc ": rule `" id "` — description quotes `" tok "` but `" miss \
          "` appears in neither its matcher (fragments expanded) nor its message." > "/dev/stderr"
    fail = 1
    return
  }

  # --- invariant B: polarity, only for tokens the MATCHER does not contain ---
  core = tok; sub(/^[.@]/, "", core); sub(/\(\)?$/, "", core)
  if (length(core) < 3) return
  if (index(expanded(id), core) > 0) return          # the matcher has it: nothing to disagree about
  if (index(msg[id], core) == 0) return              # reached via a different atom; A already passed it
  if (claimed_positively(msg[id], core)) return      # the message claims it positively too
  # message mentions it ONLY under negation -> the description must negate it as well
  b0 = pos - NEG_WINDOW; if (b0 < 1) b0 = 1
  blen = pos - b0
  before = clip_before(substr(cell, b0, blen))
  after = clip_after(substr(cell, pos + length(tok) + 2, NEG_WINDOW))
  negd_desc = (before ~ NEG || after ~ NEG)
  if (negd_desc) return
  if ((id "\t" tok) in allowed) { hit[id "\t" tok] = 1; return }
  print "check-rule-desc-tokens: " loc ": rule `" id "` — description presents `" tok "` as matched" \
        " vocabulary, but it is absent from the matcher and the rule\047s own message mentions it ONLY" \
        " under a negation (\"not\"/\"never\"/\"excluding\"/...). Either the matcher gained it and the" \
        " message is stale, or the description dropped the negation." > "/dev/stderr"
  fail = 1
}

function expanded(id,   m, guard, nm) {
  if (id in expcache) return expcache[id]
  m = mat[id]; guard = 0
  while (match(m, /\$\{[A-Za-z0-9_-]+\}/) && guard++ < 64) {
    nm = substr(m, RSTART + 2, RLENGTH - 3)
    m = substr(m, 1, RSTART - 1) (nm in frag ? frag[nm] : "\001UNKNOWN-FRAGMENT\001") substr(m, RSTART + RLENGTH)
  }
  expcache[id] = m
  return m
}

END {
  # Silent-failure assertions. Every stage of this guard is a regex over a hand-maintained layout, so
  # the cheapest way for it to become useless is to keep exiting 0 while parsing nothing at all.
  if (nrules == 0) { print "check-rule-desc-tokens: parsed 0 rules from rules/dsl/*/*.json — the pack layout this guard reads has changed." > "/dev/stderr"; fail = 1 }
  if (rows_catalog == 0) { print "check-rule-desc-tokens: matched 0 DSL rows in the catalog — its table layout has changed." > "/dev/stderr"; fail = 1 }

  # ...and the cheaper way for it to become PARTLY useless is to keep parsing SOME rows. The three
  # nonzero tests above are all-or-nothing, and the real failure is not all-or-nothing: both row
  # regexes hardcode the column spellings (severity `[a-z]+`, matcher type `[a-z][a-z-]*[a-z]`) of a
  # hand-maintained catalog, so one formatting edit can drop a subset of rows out of scope while the
  # rest keep flowing through. Demonstrated 2026-07-26: capitalizing the severity column on the 20 `info`
  # rows took rows_catalog from 136 to 116 and the checked-token count from 814 down, and the guard
  # still printed OK. Same class as the 22 zero-byte artifacts that read as total success.
  #
  # The two counts must AGREE, not merely be nonzero. Equality is free rather than lucky: every DSL
  # rule declared in a pack is required to have a catalog row (check-docs-rule-ids.sh), so any
  # inequality is a rule that lost its catalog row, or a row the regex here stopped recognizing.
  # Both are the same defect from here:
  # rows outside the count are rows outside the CHECK.
  # (No apostrophes anywhere in this awk program: it is single-quoted in sh -- same constraint the
  # check-guards-wired.sh awk header records.)
  if (nrules != rows_catalog) {
    print "check-rule-desc-tokens: row census disagrees — pack rules=" nrules \
          ", catalog rows=" rows_catalog "." > "/dev/stderr"
    print "  Every DSL rule must have exactly one catalog row, so these two are the same number. A" > "/dev/stderr"
    print "  shortfall means rows silently fell OUT OF SCOPE of this guard (most likely a generator" > "/dev/stderr"
    print "  change to a column the row regex pins: severity is matched as [a-z]+ and matcher type as" > "/dev/stderr"
    print "  [a-z][a-z-]*[a-z], both lowercase). An excess means the catalog grew rows" > "/dev/stderr"
    print "  the packs do not declare. Either way the descriptions in the missing rows are unchecked," > "/dev/stderr"
    print "  which is exactly the state this guard exists to prevent." > "/dev/stderr"
    fail = 1
  }
  for (i = 1; i <= nrules; i++) {
    if (msg[order[i]] == "") {
      print "check-rule-desc-tokens: rule `" order[i] "` has no single-line \"message\" — this guard reads" \
            " the message as one line and would silently vouch for tokens it never saw." > "/dev/stderr"
      fail = 1
    }
  }
  for (k in allowed) {
    if (!(k in hit)) {
      split(k, kk, "\t")
      print "check-rule-desc-tokens: stale ALLOWLIST entry — `" kk[2] "` on rule `" kk[1] "` is no" \
            " longer a miss. Delete the entry (the vetted exception it records is gone)." > "/dev/stderr"
      fail = 1
    }
  }
  if (fail) {
    print "" > "/dev/stderr"
    print "check-rule-desc-tokens: FAILED — a description in docs/rules/catalog.md" > "/dev/stderr"
    print "quotes code the rule does not contain. Fix the prose to match the live rule, or (if the" > "/dev/stderr"
    print "quotation is legitimate and unmatchable by substring) add a vetted ALLOWLIST entry." > "/dev/stderr"
    exit 1
  }
  printf "check-rule-desc-tokens: OK (%d code-shaped description tokens vouched by their rule across %d DSL rules; rows in scope: catalog %d; %d vetted exceptions)\n", checked, nrules, rows_catalog, length(allowed)
}
' "$SHARED" rules/dsl/*/*.json "$CATALOG"
