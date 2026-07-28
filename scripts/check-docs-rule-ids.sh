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
# config-facing as-is, including the 25 `cross-layer/*` ids that carry a "/" of their own — those are
# NOT DSL packs and are never re-prefixed. The valid id universe additionally includes bare DSL PACK ids
# (`sql`, `typescript`, ...): crates/core/src/registry.rs's `is_enabled` doc states all three id shapes
# are honored end to end ("a bare native-analysis/JS-quick-rule id, a whole DSL pack id, or a full
# `"<pack>/<rule>"` id"), so a doc example disabling a whole pack by its bare id is legitimate.
#
# Covered example shapes (all matched after decoding `&quot;` -> `"` so an HTML surface that entity-encodes
# its code blocks stays scanned; site/usage.html currently uses plain quotes inside <pre><code>):
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
#   - `packs.disabled` entries (bare pack ids by design — a different, pack-id-only key space; validating
#     it against the pack-id set would be a separate check, not this drift class).
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

# Default file list: the TWO user-facing surfaces that still carry `rules:`-shaped config examples —
# docs/getting-started.md and site/usage.html. It said "the four real user-facing surfaces" while
# listing two, uncorrected across the removals that took the other two away (the JS front-end's init
# template went with the JS CLI on 2026-07-20; README's config block was distilled out). Counted
# 2026-07-26. The count is stated once, right here, next to the list it counts — a scope claim that
# lives away from its own list is a scope claim nobody re-reads when the list changes. The OK line at
# the bottom prints the ACTUAL count, so the two can be compared without trusting this comment.
# ZZOP_CHECK_DOCS_RULE_IDS_FILES overrides the list (newline-separated paths) for self-testing against
# fixtures — never used by the shipped CI step.
if [ -n "${ZZOP_CHECK_DOCS_RULE_IDS_FILES:-}" ]; then
  files="$ZZOP_CHECK_DOCS_RULE_IDS_FILES"
else
  # DERIVED, not hand-listed. Two hardcoded paths sat here until 2026-07-28 — and this script's own
  # header already recorded that the list had rotted once ("It said 'the four real user-facing
  # surfaces' while listing two, uncorrected across the removals"). Measured that day: a bare DSL id
  # (`no-explicit-any` instead of `typescript/no-explicit-any`) planted in README.md — a real
  # user-facing surface outside the list — left this guard GREEN. A guard whose SCANNED SET is
  # hand-typed cannot see a surface nobody remembered to add, and the header proves nobody does.
  #
  # The subject is now every tracked .md/.html, PREFILTERED to the files that could possibly carry a
  # finding. The three passes below only ever fire on a severity token, a `"severity"` field, or a
  # `disabledRules` array, so a file with none of those three is provably a no-op — and per-file cost
  # here is process spawns, which dominate under msys (the widened scan measured 55s over all 43 files
  # and 25s over the 19 that match). The prefilter is deliberately LOOSER than the passes (bare words,
  # no anchoring, entity form included): it may only over-include, never under-include, because a
  # prefilter that drops a real candidate is a silent false green — the exact failure this whole sweep
  # is about. If a pass ever grows a fourth trigger, widen this regex in the same edit.
  files="$(git -C "$repo_root" ls-files -- '*.md' '*.html' \
    | sed "s|^|$repo_root/|" \
    | xargs grep -lE 'severity|disabledRules|"(off|none|disable|disabled|critical|error|err|high|warning|warn|medium|info|information|note|low)"' 2>/dev/null || true)"
fi

