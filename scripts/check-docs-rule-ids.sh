#!/usr/bin/env bash
# Guards against the "bare DSL id in a config example" drift class: a 2026-07-13 audit found user-facing
# `rules:` config examples (README, init template, getting-started doc, marketing site) using bare DSL
# rule ids ("no-explicit-any", "n-plus-one", "toctou") while the engine matches EXACT "{pack}/{rule}"
# strings — each such example was a silent no-op (the id never matched anything, so the "override"
# quietly did nothing). This guard turns that doc drift into a CI failure instead of a silent no-op
# discovered by a user.
#
# SSOT id set: docs/rules/catalog.md, read the same way scripts/check-rules-catalog-sync.sh does — it is
# machine-pinned to the engine by crates/engine/tests/rule_contracts/, so it transitively vouches for
# reality. The catalog lists DSL rule ids BARE, one table per pack under a `### `<pack>`` heading (e.g.
# `no-explicit-any` under `### `typescript``), so this script reconstructs each rule's config-facing id
# as `<pack>/<id>` from heading + row. Native analysis ids (the "## Native analyses" section) are
# config-facing as-is, including the `cross-layer/*` ids that carry a "/" of their own — those are
# (how many: grep -o '^| `cross-layer/[a-z-]*`' docs/rules/catalog.md | sort -u | wc -l. The number is
# not written here. It said 25 while the catalog held 27, because a bare count in a guard's own header
# fails nothing and so nobody recounts it.)
# NOT DSL packs and are never re-prefixed. The valid id universe additionally includes bare DSL PACK ids
# (`sql`, `typescript`, ...): crates/core/src/registry.rs's `is_enabled` doc states all three id shapes
# are honored end to end ("a bare native-analysis/JS-quick-rule id, a whole DSL pack id, or a full
# `"<pack>/<rule>"` id"), so a doc example disabling a whole pack by its bare id is legitimate.
#
# Covered example shapes (all matched after decoding `&quot;` -> `"` so an HTML surface that entity-encodes
# its code blocks stays scanned — site/rules.html does; the site's Rules tab in site/index.html writes the
# same shape with plain quotes, so both spellings have to be reachable. That second example was
# site/usage.html until it became a redirect stub on 2026-08-14):
#   A. `"<key>": "<severity-token>"`  — single-line string form. <key> must be an allowlisted
#      severity-carrying config key or a cataloged id.
#   B. `"<key>": {`                   — object form (exclude-only, severity-carrying, or empty; the body
#      may span lines — only the opening brace must sit on the key's line). <key> must be an allowlisted
#      structural config key or a cataloged id. This deliberately does NOT require a "severity" field:
#      `{ "exclude": [...] }`-only rule entries are exactly as reachable by the drift.
#   C. `"disabledRules": [ ... ]`     — embedder arrays, single- OR multi-line; every quoted element must
#      be a cataloged id or a DSL pack id.
#   D. `"rule": "<id>"`               — the embedder `suppressions` entry shape (docs/getting-started.md's
#      SDK example); the VALUE must be a cataloged id or pack id.
#
# Known-uncovered shapes (documented, not silently ignored):
#   - A key whose opening `{` sits on the NEXT line (`"id":` <newline> `{`) — no scanned surface writes
#     JSON that way; covering it needs a real parser, not grep.
#   - ~~`packs.disabled` entries~~ — CLOSED 2026-08-12 by `scripts/check-pack-id-values.sh`, which is the
#     separate check this line predicted: bare pack ids are a pack-id-only key space, so it validates
#     `packs.disabled`/`packs.only` array VALUES against the ids under `rules/dsl/`, over a wider subject
#     than this guard's `.md`/`.html` (it also reads `*.jsonc`/`*.json`, so the repo's own committed
#     config and the config-parity fixture are in scope). Kept here rather than deleted: this line is
#     where a reader looks for that key space, and "covered elsewhere" is the answer they need.
#
# Severity/disable token vocabulary (pass A): the UNION of crates/core's wire-level `Severity` enum
# (crates/core/src/finding.rs: `#[serde(rename_all = "lowercase")] enum Severity { Critical, Warning,
# Info }`) and crates/config/src/mapper/severity.rs's `SEVERITY_ALIASES` — the single table turning
# friendly config severities into those serde values, whose declaration ORDER is load-bearing (its own
# doc says so: the order is reproduced verbatim in the "Expected one of: ..." error text). Plus the
# off/none/disable/disabled family it maps to a disabled rule. (Was packages/cli/lib/mapper.js until the
# JS front-end was removed 2026-07-20 and ported to crates/config.)
# The full alias set matters: real examples write "warn", not "warning" — a narrower token set would
# silently skip exactly the shape most likely to recur.
#
# Failure-mode bias: an unknown key is a LOUD failure, on purpose. Adding a new config key later that
# legitimately takes a severity-like value or an object value (rare — see the two allowlists below) will
# fail this guard until it is allowlisted. That's intended: cheap to fix, and it forces a human to look
# rather than the guard silently widening its own blind spot. A bare rule id passing silently is the one
# failure mode this script must never have.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
catalog="$repo_root/docs/rules/catalog.md"

