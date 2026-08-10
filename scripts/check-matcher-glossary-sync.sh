#!/usr/bin/env bash
# Matcher-glossary sync guard — binds every HAND-WRITTEN list of DSL matcher shapes to the `Matcher`
# enum, which is the only place the list really exists.
#
# ## The subjects, and what each one claims
#   * `crates/core/src/dsl/def/matcher.rs` — SSOT. `#[serde(tag = "type", rename_all = "kebab-case")]`
#     means the wire spelling of every variant is derived, not authored: `LiteralScan` -> `literal-scan`.
#   * `site/rules.html`, `<section id="matchers">` — the PUBLIC glossary table. One row per shape, and
#     the row is what a reader outside this repo learns the vocabulary from.
#   * `docs/rules/dsl-reference.md` — one `### \`<kebab>\` (\`<Variant>\`)` section per shape. That
#     heading spells BOTH names, so it is checked as a pair: a heading whose two halves disagree is a
#     rename that landed on one of them.
#   * `crates/core/src/dsl/tests_diagnostics.rs` + `tests_diagnostics/family_cases.rs` — not prose: the
#     skip-diagnostic TESTS themselves, one `#[test] fn <family>_…` per shape (`<family>` = the variant in
#     snake_case). That module header used to carry the list AND the count ("Pins six matcher families
#     (…)"), which is the same second copy in a different costume; it now states the quantifier and the
#     check below is what makes the quantifier true. One direction only, deliberately: a leftover test for
#     a retired family is dead code the compiler and a reviewer both see, while a NEW family with no
#     skip-diagnostic test is invisible — and that is the direction that actually shipped (literal-scan,
#     closed 2026-08-08 after five-of-six had stood behind the word "every").
#
# ## Why
# `check-rules-catalog-sync.sh` parses `site/rules.html`, but its needles require three `<code>` cells
# per row (id, severity, matcher type) and a glossary row has two — so the glossary sat OUTSIDE every
# check that file has ever had, and `rules/README.md` said so in plain text rather than fixing it. The
# cost was measured on 2026-08-09: the table had FOUR rows while the enum had SIX. `call-scan` and
# `literal-scan` shipped, the public page never learned about them, and the two rows were added BY HAND
# in the batch that noticed. Nothing would have noticed the seventh.
#
# ## What is NOT checked here, on purpose
# The `<td>` PROSE of a glossary row ("Operates over" / "Use it for") is hand-written and stays that
# way: it is a definition, not a token, and there is nothing in the enum to compare it against. This
# guard pins the SET and the SPELLING of the shapes — the axis on which the table was actually wrong.
#
# ## Copy vs subset claim — the line this guard draws
# Prose across docs/ and crates/ names SOME of the shapes on purpose ("only symbol-scan/io-scan rules can
# fire in envelope mode"). One question separates the two: WOULD A SEVENTH VARIANT MAKE THE SENTENCE FALSE?
# If yes it is a COPY and needs a check; if the sentence stays true, it is a SUBSET CLAIM about behavior and
# a set comparison would false-red it. About twenty subset claims exist and none is in scope here.
#
# The copies deliberately NOT checked by this script, and why:
#   * `crates/summary/src/contracts.rs` — two `description:` strings spell the whole set, and `include_str!`
#     bakes them into the binary, so a stale one freezes into every prebuilt release. They are pinned in
#     Rust instead: `zzop-summary`'s `every_matcher_kind_appears_in_the_descriptions_that_claim_to_list_them_all`
#     derives the same set from the same enum and carries its own extraction floor. Not re-checked here —
#     two guards over one string is one more thing to keep honest, and that test already fails the build.
#   * `docs/rules/dsl-reference.md` had a second copy inline at the top of `## Matchers`. It was DELETED
#     rather than pinned: the `###` sections directly beneath it already are the list and are checked above,
#     so the "a needle matches exactly one wording" problem that sentence posed no longer needs solving.
#
# No deps beyond grep/awk. Exit 1 on any violation.
set -euo pipefail
cd "$(dirname "$0")/.."
export LC_ALL=C

ENUM=crates/core/src/dsl/def/matcher.rs
SITE=site/rules.html
REFERENCE=docs/rules/dsl-reference.md
DIAG=crates/core/src/dsl/tests_diagnostics.rs
DIAG_FAMILIES=crates/core/src/dsl/tests_diagnostics/family_cases.rs

for f in "$ENUM" "$SITE" "$REFERENCE" "$DIAG" "$DIAG_FAMILIES"; do
  [ -f "$f" ] || { echo "matcher-glossary guard: missing $f" >&2; exit 1; }
done

fail=0

