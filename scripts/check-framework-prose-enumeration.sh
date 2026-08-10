#!/usr/bin/env bash
# Framework-prose enumeration guard — fails when a parser's DOC PROSE lists SOME of the frameworks its
# `FRAMEWORK_RECOGNIZERS` const declares.
#
# ## The invariant, in one line
# A documentation paragraph may name TWO OR MORE of a parser crate's recognized frameworks only if it
# names ALL of them. A partial list is the defect; a complete list is a second copy this guard keeps
# honest; naming one framework in passing is not a list at all.
#
# ## Why it exists
# `FRAMEWORK_RECOGNIZERS` in each `parser/*/src/lib.rs` is the machine-verified answer to "which
# frameworks does this crate recognize" (aggregated by `zzop_engine::framework_recognizers`, checked
# against the io the adapters really build by the engine's `rule_contracts::recognizer_channels` and
# `recognizer_drift` tests). Those tests read the CONST ONLY. The crate-root doc beside it, and
# `parser/README.md` above it, are prose — and prose that re-lists the same names drifts silently,
# which is not a hypothesis. Measured 2026-08-08, before that prose was converted to pointers:
#   * parser-typescript's `## 2-layer layout` named 5 frameworks; the const declared 18 rows.
#   * parser-python-3's named 5; the const declared 8 rows.
#   * parser-go's named 3 of 4 (silent on `gorm`, whose adapter the crate re-exported two screens down).
#   * parser-java-21's named four producers and was silent on the whole Spring Security channel.
#   * parser/README.md was stale in four rows at once.
# Four of those were repaired by hand in that batch. Nothing stopped the fifth, or the next one: the
# repair was a comment saying "deliberately not listed here", and a comment is not a check.
#
# ## The check, mechanically
#   1. Names come from the const: every `framework: "<name>"` value in `parser/<crate>/src/lib.rs`,
#      de-duplicated. Never a hand list here — the subject set is derived from the same literal the
#      engine tests read, so a new recognizer is in scope the day it lands.
#   2. Prose is split into PARAGRAPHS (a maximal run of non-blank lines). For a crate lib.rs the prose
#      is its crate-root doc comment (the `//!` lines) and the names checked are its OWN crate's. For
#      `parser/README.md` the prose is the whole file and EVERY crate's name set is checked separately,
#      because that file speaks for all of them.
#   3. A paragraph that names >= 2 of one crate's frameworks but not all of them FAILS.
#
# ## Matching, and the false positives it was shaped around
# Names are compared after NORMALIZATION (lowercase; `-` and `_` folded to a space) so `raw-SQL`,
# `raw_sql` and `raw sql` are one name, and only at NON-ALPHANUMERIC BOUNDARIES so `gin` does not match
# inside `engine` — it did, in parser-go's own doc, on the first cut of this script. Longer names are
# consumed FIRST and their span blanked, so `Spring Security` counts once instead of lighting up both
# `spring security` and `spring`; without that, one honest mention in parser-java-21 read as a
# two-item list.
#
# ## There is no exclusion — and the two attempts at one are why
# The first cut skipped any paragraph containing `FRAMEWORK_RECOGNIZERS`, reasoning that a paragraph
# handing the reader to the const is not claiming to be the list. Measured before this guard landed: in
# all four repaired crates that pointer sentence shares a paragraph with the layout bullets, so the
# exemption covered exactly the text this guard exists to watch. Replanting a partial list into
# parser-go`s repaired paragraph exited 0. Narrowing the exemption to the pointer SENTENCE fixed that,
# and then its own both-directions liveness check reported it as dead machinery: the drift-ledger tails
# (the only thing in those sentences that named frameworks) belong in this header, not repeated in five
# docs, and once they moved here no pointer sentence named a framework at all. An exemption with nothing
# to excuse can only hide a real hit, so it is gone. What remains is one rule with no carve-out —
# name all, name one, or name none. The END block still fails if NO doc points at the const, because
# that pointer is the repair this guard was built to protect.
#
# ## Residual, stated rather than discovered later
# A paragraph naming exactly ONE of N frameworks is not caught (one name is not a list, and refusing it
# would red-flag parser/README.md's promotion prose, which names Django and ASP.NET Core as examples of
# the commonality bar). So a crate growing from one recognizer to two while its doc still names the
# first is invisible here; two to three is not. The complementary rule — every multi-recognizer crate
# must carry the pointer token at all — is deliberately NOT enforced here: two crates (parser-csharp,
# parser-rust) carry complete in-prose lists today, and converting those to pointers is a parser-side
# editorial call rather than something a guard may decide.
#
# No deps beyond git/grep/awk. Exit 1 on any violation.
set -euo pipefail
cd "$(dirname "$0")/.."
export LC_ALL=C