scanned=0
for f in $files; do
  [ -f "$f" ] || { echo "check-docs-rule-ids: missing $f" >&2; exit 1; }
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
catalog_ids="$(awk '
  /^## DSL packs/      { mode = "dsl"; next }
  /^## Native analyses/{ mode = "native"; next }
  /^### `/ {
    if (mode == "dsl") {
      pack = $0
      sub(/^### `/, "", pack)
      sub(/`.*/, "", pack)
    }
    next
  }
  /^\| `[a-z0-9]/ {
    id = $0
    sub(/^\| `/, "", id)
    sub(/`.*/, "", id)
    if (mode == "dsl")    print pack "/" id
    else if (mode == "native") print id
  }
' "$catalog" | sort -u)"

# Bare DSL pack ids (the `### `<pack>`` headings of the DSL section; the stub-packs heading carries no
# backtick-wrapped id, so it is naturally excluded). Valid in `disabledRules` and as a whole-pack-off
# `rules:` key per crates/core/src/registry.rs's `is_enabled` doc (see header comment).
pack_ids="$(awk '
  /^## Native analyses/ { exit }
  /^### `/ { p = $0; sub(/^### `/, "", p); sub(/`.*/, "", p); print p }
' "$catalog" | sort -u)"

valid_ids="$(printf '%s\n%s\n' "$catalog_ids" "$pack_ids" | grep -v '^$' | sort -u)"

catalog_id_count="$(printf '%s\n' "$catalog_ids" | grep -c . || true)"
pack_id_count="$(printf '%s\n' "$pack_ids" | grep -c . || true)"
[ "$catalog_id_count" -gt 0 ] || { echo "check-docs-rule-ids: extracted 0 ids from $catalog — extraction is broken" >&2; exit 1; }
[ "$pack_id_count" -gt 0 ] || { echo "check-docs-rule-ids: extracted 0 pack ids from $catalog — extraction is broken" >&2; exit 1; }

# Herestrings, never `printf big-blob | grep -q`: under pipefail, grep -q exiting on first match
# SIGPIPEs printf (exit 141) once the input exceeds the pipe buffer — a real match reads as failure.
is_valid_id() {
  grep -qxF "$1" <<< "$valid_ids"
}

is_in() { # $1 = candidate, $2 = newline-separated set
  grep -qxF "$1" <<< "$2"
}

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

check_file() {
  local file="$1"

  # Decode &quot; so an entity-encoded HTML code block is scanned identically to a Markdown/JS one.
  # Line numbers are preserved (pure in-line substitution).
  local content
  content="$(sed 's/&quot;/"/g' "$file")"

  # --- Pass A: single-line string form — "<key>": "<severity-token>" ---
  local simple
  simple="$(printf '%s\n' "$content" \
    | grep -noE '"[A-Za-z][A-Za-z0-9/_.-]*"[[:space:]]*:[[:space:]]*"('"$severity_tokens"')"' || true)"
  while IFS=: read -r lineno match; do
    [ -z "$lineno" ] && continue
    local key
    key="$(printf '%s' "$match" | sed -E 's/^"([^"]+)".*/\1/')"
    is_in "$key" "$severity_key_allowlist" && continue
    is_valid_id "$key" && continue
    echo "check-docs-rule-ids: $file:$lineno: key \"$key\" is not a cataloged rule/analysis id" >&2
    echo "  (DSL rules need the full pack/rule id, e.g. \"typescript/no-explicit-any\", not a bare id)" >&2
    fail=1
  done <<< "$simple"

  # --- Pass B: object form — "<key>": {   (body may span lines; exclude-only entries included) ---
  #
  # A key in OBJECT form is a rule id only when its object is a rule CONFIG, and the one thing that
  # makes it one is a `severity` field (crates/config's `ruleObject`). Without that discriminator this
  # pass treats every `"key": {` in the repo as a candidate id — survivable while the scanned surface
  # was two curated config-example files, and 28 false positives the moment the surface became derived
  # (2026-07-28): envelope shapes, MCP server configs, DSL matcher docs. The alternative on offer was
  # growing `object_key_allowlist` to hold them, and an allowlist that grows is how a guard stops
  # guarding — the reason check-vendor-token-literals.sh never got an escape hatch. Anchor on meaning.
  #
  # ONE awk pass, not a per-match `sed` window: the obvious spelling (spawn a sed per candidate to slice
  # the lookahead) measured 2m21s on 43 files under msys, where process creation dominates — the same
  # trap check-max-file-lines.sh's header records for its own census. A guard nobody can afford to run
  # in a pre-commit hook is a guard that gets moved to CI and then skipped.
  local objform
  objform="$(printf '%s\n' "$content" | awk '
    { line[NR] = $0 }
    END {
      for (i = 1; i <= NR; i++) {
        win = ""
        for (j = i; j <= i + 4 && j <= NR; j++) win = win line[j]
        if (win !~ /"severity"/) continue
        # EVERY match on the line, not just the first. A single-line example
        # (`{ "rules": { "bad-id": { "severity": "off" } } }`) puts the allowlisted `rules` key and the
        # real candidate on one line; stopping at the first match finds only `rules`, skips it as
        # structural, and passes. Measured 2026-07-28 — this loop was a bare `match()` for one revision
        # and a planted bare id went undetected, which is how the grep this replaced behaved correctly
        # by accident (`grep -oE` emits all matches per line).
        rest = line[i]
        while (match(rest, /"[A-Za-z][A-Za-z0-9\/_.-]*"[ \t]*:[ \t]*\{/)) {
          key = substr(rest, RSTART + 1)
          sub(/".*/, "", key)
          print i ":" key
          rest = substr(rest, RSTART + RLENGTH)
        }
      }
    }' || true)"
  while IFS=: read -r lineno key; do
    [ -z "$lineno" ] && continue
    is_in "$key" "$object_key_allowlist" && continue
    is_valid_id "$key" && continue
    echo "check-docs-rule-ids: $file:$lineno: object-form key \"$key\" is not a cataloged rule/analysis id" >&2
    echo "  (DSL rules need the full pack/rule id, e.g. \"sql/nplus1\", not a bare id; a new structural" >&2
    echo "   config key with an object value must be added to this script's object_key_allowlist)" >&2
    fail=1
  done <<< "$objform"

  # --- Pass C: "disabledRules": [ ... ] arrays, single- or multi-line; validate every element ---
  local dr_elems
  dr_elems="$(printf '%s\n' "$content" | awk '
    function emit(s,   t) {
      t = index(s, "]")
      if (t) { s = substr(s, 1, t - 1); inarr = 0 }
      while (match(s, /"[^"]*"/)) {
        print NR ":" substr(s, RSTART + 1, RLENGTH - 2)
        s = substr(s, RSTART + RLENGTH)
      }
    }
    inarr { emit($0); next }
    /"disabledRules"[ \t]*:[ \t]*\[/ {
      s = $0
      sub(/.*"disabledRules"[ \t]*:[ \t]*\[/, "", s)
      inarr = 1
      emit(s)
    }
  ' || true)"
  while IFS=: read -r lineno elem; do
    [ -z "$lineno" ] && continue
    is_valid_id "$elem" && continue
    echo "check-docs-rule-ids: $file:$lineno: disabledRules entry \"$elem\" is not a cataloged rule/analysis/pack id" >&2
    echo "  (DSL rules need the full pack/rule id; native ids incl. cross-layer/* and bare pack ids are valid as-is)" >&2
    fail=1
  done <<< "$dr_elems"

  # --- Pass D: suppressions entry shape — "rule": "<id>"; the VALUE must be a valid id ---
  local rulevals
  rulevals="$(printf '%s\n' "$content" \
    | grep -noE '"rule"[[:space:]]*:[[:space:]]*"[^"]+"' || true)"
  while IFS=: read -r lineno match; do
    [ -z "$lineno" ] && continue
    local val
    val="$(printf '%s' "$match" | sed -E 's/^"rule"[[:space:]]*:[[:space:]]*"([^"]+)"$/\1/')"
    is_valid_id "$val" && continue
    echo "check-docs-rule-ids: $file:$lineno: suppression rule \"$val\" is not a cataloged rule/analysis/pack id" >&2
    echo "  (DSL rules need the full pack/rule id, e.g. \"sql/nplus1\", not a bare id)" >&2
    fail=1
  done <<< "$rulevals"
}

for f in $files; do
  check_file "$f"
done

if [ "$fail" -ne 0 ]; then
  echo "check-docs-rule-ids: FAILED — fix the offending example(s) to use a cataloged id (see docs/rules/catalog.md)." >&2
  exit 1
fi

file_count="$(printf '%s\n' "$files" | grep -c . || true)"
echo "check-docs-rule-ids: OK (${catalog_id_count} catalog ids + ${pack_id_count} pack ids vouched, ${file_count} doc/config surfaces checked)"