[ -f "$catalog" ] || { echo "check-docs-rule-ids: missing $catalog" >&2; exit 1; }

# Subject set: DERIVED from git. Not hand-listed, and NOT overridable — this script takes no input
# that can narrow what it reads. Two pieces of history, both load-bearing:
#
#  1. It began as TWO hardcoded paths (docs/getting-started.md, site/usage.html) under a comment
#     claiming "the four real user-facing surfaces" while listing two — uncorrected across the two
#     removals that took the others away (the JS front-end's init template went with the JS CLI on
#     2026-07-20; README's config block was distilled out). Measured 2026-07-28: a bare DSL id
#     (`no-explicit-any` instead of `typescript/no-explicit-any`) planted in README.md — a real
#     user-facing surface outside the list — left this guard GREEN. A guard whose SCANNED SET is
#     hand-typed cannot see a surface nobody remembered to add, and the header proved nobody does.
#
#  2. It then carried a `ZZOP_CHECK_DOCS_RULE_IDS_FILES` env override that REPLACED the derived set
#     with an arbitrary newline-separated path list, annotated "for self-testing against fixtures —
#     never used by the shipped CI step". Removed 2026-07-28. Nothing in the tree ever read it: it
#     was the only guard-scope override among the 23 scripts/check-*.sh guards, and no fixture
#     self-test harness exists for any of them, so the use it was written for never arrived. What
#     did exist was a working off-switch — measured, with a real violation planted in README.md:
#       $ bash scripts/check-docs-rule-ids.sh
#       check-docs-rule-ids: README.md:327: object-form key "no-explicit-any" is not a cataloged ...
#       exit=1
#       $ ZZOP_CHECK_DOCS_RULE_IDS_FILES="$PWD/LICENSE" bash scripts/check-docs-rule-ids.sh
#       check-docs-rule-ids: OK (196 catalog ids + 12 pack ids vouched, 1 doc/config surfaces checked)
#       exit=0
#     An env var that turns a guard green over a file it would never otherwise scan is an off-switch
#     whatever its comment calls it, and the zero-file abort below cannot catch it — one file is not
#     zero files. Deleted rather than converted into a "fails if the scope shrinks" form: detecting a
#     shrink needs a committed expected count, i.e. a SECOND enumeration of what "checked" means, and
#     a second enumeration is precisely the defect class this repo spent 2026-07-28 removing from
#     sixteen guards. Paying for that to keep a hatch with no caller is the wrong trade. A developer
#     wanting a narrow run edits their working copy, where the diff is visible, instead of exporting
#     a variable that leaves no trace in the exit code.
#
# The subject is every tracked .md/.html, PREFILTERED to the files that could possibly carry a
# finding. The four passes below only ever fire on a severity token, a `"severity"` field, a
# `disabledRules` array, or a `"rule"` key, so a file with none of those is provably a no-op. The
# prefilter is deliberately LOOSER than the passes (bare words, no anchoring, entity form included):
# it may only over-include, never under-include, because a prefilter that drops a real candidate is a
# silent false green — the exact failure this whole sweep is about. If a pass ever grows a fifth
# trigger, widen this regex in the same edit.
#
# What the prefilter is now WORTH is smaller than the number this comment used to quote. It was
# introduced when the scan cost five process spawns PER FILE, so halving the file count halved the
# guard (the figures recorded here were 55s over all 43 files, 25s over the ~20 that match). The
# scan below is now ONE awk over every subject file, so a wider subject costs reading, not spawning:
# measured 2026-07-29, dropping the prefilter entirely moves this guard by well under a second. It is
# kept because narrowing it back later would be a coverage change nobody would notice, and because
# the `xargs grep -l` it costs is a single batched spawn — but it is no longer a performance argument,
# and nobody should defend it as one.
#
# The OK line at the bottom prints the ACTUAL count, so the scanned scope is readable from the run
# itself rather than trusted from this comment.
files="$(git -C "$repo_root" ls-files -- '*.md' '*.html' \
  | sed "s|^|$repo_root/|" \
  | xargs grep -lE 'severity|disabledRules|"(off|none|disable|disabled|critical|error|err|high|warning|warn|medium|info|information|note|low)"' 2>/dev/null || true)"