README=parser/README.md
[ -f "$README" ] || { echo "framework-prose guard: missing $README" >&2; exit 1; }

# Subject set: DERIVED from the tree, never enumerated here. `git ls-files` plus the
# untracked-but-not-ignored half — the same scope check-guards-wired.sh uses — so a parser crate
# authored in this very commit is covered before its first `git add` rather than after it.
libs="$( { git ls-files -z -- 'parser/*/src/lib.rs'
           git ls-files -z --others --exclude-standard -- 'parser/*/src/lib.rs'
         } | tr '\0' '\n' | sort -u | grep -v '^$' || true)"

lib_count=$(printf '%s\n' "$libs" | grep -c . || true)
if [ "$lib_count" -eq 0 ]; then
  echo "framework-prose guard: FAILED -- enumerated ZERO parser/*/src/lib.rs files. The subject set is" >&2
  echo "  empty, so every paragraph below would be judged against nothing and this guard would print a" >&2
  echo "  clean bill for a fleet it never read." >&2
  exit 1
fi

# One `<crate><TAB><framework name>` line per recognizer row.
names=""
crates_with_names=0
for lib in $libs; do
  crate="$(basename "$(dirname "$(dirname "$lib")")")"
  set +e
  raw="$(grep -oE 'framework:[[:space:]]*"[^"]+"' "$lib")"
  rc=$?
  set -e
  if [ "$rc" -gt 1 ]; then
    echo "framework-prose guard: grep failed reading $lib (exit $rc) -- aborting rather than judge that" >&2
    echo "  file from a scan which never ran." >&2
    exit 1
  fi
  # A crate with no recognizers at all is legal; a crate whose const this extractor can no longer read
  # is not, and the const's own presence is what tells the two apart. Without this branch a renamed
  # field spelling would empty one crate's name set and every paragraph in it would pass vacuously.
  if [ "$rc" -ne 0 ]; then
    if grep -q 'FRAMEWORK_RECOGNIZERS' "$lib"; then
      echo "framework-prose guard: FAILED -- $lib declares FRAMEWORK_RECOGNIZERS but the" >&2
      echo "  framework-name extractor matched ZERO rows in it, so every paragraph in that crate would" >&2
      echo "  be vacuously compliant. Fix the extraction, not this assertion." >&2
      exit 1
    fi
    continue
  fi
  crates_with_names=$((crates_with_names + 1))
  while IFS= read -r r; do
    [ -n "$r" ] || continue
    nm=${r#*\"}
    nm=${nm%\"}
    names="${names}${crate}	${nm}"$'\n'
  done <<< "$raw"
done

if [ "$crates_with_names" -eq 0 ]; then
  echo "framework-prose guard: FAILED -- not one of the $lib_count parser crates yielded a framework" >&2
  echo "  name. This repo ships recognizer-bearing frontends in most of them; a zero here is a broken" >&2
  echo "  extraction, never a clean tree." >&2
  exit 1
fi

# (No apostrophes anywhere inside the awk program below: it is single-quoted in sh, the same constraint
# check-guards-wired.sh and check-rule-desc-tokens.sh record for theirs.)
awk -v namelist="$names" -v readme="$README" '
function norm(s) { s = tolower(s); gsub(/[-_]/, " ", s); gsub(/  +/, " ", s); return s }

# Filesystem paths are not enumerations. Measured on this tree: parser/README.md`s promotion paragraph
# names Django as an example AND points at crates/engine/examples/fastapi_overlay_adapter/main.rs, and
# the directory name made the paragraph read as a two-item list of parser-python-3 frameworks. A token
# carrying both a `/` and a dotted extension is a path, and the same class check-rule-desc-tokens.sh
# excludes from its own token set. A slash-only name (`net/http`) keeps its slash and stays in scope.
function strip_paths(s) { gsub(/[^ \t]*\/[^ \t]*\.[A-Za-z0-9]+/, " ", s); return s }

BEGIN {
  n = split(namelist, rows, "\n")
  for (i = 1; i <= n; i++) {
    if (rows[i] == "") continue
    t = index(rows[i], "\t")
    c = substr(rows[i], 1, t - 1)
    nm = norm(substr(rows[i], t + 1))
    if ((c SUBSEP nm) in seen) continue
    seen[c SUBSEP nm] = 1
    cnt[c]++
    name[c, cnt[c]] = nm
    if (!(c in known)) { known[c] = 1; order[++ncrates] = c }
  }
  # Longest name first, per crate: `spring security` must be consumed before `spring`, or a single
  # mention counts twice and an honest sentence reads as a two-item list.
  for (k = 1; k <= ncrates; k++) {
    c = order[k]
    for (i = 1; i < cnt[c]; i++)
      for (j = i + 1; j <= cnt[c]; j++)
        if (length(name[c, j]) > length(name[c, i])) {
          tmp = name[c, i]; name[c, i] = name[c, j]; name[c, j] = tmp
        }
  }
}

# Count boundary-delimited occurrences of nm in BUF, blanking each one, so a shorter name nested in a
# longer one cannot be counted a second time.
function eat(nm,   c, p, off, before, after, L) {
  L = length(nm); c = 0; off = 1
  while (1) {
    p = index(substr(BUF, off), nm)
    if (p == 0) return c
    p = off + p - 1
    before = (p == 1) ? " " : substr(BUF, p - 1, 1)
    after = substr(BUF, p + L, 1); if (after == "") after = " "
    if (before !~ /[A-Za-z0-9]/ && after !~ /[A-Za-z0-9]/) {
      c++
      BUF = substr(BUF, 1, p - 1) sprintf("%" L "s", "") substr(BUF, p + L)
    }
    off = p + L
  }
}

function judge(c,   i, hits, hit, missing, k) {
  if (cnt[c] < 2) return
  hits = 0; hit = ""; missing = ""
  for (i = 1; i <= cnt[c]; i++) {
    k = eat(name[c, i])
    if (k > 0) { hits++; hit = hit (hit == "" ? "" : ", ") name[c, i] }
    else missing = missing (missing == "" ? "" : ", ") name[c, i]
  }
  if (hits == 0) return
  # ONE name is not a list. Refusing a lone mention would red-flag parser/README.md`s own promotion
  # prose (which names Django and ASP.NET Core as examples of the commonality bar) and every module
  # caption that says which framework it handles. Counted, so the OK line states how much this costs.
  if (hits < 2) { singletons++; return }
  if (hits == cnt[c]) { complete++; return }
  print "framework-prose guard: " PFILE ":" PLINE ": paragraph names " hits " of " cnt[c] \
        " frameworks declared by " c " -- a PARTIAL list." > "/dev/stderr"
  print "    named:   " hit > "/dev/stderr"
  print "    missing: " missing > "/dev/stderr"
  fail = 1
}

# There is NO exclusion. Every paragraph of every subject file is judged, including the ones that hand
# the reader to the const.
#
# The first cut of this guard exempted any paragraph containing the token FRAMEWORK_RECOGNIZERS, on the
# reasoning that a paragraph pointing at the const is not claiming to be the list. Measured 2026-08-09,
# before it ever landed: in all four repaired crates the pointer sentence shares a paragraph with the
# layout bullets, so that exemption covered exactly the text this guard exists to watch — replanting
# "producers emitting for gin and net/http" into parser-go`s repaired paragraph exited 0. Narrowing the
# exemption to the pointer SENTENCE fixed that, and then the both-directions liveness check reported the
# exemption as dead machinery: with the drift-ledger tails moved to this header (see "Why it exists"),
# no pointer sentence names a framework at all. So the exemption had nothing left to excuse and is gone.
# The rule it leaves behind is simpler and strictly stronger: name all, name one, or name none.
function flush(   i) {
  if (PTEXT == "") return
  paragraphs++
  if (index(PTEXT, "FRAMEWORK_RECOGNIZERS") > 0) pointer_paragraphs++
  BUF = norm(strip_paths(PTEXT))
  if (SUBJ == "ALL") { for (i = 1; i <= ncrates; i++) judge(order[i]) }
  else if (SUBJ in known) judge(SUBJ)
  PTEXT = ""
}

FNR == 1 {
  flush()
  PFILE = FILENAME
  files++
  if (FILENAME == readme) SUBJ = "ALL"
  else {
    SUBJ = FILENAME
    sub(/\/src\/lib\.rs$/, "", SUBJ)
    sub(/^.*\//, "", SUBJ)
  }
  prose[FILENAME] = 0
}

{
  line = $0
  if (FILENAME != readme) {
    # Crate-root doc only. A `///` item doc naming the one framework it handles is a caption, not a
    # list of the crate, so it is out of scope by design.
    if (line !~ /^\/\/!/) { flush(); next }
    sub(/^\/\/![ ]?/, "", line)
  }
  if (line ~ /^[[:space:]]*$/) { flush(); next }
  prose[FILENAME]++
  if (PTEXT == "") PLINE = FNR
  PTEXT = (PTEXT == "" ? line : PTEXT " " line)
}

END {
  flush()
  # --- floors. Every one of these is a way for this guard to read nothing and print a clean bill. ---
  for (f in prose) {
    if (prose[f] == 0) {
      print "framework-prose guard: FAILED -- read ZERO prose lines from " f ". For a lib.rs that means" > "/dev/stderr"
      print "  the crate-root `//!` doc is gone or respelled, and none of its paragraphs were judged." > "/dev/stderr"
      fail = 1
    }
  }
  if (paragraphs == 0) {
    print "framework-prose guard: FAILED -- zero paragraphs assembled across every subject file." > "/dev/stderr"
    fail = 1
  }
  # The pointer sentence is not exempt from anything any more (see flush), but its DISAPPEARANCE is
  # still a defect worth naming: it is the repair this guard was built to protect. If no doc points at
  # the const, the drift it prevents is one edit away and nothing here would say so.
  if (pointer_paragraphs == 0) {
    print "framework-prose guard: FAILED -- not one paragraph names FRAMEWORK_RECOGNIZERS. Every" > "/dev/stderr"
    print "  crate-root doc stopped handing the reader to the const, or the token was respelled. The" > "/dev/stderr"
    print "  partial-list rule below still holds, but the repair it guards has been undone." > "/dev/stderr"
    fail = 1
  }
  if (fail) {
    print "" > "/dev/stderr"
    print "framework-prose guard: FAILED. A parser doc paragraph lists SOME of the frameworks its" > "/dev/stderr"
    print "FRAMEWORK_RECOGNIZERS const declares. Name all of them, or name none and point at the const" > "/dev/stderr"
    print "instead -- the sentence four crates already use:" > "/dev/stderr"
    print "    WHICH frameworks is deliberately not listed here: [`FRAMEWORK_RECOGNIZERS`] below is the" > "/dev/stderr"
    print "    machine-verified answer." > "/dev/stderr"
    exit 1
  }
  printf "framework-prose guard: OK (%d paragraphs over %d files, %d crates with recognizers; %d lone mention(s) under the two-name list threshold; %d complete in-prose list(s) verified against their const; %d paragraph(s) pointing at the const).\n", \
    paragraphs, files, ncrates, singletons, complete, pointer_paragraphs
}
' $libs "$README"