# --- the SSOT, and the serde contract that makes its spelling derivable -----------------------------
# The kebab spelling below is COMPUTED from the variant name. That computation is only correct while
# the enum really carries `rename_all = "kebab-case"`, so the attribute is asserted rather than
# assumed: a switch to snake_case would leave this guard comparing a spelling nothing serves, and
# comparing two wrong lists that agree is the failure mode of every second copy.
# Checked on the two lines DIRECTLY above `pub enum Matcher {`, not anywhere in the file: a sibling
# type carrying the same attribute would otherwise vouch for an enum that lost it.
if ! awk '
  /^pub enum Matcher[[:space:]]*\{/ {
    found = 1
    if (p1 ~ /rename_all = "kebab-case"/ || p2 ~ /rename_all = "kebab-case"/) ok = 1
  }
  { p2 = p1; p1 = $0 }
  END { exit((found && ok) ? 0 : 1) }
' "$ENUM"; then
  echo "matcher-glossary guard: FAILED -- the two lines above \`pub enum Matcher {\` in $ENUM no longer" >&2
  echo "  carry rename_all = \"kebab-case\" (or the enum itself is gone). Every wire spelling this guard" >&2
  echo "  DERIVES from a variant name assumes that attribute; without it the comparison below is between" >&2
  echo "  the enum and a spelling nothing accepts." >&2
  exit 1
fi

# `<Variant>\t<kebab>` for every variant of `pub enum Matcher`, brace-scoped so a sibling enum in the
# same file cannot contribute rows.
variants="$(awk '
  /^pub enum Matcher[[:space:]]*\{/ { inenum = 1; next }
  inenum && /^\}/ { inenum = 0 }
  inenum && /^    [A-Z][A-Za-z0-9]*\(/ {
    v = $0
    sub(/^    /, "", v); sub(/\(.*$/, "", v)
    out = ""
    for (i = 1; i <= length(v); i++) {
      ch = substr(v, i, 1)
      if (ch ~ /[A-Z]/) { if (i > 1) out = out "-"; out = out tolower(ch) }
      else out = out ch
    }
    print v "\t" out
  }
' "$ENUM")"

variant_count=$(printf '%s\n' "$variants" | grep -c . || true)
if [ "$variant_count" -eq 0 ]; then
  echo "matcher-glossary guard: FAILED -- extracted ZERO variants from \`pub enum Matcher\` in $ENUM." >&2
  echo "  Every comparison below is a set difference, and the difference of two empty sets is empty, so" >&2
  echo "  a collapsed extraction here reads as perfect agreement between the enum and both documents." >&2
  exit 1
fi

# --- the public glossary table ----------------------------------------------------------------------
# Scoped to `<section id="matchers">` .. `</section>`: the page carries hundreds of other rows, and a
# needle that read them all would compare rule ids against matcher shapes.
site_rows="$(awk '
  /<section id="matchers">/ { insec = 1 }
  insec && /<\/section>/ { insec = 0 }
  insec && /<tr><td><code>[a-z][a-z-]*<\/code><\/td>/ {
    v = $0
    sub(/^.*<tr><td><code>/, "", v); sub(/<\/code>.*$/, "", v)
    print v
  }
' "$SITE")"
site_count=$(printf '%s\n' "$site_rows" | grep -c . || true)

# --- the reference sections ---------------------------------------------------------------------------
ref_rows="$(awk '
  /^### `[a-z][a-z-]*` \(`[A-Z][A-Za-z0-9]*`\)/ {
    k = $0; sub(/^### `/, "", k); sub(/`.*$/, "", k)
    v = $0; sub(/^[^(]*\(`/, "", v); sub(/`\).*$/, "", v)
    print v "\t" k
  }
' "$REFERENCE")"
ref_count=$(printf '%s\n' "$ref_rows" | grep -c . || true)

# --- the skip-diagnostic tests ------------------------------------------------------------------------
# `#[test] fn <name>` from both files, gated on the ATTRIBUTE line so the module's helpers (`fn eval`,
# `fn file`, `fn assert_actionable`) cannot vouch for a family they merely support.
diag_tests="$(cat "$DIAG" "$DIAG_FAMILIES" | awk '
  /^#\[test\]/ { pending = 1; next }
  pending && /^fn [a-z0-9_]+\(/ { n = $2; sub(/\(.*$/, "", n); print n; pending = 0; next }
  pending && /^[[:space:]]*$/ { next }
  { pending = 0 }
')"
diag_count=$(printf '%s\n' "$diag_tests" | grep -c . || true)

for pair in "site/rules.html <section id=\"matchers\"> rows|$site_count|the glossary table layout changed and this guard now compares the enum against nothing" \
            "docs/rules/dsl-reference.md matcher sections|$ref_count|the '### \`<kebab>\` (\`<Variant>\`)' heading shape changed" \
            "skip-diagnostic test functions|$diag_count|the '#[test]' + 'fn <name>(' shape in $DIAG / $DIAG_FAMILIES changed"; do
  label="${pair%%|*}"; rest="${pair#*|}"; n="${rest%%|*}"; why="${rest#*|}"
  if [ "$n" -eq 0 ]; then
    echo "matcher-glossary guard: FAILED -- extracted 0 $label. Probably $why." >&2
    echo "  A comparison against an empty set passes vacuously, which is the one way a sync guard can" >&2
    echo "  certify two surfaces it never read. Fix the extraction, not this assertion." >&2
    fail=1
  fi
done
[ "$fail" -eq 0 ] || exit 1

# --- the comparisons: both DOCUMENT surfaces both ways, the test coverage one way ---------------------
# Both directions on the documents, always: "the enum has a shape the page never learned" and "the page
# names a shape the enum retired" are different defects with the same fix window, and a one-way check is
# how the second one ships for two weeks. The test-coverage check is one-way on purpose, for the reason
# in its subject bullet at the top of this file.
report="$(awk -v variants="$variants" -v siterows="$site_rows" -v refrows="$ref_rows" -v diagtests="$diag_tests" '
BEGIN {
  n = split(variants, a, "\n")
  for (i = 1; i <= n; i++) {
    if (a[i] == "") continue
    t = index(a[i], "\t")
    v = substr(a[i], 1, t - 1); k = substr(a[i], t + 1)
    kebab[k] = v; pascal[v] = k; nkebab++
  }
  n = split(siterows, b, "\n")
  for (i = 1; i <= n; i++) {
    if (b[i] == "") continue
    if (b[i] in siteseen) print "DUP\tsite/rules.html\t" b[i]
    siteseen[b[i]] = 1
    if (!(b[i] in kebab)) print "EXTRA\tsite/rules.html glossary row\t" b[i]
  }
  n = split(refrows, c, "\n")
  for (i = 1; i <= n; i++) {
    if (c[i] == "") continue
    t = index(c[i], "\t")
    v = substr(c[i], 1, t - 1); k = substr(c[i], t + 1)
    if (k in refseen) print "DUP\tdocs/rules/dsl-reference.md\t" k
    refseen[k] = 1
    if (!(v in pascal)) { print "EXTRA\tdsl-reference.md section (variant)\t" v " (`" k "`)"; continue }
    # The heading spells both halves, so it is checked as a PAIR: `### `io-scan` (`CallScan`)` names two
    # real things and still lies about which is which.
    if (pascal[v] != k) print "PAIR\tdsl-reference.md section\t" v " is serialized as `" pascal[v] "`, heading says `" k "`"
  }
  n = split(diagtests, dt, "\n")
  for (i = 1; i <= n; i++) { if (dt[i] != "") tests[dt[i]] = 1 }
  for (k in kebab) {
    if (!(k in siteseen)) print "MISSING\tsite/rules.html glossary\t" k " (" kebab[k] ")"
    if (!(k in refseen)) print "MISSING\tdocs/rules/dsl-reference.md section\t" k " (" kebab[k] ")"
    # Coverage, not spelling: the snake_case family name must PREFIX some `#[test] fn`. Prefix and not
    # substring, so `every_line_scan_regex_field_reports_its_own_name` cannot stand in for the case that
    # belongs to the family itself.
    snake = k; gsub(/-/, "_", snake)
    covered = 0
    for (tn in tests) if (index(tn, snake "_") == 1) covered = 1
    if (!covered) print "UNTESTED\tthe skip-diagnostic tests\t#[test] fn " snake "_... (" kebab[k] ")"
  }
  print "#count\t" nkebab
}')"

pinned=0
while IFS=$'\t' read -r kind where what; do
  [ -n "$kind" ] || continue
  case "$kind" in
    "#count") pinned="$where"; continue ;;
    MISSING) echo "matcher-glossary guard: $where does not list \`$what\` -- the enum declares it." >&2 ;;
    EXTRA)   echo "matcher-glossary guard: $where names \`$what\`, which is not a \`Matcher\` variant." >&2 ;;
    DUP)     echo "matcher-glossary guard: $where lists \`$what\` twice." >&2 ;;
    PAIR)    echo "matcher-glossary guard: $where -- $what." >&2 ;;
    UNTESTED) echo "matcher-glossary guard: $where carry no \`$what\` -- the enum declares that family and the module header claims every one of them is pinned." >&2 ;;
  esac
  fail=1
done <<< "$report"

if [ "$fail" -ne 0 ]; then
  echo >&2
  echo "matcher-glossary guard: FAILED. A matcher shape is spelled differently, or is missing, on a" >&2
  echo "surface that claims to list them all. The enum in $ENUM is the SSOT and its wire" >&2
  echo "spelling is derived (serde kebab-case) -- edit the documents, never the derivation." >&2
  echo "  * public glossary: $SITE, <section id=\"matchers\">" >&2
  echo "  * field reference: $REFERENCE, one '### \`<kebab>\` (\`<Variant>\`)' section per shape" >&2
  echo "  * skip-diagnostic tests: $DIAG (+ family_cases.rs), one '#[test] fn <family>_...' per shape" >&2
  exit 1
fi

echo "matcher-glossary guard: OK ($pinned Matcher variants; $site_count glossary rows on $SITE and $ref_count sections in $REFERENCE agree with the enum, both directions; every family has its own case among the $diag_count skip-diagnostic tests in $DIAG + family_cases.rs)."