# Word-splitting on $files (unquoted) is the enumeration this guard has always used; the array is
# only a way to hand the very same list to awk as argv instead of re-splitting it per pass.
subject=()
scanned=0
for f in $files; do
  [ -f "$f" ] || { echo "check-docs-rule-ids: missing $f" >&2; exit 1; }
  subject+=("$f")
  scanned=$((scanned + 1))
done
# An empty subject set is a broken enumeration, not a repo with no docs — the sibling failure this
# repo has already paid for twice (a scan root pointing at a deleted directory, green while reading
# nothing). Load-bearing now that the list is a glob rather than two literal paths.
if [ "$scanned" -eq 0 ]; then
  echo "check-docs-rule-ids: FAILED -- ZERO files to scan. The surface enumeration matched nothing, so" >&2
  echo "no doc was checked against the catalog. This repo ships docs; a zero here is a broken scan." >&2
  exit 1
fi

# --- Build the SSOT id set from docs/rules/catalog.md ---------------------------------------------------
# DSL rule ids are prefixed with their owning pack's heading (`<pack>/<id>`); native analysis ids
# (including the already-slashed `cross-layer/*` ids) are taken bare, exactly as printed.
#
# The valid-id universe is exactly THREE tables, and the `mode` machine below is what says which one a
# row is in. Two of the three are pack sections whose rows are prefixed with their `### `<pack>``
# heading — `## DSL packs` (bundled) and `## Exported packs` (`examples/packs/`, retrievable and
# loadable, so a config example naming one of their ids is legitimate). The third is the native table,
# whose rows stand as printed.
#
# EVERY OTHER HEADING CLOSES THE UNIVERSE, and that line is the 2026-08-13 repair. `mode` used to be
# set by the two `##` anchors and never reset, so it leaked DOWNWARD past `## Native analyses` into
# `### Recommendation ids` — a table this catalog introduces with the words **"These are not rule
# ids"** — and admitted its 8 rows as valid ids (measured: 211 ids, of which 7 were recommendation-only
# and 1 (`circular`) collided with a real native id). The binary says the opposite in as many words:
#   $ zzop explain hot-churn
#   zzop: "hot-churn" is a RECOMMENDATION id, not a rule id ... there is no per-recommendation toggle.
# So `rules: { "hot-churn": "off" }` in a doc example is exactly the silent no-op this guard exists to
# fail on, and this guard was the thing blessing it.
#
# The leak's OTHER half is deliberate and stays: `## Exported packs` inherits pack mode on purpose,
# now by an explicit anchor rather than by omission. Fixing "the mode never resets" by resetting at
# every `##` would have deleted the 28 exported ids from the universe — legitimate ids, in the
# population this repo just spent a release teaching people to load.
#
# The failure DIRECTION of the reset is the safe one: a heading this machine does not recognize
# narrows the universe, so a doc example under it goes RED and a human looks, per the header's
# failure-mode bias. A widening blind spot is the one outcome that must never be silent.
catalog_ids="$(awk '
  /^## DSL packs/       { mode = "dsl";    next }
  /^## Exported packs/  { mode = "dsl";    next }
  /^## Native analyses/ { mode = "native"; next }
  /^### `/ {
    if (mode == "dsl") {
      pack = $0
      sub(/^### `/, "", pack)
      sub(/`.*/, "", pack)
    } else {
      # A backticked subheading outside a pack section names no pack — whatever follows it is not a
      # rule table, so stop attributing rows to the section above.
      mode = ""
    }
    next
  }
  /^###? / { mode = ""; next }
  /^\| `[a-z0-9]/ {
    id = $0
    sub(/^\| `/, "", id)
    sub(/`.*/, "", id)
    if (mode == "dsl")    print pack "/" id
    else if (mode == "native") print id
  }
' "$catalog" | sort -u)"

# Bare DSL pack ids — every `### `<pack>`` heading above `## Native analyses`, which is BOTH pack
# sections (bundled and exported); a heading with no backtick-wrapped id is naturally excluded. Valid
# in `disabledRules` and as a whole-pack-off `rules:` key per crates/core/src/registry.rs's
# `is_enabled` doc (see header comment). No `mode` machine here and none needed: the `exit` is the
# boundary, and the only headings above it are pack headings.
pack_ids="$(awk '
  /^## Native analyses/ { exit }
  /^### `/ { p = $0; sub(/^### `/, "", p); sub(/`.*/, "", p); print p }
' "$catalog" | sort -u)"

# `|| true` is load-bearing, and ORDER is why: when BOTH extractions above come back empty this
# `grep -v '^$'` sees nothing but blank lines, matches none of them, and exits 1 — which under
# `set -e -o pipefail` kills the script HERE, three lines above the two floors written to diagnose
# exactly that state. Measured 2026-08-14 by pointing both awk needles at headings the catalog does
# not carry: without it the guard exited 1 with ZERO bytes of output; with it the "extracted 0 ids
# from $catalog — extraction is broken" line prints. The two `grep -c ... || true` floors below were
# already immune, which is what made this line the last unlit fuse rather than an obvious one.
valid_ids="$(printf '%s\n%s\n' "$catalog_ids" "$pack_ids" | grep -v '^$' | sort -u || true)"

catalog_id_count="$(printf '%s\n' "$catalog_ids" | grep -c . || true)"
pack_id_count="$(printf '%s\n' "$pack_ids" | grep -c . || true)"
[ "$catalog_id_count" -gt 0 ] || { echo "check-docs-rule-ids: extracted 0 ids from $catalog — extraction is broken" >&2; exit 1; }
[ "$pack_id_count" -gt 0 ] || { echo "check-docs-rule-ids: extracted 0 pack ids from $catalog — extraction is broken" >&2; exit 1; }

# The id sets are handed to awk and looked up as associative-array keys, which is an exact
# whole-string match — the same semantics as the `grep -qxF "$candidate" <<< "$set"` helpers this
# replaced, without the spawn. (Those helpers used a HERESTRING, never `printf big-blob | grep -q`:
# under pipefail, grep -q exiting on first match SIGPIPEs printf (exit 141) once the input exceeds
# the pipe buffer, so a real match reads as failure. The hazard is recorded here because the idiom
# is still the right one anywhere a set membership test does need a subprocess.)

# Pass-A allowlist: config keys that legitimately carry a severity-like STRING value without being a rule
# id. Source: crates/config/config-surface.json — `configKeys.top` includes `failOn`;
# `configKeys.ruleObject` includes `severity` (the nested field of the object form).
severity_key_allowlist="severity
failOn"

# Pass-B allowlist: structural config keys that legitimately take an OBJECT value without being a rule
# id. Source: crates/config/config-surface.json's `configKeys.top` — exactly these take an object
# literal (`rules`, `packs`, `git`, `report`, `vocabulary`); the rest take strings/arrays/scalars and
# can never match pass B's `"<key>": {` shape.
object_key_allowlist="rules
packs
git
report
vocabulary"

# Severity/disable token vocabulary — see header comment for sources.
severity_tokens='off|none|disable|disabled|critical|error|err|high|warning|warn|medium|info|information|note|low'

fail=0

# ONE awk process for the WHOLE subject set — see the cost note at the top of this block.
#
# What this replaced (2026-07-29): a bash `check_file` called once per subject file, each call
# spawning a `sed` (entity decode), two `grep`s (passes A and D), two `awk`s (passes B and C), plus a
# `sed` per pass-A/D match to slice the key out and a `grep -qxF` per candidate to test set
# membership. That is five processes per file BEFORE any match, on ~20 files, and the measured cost
# was 45-47s of which `user` time was a rounding error: on this box the tax is fork/exec, not
# computation. Nothing about WHAT is examined changes here — same subject files, same four passes,
# same regexes, same messages, same line numbers.
#
# awk already knows file boundaries (FILENAME/FNR), so the per-file loop the shell was paying a
# process for is just an index. Lines are buffered per file because pass B needs a five-line
# lookahead window; the buffer is flushed on the next file's FNR == 1 and once more in END.
#
# The sets cross into awk through the ENVIRONMENT, not `-v`: `awk -v x="$multi_line_value"` runs the
# value through escape-sequence processing, so an id containing a backslash would be silently
# rewritten. ENVIRON is verbatim, and costs no extra process.
scan_rc=0
ZZ_VALID_IDS="$valid_ids" \
ZZ_SEVERITY_KEYS="$severity_key_allowlist" \
ZZ_OBJECT_KEYS="$object_key_allowlist" \
ZZ_SEVERITY_TOKENS="$severity_tokens" \
awk '
BEGIN {
  nv = split(ENVIRON["ZZ_VALID_IDS"], v, "\n")
  for (i = 1; i <= nv; i++) if (v[i] != "") valid[v[i]] = 1
  ns = split(ENVIRON["ZZ_SEVERITY_KEYS"], s, "\n")
  for (i = 1; i <= ns; i++) if (s[i] != "") sevkey[s[i]] = 1
  no = split(ENVIRON["ZZ_OBJECT_KEYS"], o, "\n")
  for (i = 1; i <= no; i++) if (o[i] != "") objkey[o[i]] = 1

  # Built as strings because the severity alternation comes from the shell. Byte-for-byte the EREs
  # the two greps used, including [[:space:]] rather than [ \t] — the passes that were already awk
  # (B and C) keep their own [ \t] spellings below, also unchanged.
  sev_re  = "\"[A-Za-z][A-Za-z0-9/_.-]*\"[[:space:]]*:[[:space:]]*\"(" ENVIRON["ZZ_SEVERITY_TOKENS"] ")\""
  rule_re = "\"rule\"[[:space:]]*:[[:space:]]*\"[^\"]+\""
  fail = 0
}

# Flush the PREVIOUS file before `subject` is reassigned. An empty file produces no records at all,
# so it neither flushes nor is flushed twice — it simply contributes nothing, exactly as the
# per-file version did.
FNR == 1 && nl > 0 { check_file() }
FNR == 1 { subject = FILENAME; nl = 0; delete L }

# Decode &quot; so an entity-encoded HTML code block is scanned identically to a Markdown/JS one.
# Line numbers are preserved (pure in-line substitution).
{ ln = $0; gsub(/&quot;/, "\"", ln); L[++nl] = ln }

END { if (nl > 0) check_file(); if (fail) exit 1 }

function check_file(   i, j, rest, m, key, win, str, t, elem, val, inarr) {
  # --- Pass A: single-line string form — "<key>": "<severity-token>" ---
  # EVERY match on the line, not just the first: `grep -oE` emits them all, and the object-form pass
  # below records what it cost to lose that property once.
  for (i = 1; i <= nl; i++) {
    rest = L[i]
    while (match(rest, sev_re)) {
      m = substr(rest, RSTART, RLENGTH)
      rest = substr(rest, RSTART + RLENGTH)
      key = substr(m, 2); sub(/".*/, "", key)
      if (key in sevkey) continue
      if (key in valid) continue
      print "check-docs-rule-ids: " subject ":" i ": key \"" key "\" is not a cataloged rule/analysis id" > "/dev/stderr"
      print "  (DSL rules need the full pack/rule id, e.g. \"typescript/no-explicit-any\", not a bare id)" > "/dev/stderr"
      fail = 1
    }
  }

  # --- Pass B: object form — "<key>": {   (body may span lines; exclude-only entries included) ---
  #
  # A key in OBJECT form is a rule id only when its object is a rule CONFIG, and the one thing that
  # makes it one is a `severity` field (crates/config'"'"'s `ruleObject`). Without that discriminator this
  # pass treats every `"key": {` in the repo as a candidate id — survivable while the scanned surface
  # was two curated config-example files, and 28 false positives the moment the surface became derived
  # (2026-07-28): envelope shapes, MCP server configs, DSL matcher docs. The alternative on offer was
  # growing `object_key_allowlist` to hold them, and an allowlist that grows is how a guard stops
  # guarding — the reason check-vendor-token-literals.sh never got an escape hatch. Anchor on meaning.
  #
  # ONE pass over the buffered lines, not a per-match `sed` window: the obvious spelling (spawn a sed
  # per candidate to slice the lookahead) measured 2m21s on 43 files under msys, where process
  # creation dominates — the same trap check-max-file-lines.sh'"'"'s header records for its own census. A
  # guard nobody can afford to run in a pre-commit hook is a guard that gets moved to CI and then
  # skipped.
  for (i = 1; i <= nl; i++) {
    win = ""
    for (j = i; j <= i + 4 && j <= nl; j++) win = win L[j]
    if (win !~ /"severity"/) continue
    # EVERY match on the line, not just the first. A single-line example
    # (`{ "rules": { "bad-id": { "severity": "off" } } }`) puts the allowlisted `rules` key and the
    # real candidate on one line; stopping at the first match finds only `rules`, skips it as
    # structural, and passes. Measured 2026-07-28 — this loop was a bare `match()` for one revision
    # and a planted bare id went undetected, which is how the grep this replaced behaved correctly
    # by accident (`grep -oE` emits all matches per line).
    rest = L[i]
    while (match(rest, /"[A-Za-z][A-Za-z0-9\/_.-]*"[ \t]*:[ \t]*\{/)) {
      key = substr(rest, RSTART + 1)
      sub(/".*/, "", key)
      rest = substr(rest, RSTART + RLENGTH)
      if (key in objkey) continue
      if (key in valid) continue
      print "check-docs-rule-ids: " subject ":" i ": object-form key \"" key "\" is not a cataloged rule/analysis id" > "/dev/stderr"
      print "  (DSL rules need the full pack/rule id, e.g. \"sql/nplus1\", not a bare id; a new structural" > "/dev/stderr"
      # \047 is the apostrophe: the awk program is single-quoted in sh, so a literal one cannot appear
      # here (the same constraint check-rule-desc-tokens.sh and check-guards-wired.sh record).
      print "   config key with an object value must be added to this script\047s object_key_allowlist)" > "/dev/stderr"
      fail = 1
    }
  }

  # --- Pass C: "disabledRules": [ ... ] arrays, single- or multi-line; validate every element ---
  # The `inarr` state machine is unchanged; it is a function-local here because the enclosing
  # per-file scope is now a function call rather than a fresh awk process.
  inarr = 0
  for (i = 1; i <= nl; i++) {
    str = L[i]
    if (!inarr) {
      if (str !~ /"disabledRules"[ \t]*:[ \t]*\[/) continue
      sub(/.*"disabledRules"[ \t]*:[ \t]*\[/, "", str)
      inarr = 1
    }
    t = index(str, "]")
    if (t) { str = substr(str, 1, t - 1); inarr = 0 }
    while (match(str, /"[^"]*"/)) {
      elem = substr(str, RSTART + 1, RLENGTH - 2)
      str = substr(str, RSTART + RLENGTH)
      if (elem in valid) continue
      print "check-docs-rule-ids: " subject ":" i ": disabledRules entry \"" elem "\" is not a cataloged rule/analysis/pack id" > "/dev/stderr"
      print "  (DSL rules need the full pack/rule id; native ids incl. cross-layer/* and bare pack ids are valid as-is)" > "/dev/stderr"
      fail = 1
    }
  }

  # --- Pass D: suppressions entry shape — "rule": "<id>"; the VALUE must be a valid id ---
  for (i = 1; i <= nl; i++) {
    rest = L[i]
    while (match(rest, rule_re)) {
      m = substr(rest, RSTART, RLENGTH)
      rest = substr(rest, RSTART + RLENGTH)
      val = m
      sub(/^"rule"[[:space:]]*:[[:space:]]*"/, "", val)
      sub(/"$/, "", val)
      if (val in valid) continue
      print "check-docs-rule-ids: " subject ":" i ": suppression rule \"" val "\" is not a cataloged rule/analysis/pack id" > "/dev/stderr"
      print "  (DSL rules need the full pack/rule id, e.g. \"sql/nplus1\", not a bare id)" > "/dev/stderr"
      fail = 1
    }
  }
}
' "${subject[@]}" || scan_rc=$?

# exit 1 is this program saying "violations"; anything else is awk itself failing (unreadable file,
# broken program) and must not be reported as a verdict. The version this replaced piped awk through
# `|| true`, which swallowed both.
if [ "$scan_rc" -eq 1 ]; then
  fail=1
elif [ "$scan_rc" -ne 0 ]; then
  echo "check-docs-rule-ids: the scan pass failed (awk exit $scan_rc) — aborting rather than report a" >&2
  echo "  verdict from a scan that did not finish." >&2
  exit 1
fi

if [ "$fail" -ne 0 ]; then
  echo "check-docs-rule-ids: FAILED — fix the offending example(s) to use a cataloged id (see docs/rules/catalog.md)." >&2
  exit 1
fi

file_count="$(printf '%s\n' "$files" | grep -c . || true)"
echo "check-docs-rule-ids: OK (${catalog_id_count} catalog ids + ${pack_id_count} pack ids vouched, ${file_count} doc/config surfaces checked)"
