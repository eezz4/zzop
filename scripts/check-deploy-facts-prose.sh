#!/usr/bin/env bash
# check-deploy-facts-prose.sh — guards against the "an inventory COUNT was written into prose in
# several places, one copy changed, the rest rotted" defect class. Hit at least three times:
#   - the bundled DSL pack count read "15" in docs/modules/mcp.md, "14" in crates/summary/src/contracts.rs
#     and "14" in rules/README.md while the truth was 12 (fixed 2026-07-24; the wire-visible copy in
#     embedded.rs is now pinned by a unit test, every prose copy was not pinned by anything);
#   - the bundled `security` pack's rule count read 2 against a truth of 44 in two facade test
#     assertions (commit e5d1d1b), left behind by the be-security/security merge.
# Every count below is DERIVED from the code on every run — nothing here is a hardcoded number, so
# adding a pack, a rule, or a release platform changes what this guard demands on the very next run.
#
# SSOTs parsed at run time:
#   1. Bundled DSL pack count — the tracked `*.json` files under `rules/dsl/`. This is exactly what
#      `crates/config/build.rs`'s `collect()` walks into `zzop_config::BUNDLED_PACK_SOURCES` (recursive,
#      extension `json`), so this script re-implements that one rule rather than reading the generated
#      `$OUT_DIR/bundled_packs.rs` (which requires a build, and a stale `target/` copy would be a worse
#      truth than none). `git ls-files` rather than a filesystem walk: what ships is what is committed,
#      and an untracked local pack must not silently license a prose bump.
#      Cross-check: every pack file must sit in its own directory and declare a pack `"id"`; a directory
#      holding zero or two pack files aborts this guard loudly instead of guessing.
#   2. Per-pack DSL rule count — the number of rule objects in that pack's JSON, counted as lines
#      matching `^      "id":` (the packs are 2-space indented, so a `rules[]` element's own `"id"` key
#      sits at exactly 6 spaces while the pack's `"id"` sits at 2). Cross-checked against
#      "every `"id":` in the file, minus the pack's own"; if the two derivations disagree the pack JSON
#      was reformatted and this guard aborts loudly rather than assert a number it no longer trusts.
#   3. Release platform count — the `platform:` entries of `.github/workflows/prebuild.yml`'s `build`
#      job matrix, read through the shared reader scripts/lib/release-matrix.sh (the same block
#      check-asset-name-prose.sh reads for asset NAMES and check-server-json-hashes.sh reads for the
#      ORDERED platform list; this guard only needs how MANY, so it takes the count accessor).
#      Cross-checked against `packages/cli/package.json`'s `optionalDependencies`
#      (one `@zzop/cli-<platform>` sub-package per platform) and the directories under
#      `packages/cli/npm/`; a disagreement between those three is itself a real drift and fails.
#   4. npm package count — platform count + 1, the `@zzop/cli` shim itself.
#
# Scan surface: TRACKED `*.md`, `*.html`, `*.rs`, `.github/workflows/*.yml` AND `scripts/*.sh`.
# `*.rs` is deliberately IN scope, unlike
# check-asset-name-prose.sh's md/html-only surface: two of the three confirmed instances of this defect
# class lived in Rust, not Markdown — `crates/summary/src/contracts.rs`'s `rule-catalog` description (which
# ships over MCP `resources/list`, i.e. a reader's only pack-count signal without a checkout) and
# `crates/facade/src/packs_tests.rs`'s shadow-warning comments/assertions. A doc comment or a test
# comment stating an inventory count rots exactly like a README does. Generated/vendored trees
# (`target/`, `node_modules/`, `.claude/`) are excluded by scripts/lib/tracked-grep.sh's standard
#
# `.github/workflows/*.yml` and `scripts/*.sh` joined the surface on 2026-08-08, and the reason is that
# the old justification for excluding them was HALF true. It read: "the workflows state these counts
# structurally, as matrix entries, which is the SSOT itself." That is right about the matrix and wrong
# about the PROSE ABOVE it — a workflow comment saying "the 5 platform binaries", or a guard header
# saying "all 23 guards", is an inventory claim in exactly this defect class, and nothing read it.
# Measured that day: `prebuild.yml` said "all 23 guards" against a fleet of 27, `ci.yml` said the
# committed graph block held "1,088 nodes" against 1,349, and `check-io-key-vocab.sh` said "43 docs"
# against 55. Three stale counts in the two file families this guard had declared it did not need to
# read. The platform/package claims in those same files were CORRECT and are now bound rather than
# deleted — a correct number that a machine holds is worth more than a number removed to stop it
# rotting. Widening cost nothing at the time (the surface went to 1,211 files and stayed clean).
#
# Claim shapes (the phrasing that actually carries the claim — never a bare digit, because numbers
# appear in prose for a hundred unrelated reasons and a guard that cries wolf gets disabled):
#   A. `N DSL packs` / `N bundled DSL packs` / `N packs shipped`   -> bundled pack count
#   B. `` `<pack>` (N rules) ``            (docs/rules/catalog.md's per-pack headings)
#      `` "<pack>" ships N rules ``        (packs_tests.rs's shadow-warning comment)
#      `` N-rule "<pack>" ``               (packs_tests.rs's collision comment)
#                                          -> that pack's rule count
#   C. `N platforms` / `N platform sub-packages` / `N platform targets`  -> release platform count
#   D. `N packages`, ONLY on a line that also mentions npm  -> npm package count
#   E. `N DSL rules`                       -> total rules across all bundled packs
# A bare `N packs` is deliberately NOT a claim shape: docs/rules/dsl-reference.md says "~90 rules across
# 11 packs before this mechanism existed", a HISTORICAL statement about a past tree that is correct as
# written and must never be rewritten to today's count.
#
# Zero carve-out: a match whose number is `0` is skipped everywhere. Every derived truth here is >= 1, and
# `0` is this repo's standard way to describe the DEGENERATE case ("would degrade to 0 DSL rules with only
# a warning" in crates/config/build.rs, "it means 0 DSL rules and 0 git signals" in crates/config/src/lib.rs)
# — two guaranteed false positives otherwise. The cost is that a literal `0 DSL packs` claim would slip
# through; no surface writes one, and it would be self-evidently wrong to any reader.
#
# Known-uncovered (documented, not silently ignored):
#   - the `N native analysis ids` count (docs/rules/catalog.md's totals line). Derivable only from
#     `zzop_engine::register_all_native`, i.e. only by BUILDING; a shell guard cannot see it. Already
#     pinned by `crates/engine/tests/rule_contracts/`'s `catalog_totals_match_loaded_rule_and_analysis_counts`,
#     which also re-pins the pack/rule totals this guard checks — this guard's value there is catching them
#     in pre-commit, seconds instead of a full workspace test run.
#   - the `all <word> tools` count (docs/modules/mcp.md). Spelled as an English word, not a digit, so this
#     guard's digit-anchored claim shapes do not see it.
#     NOT because the set is underivable: `packages/mcp/src/tools/definitions.rs` IS a declarative
#     registry (the `tools/list` schema), and `"name": "<tool>"` entries in it count the set exactly.
#     This header used to say the opposite — that the tools were plain match arms and "deriving it would
#     be guessing" — and that claim was wrong AND its own copy of the number was stale, in both the count
#     and the file it named (2026-08-08 audit). The remaining reason to leave it uncovered is the English
#     spelling, which is a matcher gap, not an underivability. Closing it means adding a word-number claim
#     shape, not a new source of truth.
#   - `All 12 ship rules.` (docs/rules/catalog.md, same sentence as the totals). A bare digit with no
#     anchoring noun; matching it would mean matching every bare number in the repo.
#   - illustrative output-format examples such as `` `typescript: 12 rules` `` (crates/engine/src/output.rs,
#     crates/engine/src/analyze/diagnostics/capability.rs). They demonstrate a MESSAGE SHAPE, and their
#     numbers are decoration, not inventory. Binding `<pack>: N rules` would make every reworded example a
#     guard failure.
#   - arrow/delta shapes such as `(bundled 2 -> replacement 1)` in a test's panic message. The two numbers
#     on such a line belong to different sides of a comparison; no anchoring distinguishes them, and
#     guessing wrong would fire on a correct sentence.
set -euo pipefail
cd "$(dirname "$0")/.."

# shellcheck source=scripts/lib/tracked-grep.sh
. ./scripts/lib/tracked-grep.sh
# shellcheck source=scripts/lib/release-matrix.sh
. ./scripts/lib/release-matrix.sh

SELF=check-deploy-facts-prose
PREBUILD=.github/workflows/prebuild.yml
CLI_PKG=packages/cli/package.json
# The two RUNTIME platform maps — the shim's install-time resolution table and the plugin's download
# bootstrap. Both carry the platform set by hand; the names axis below is what keeps them honest.
CLI_SHIM=packages/cli/bin/zzop.js
PLUGIN_BOOTSTRAP=.claude-plugin/hooks/bootstrap.sh
seen_platform_dirs=""

abort() { echo "$SELF: $*" >&2; exit 1; }

fail=0
report() { # $1=file $2=lineno $3=stated $4=expected $5=what
  echo "$SELF: $1:$2: states $3 $5, but the code says $4" >&2
  fail=1
}

# --- Truth 1 + 2: bundled DSL packs and their rule counts ------------------------------------------
# Process spawns are the dominant cost of every guard in this repo (a bare `git ls-files` costs ~2s on
# the Windows dev box), so each derivation below is one subprocess, with the loops done in bash.
pack_files=()
while IFS= read -r f; do
  case "$f" in
    *.json) pack_files+=("$f") ;;
  esac
done < <(git ls-files -- rules/dsl)
[ "${#pack_files[@]}" -gt 0 ] || abort "no tracked *.json under rules/dsl/ -- pack derivation is broken"

# One awk pass over every pack file emits `path <TAB> pack id <TAB> rules-by-indent <TAB> all "id": keys`.
pack_meta="$(awk '
  FNR == 1 { order[++n] = FILENAME }
  /^  "id": "/ && !(FILENAME in packid) {
    line = $0; sub(/^  "id": "/, "", line); sub(/".*$/, "", line); packid[FILENAME] = line
  }
  /^      "id":/ { indent[FILENAME]++ }
  /"id"[[:space:]]*:/ { allids[FILENAME]++ }
  END {
    for (i = 1; i <= n; i++) {
      f = order[i]
      print f "\t" packid[f] "\t" indent[f] + 0 "\t" allids[f] + 0
    }
  }
' "${pack_files[@]}")"

pack_ids=()
pack_rules=()
pack_count=0
rules_total=0
prev_dir=""
while IFS=$'\t' read -r pf pid n_indent n_all; do
  [ -n "$pf" ] || continue
  [ -n "$pid" ] || abort "$pf declares no top-level 2-space-indented \"id\" -- pack-id derivation is broken"
  case "$pid" in
    *[0-9]*) abort "pack id \"$pid\" contains a digit; the per-pack claim shapes extract the first digit run of a match and would misread it" ;;
  esac
  [ "$n_indent" -ge 1 ] || abort "$pf yields 0 rules -- rule counting is broken (or the pack is empty)"
  [ "$n_indent" -eq "$((n_all - 1))" ] || abort \
    "$pf: rule count by indentation ($n_indent) disagrees with total-\"id\"-minus-pack-id ($((n_all - 1))); the pack JSON layout changed and this guard's derivation is no longer trustworthy"

  # `git ls-files` output is path-sorted, so equal adjacent directories are the only way two pack files
  # can share one -- the one-pack-per-directory layout this guard's `rules/dsl/<pack>/<pack>.json` reading assumes.
  dir="${pf%/*}"
  [ "$dir" != "$prev_dir" ] || abort "$dir holds more than one pack JSON -- the one-pack-per-directory layout no longer holds"
  prev_dir="$dir"

  pack_ids+=("$pid")
  pack_rules+=("$n_indent")
  pack_count=$((pack_count + 1))
  rules_total=$((rules_total + n_indent))
done <<< "$pack_meta"

[ "$pack_count" -eq "${#pack_files[@]}" ] || abort \
  "derived metadata for $pack_count of ${#pack_files[@]} pack files -- the awk pass lost one"

# --- Truth 3 + 4: release platforms and npm packages -----------------------------------------------
[ -f "$CLI_PKG" ] || abort "missing $CLI_PKG"

# The build matrix is read by the SHARED reader (2026-08-08, scripts/lib/release-matrix.sh), which also
# owns the missing-file / broken-walk / entry-without-a-platform floors. This axis needs nothing from
# those records but HOW MANY there are, so it takes the count accessor and no more: the `os:` field and
# the matrix ORDER the same records carry are what check-asset-name-prose.sh and
# check-server-json-hashes.sh respectively inject on top of the same read.
matrix_records="$(release_matrix_entries "$SELF" "$PREBUILD")" || exit 1
matrix_platforms="$(release_matrix_count "$matrix_records")"
# Belt for the layer's own floor -- release_matrix_entries refuses to return an empty record list, and
# prebuild.yml's stamping loop keeps the same belt over the same reader for the same reason.
[ "$matrix_platforms" -gt 0 ] || abort "extracted 0 platforms from $PREBUILD's build-job matrix -- parse is broken"

opt_deps="$(grep -cE '"@zzop/cli-[a-z0-9-]+":' "$CLI_PKG" || true)"

npm_dirs=0
prev_dir=""
while IFS= read -r f; do
  [ -n "$f" ] || continue
  d="${f#packages/cli/npm/}"
  d="${d%%/*}"
  [ "$d" = "$prev_dir" ] && continue
  prev_dir="$d"
  npm_dirs=$((npm_dirs + 1))
done < <(git ls-files -- packages/cli/npm)

if [ "$opt_deps" -ne "$matrix_platforms" ] || [ "$npm_dirs" -ne "$matrix_platforms" ]; then
  echo "$SELF: the three platform inventories disagree -- this is itself a real drift, not a prose typo:" >&2
  echo "  $PREBUILD build matrix: $matrix_platforms platform(s)" >&2
  echo "  $CLI_PKG optionalDependencies: $opt_deps @zzop/cli-* sub-package(s)" >&2
  echo "  packages/cli/npm/: $npm_dirs platform directory(ies)" >&2
  exit 1
fi
platform_count="$matrix_platforms"
npm_package_count="$((platform_count + 1))" # the per-platform sub-packages plus the @zzop/cli shim

# --- Platform NAMES in the code maps, not just the count (2026-07-29) -------------------------------
# The three inventories above agree on HOW MANY. They said nothing about WHICH, and two runtime code maps
# carry the platform set by hand: `packages/cli/bin/zzop.js`'s PLATFORM_PACKAGES (what the shim looks for
# at install time) and `.claude-plugin/hooks/bootstrap.sh`'s uname case (what the plugin downloads).
# Neither was in any guard's subject set, so on the sixth platform's day the counts could stay equal while
# the shim failed to resolve a binary and the plugin install broke silently.
#
# Truth here is `packages/cli/npm/` — those directories ARE the sub-packages that publish. Each must be
# named by both maps. The maps spell the token differently on purpose (the shim keys by
# `process.platform-process.arch` and maps TO `@zzop/cli-<dir>`), so what is checked is that the
# sub-package NAME appears — the string that actually has to resolve.
#
# MATCHED AT A TOKEN BOUNDARY, not as a substring. A plain `grep -F` here is VACUOUS against the most
# likely edit: renaming `darwin-arm64` to `darwin-arm64-REMOVED` leaves the original as a prefix, so the
# search still hits and the guard stays green while the map is broken. Measured while writing this axis —
# the bootstrap half passed a planted violation until the boundary was added, which is the whole reason
# this repo plants one before believing a green.
platform_named() { # $1 = file, $2 = token
  grep -qE "(^|[^A-Za-z0-9_-])$2([^A-Za-z0-9_-]|\$)" "$1"
}
missing_map_entries=""
while IFS= read -r f; do
  [ -n "$f" ] || continue
  d="${f#packages/cli/npm/}"
  d="${d%%/*}"
  case " $seen_platform_dirs " in *" $d "*) continue ;; esac
  seen_platform_dirs="$seen_platform_dirs $d"
  platform_named "$CLI_SHIM" "@zzop/cli-$d" \
    || missing_map_entries="$missing_map_entries\n  $CLI_SHIM: no entry resolving to @zzop/cli-$d"
  platform_named "$PLUGIN_BOOTSTRAP" "$d" \
    || missing_map_entries="$missing_map_entries\n  $PLUGIN_BOOTSTRAP: platform token '$d' never named"
done < <(git ls-files -- packages/cli/npm)

if [ -n "$missing_map_entries" ]; then
  echo "$SELF: a published npm sub-package is missing from a runtime platform map:" >&2
  # shellcheck disable=SC2059
  printf "$missing_map_entries\n" >&2
  echo "  Each directory under packages/cli/npm/ publishes, so a platform absent from these maps means" >&2
  echo "  the shim cannot find its binary at install time, or the plugin bootstrap cannot download it." >&2
  exit 1
fi

# --- Supported LANGUAGES named on the distribution surfaces (2026-07-29) ----------------------------
# The shipped language set is `parser/parser-*/` — those directories are the parsers that exist. Four
# distribution surfaces enumerate it BY HAND: the plugin and .mcpb descriptions, the npm keywords, and
# the README table. None was in any guard's subject set: `check-deploy-facts-prose` counted packs, rules
# and platforms; `check-version-lists-parsers` reads `version.rs` only. So adding or removing a parser
# could leave a surface UNDER-advertising (a new language nowhere) or OVER-advertising (a removed one
# still claimed) — and over-advertising is a direct hit on the no-overclaiming doctrine.
#
# COMPARISON, not generation: three of the four are free prose, and rewriting a sentence from a list is a
# worse cure than checking it. The token is derived from the directory (`parser-python-3` -> `python`,
# `parser-java-21` -> `java`), so a new parser directory fails this until every surface names it.
# Boundary-matched for the same reason as the platform axis above — and here it also stops `go` from
# matching "GitHub"/"algorithm", which would make the whole axis vacuous for that language.
lang_surfaces="$CLI_PKG .claude-plugin/plugin.json packages/mcpb/manifest.json README.md"
missing_lang=""
for parser_dir in parser/parser-*/; do
  [ -d "$parser_dir" ] || continue
  slug="${parser_dir#parser/parser-}"
  slug="${slug%/}"
  slug="${slug%-[0-9]*}" # `python-3` -> `python`, `java-21` -> `java`
  # One alternation per language, so every surface is checked by a SINGLE grep. An alias is listed only
  # where the shipped spelling is not the slug case-insensitively — a future `parser-kotlin` needs no
  # entry, while one whose display name diverges fails loudly until it gets one rather than passing on a
  # coincidence. (`C#` also accepts the `csharp` slug, which is how the npm keywords spell it.)
  case "$slug" in
    csharp) alts='C#|csharp'; shown='C# (or the csharp slug)' ;;
    *) alts="$slug"; shown="$slug" ;;
  esac
  for surface in $lang_surfaces; do
    [ -f "$surface" ] || continue
    if grep -qiE "(^|[^A-Za-z0-9_-])($alts)([^A-Za-z0-9_-]|\$)" "$surface"; then
      continue
    fi
    missing_lang="$missing_lang\n  $surface: never names $shown (parser/parser-${slug}* exists)"
  done
done

if [ -n "$missing_lang" ]; then
  echo "$SELF: a shipped parser's language is missing from a distribution surface:" >&2
  # shellcheck disable=SC2059
  printf "$missing_lang\n" >&2
  echo "  These four surfaces advertise what zzop parses. A language that ships but is not named there is" >&2
  echo "  under-advertised; one named there that no longer ships is an overclaim. Truth is parser/parser-*/." >&2
  exit 1
fi

# --- Scan tracked *.md / *.html / *.rs --------------------------------------------------------------
bt='`'
prefilter='[0-9]+ (bundled )?DSL packs|[0-9]+ packs shipped|[0-9]+ DSL rules|[0-9]+ platforms?|[0-9]+ packages|\([0-9]+ rules?\)|ships [0-9]+ rules?|[0-9]+-rule'

candidate_files="$(tracked_files_matching "$prefilter" '*.md' '*.html' '*.rs' '.github/workflows/*.yml' 'scripts/*.sh')" \
  || abort "file enumeration failed"

total_files=0
while IFS= read -r f; do
  [ -n "$f" ] && total_files=$((total_files + 1))
done < <(git ls-files -- '*.md' '*.html' '*.rs' '.github/workflows/*.yml' 'scripts/*.sh')

# Subject-set floor (2026-07-29). Every derivation above aborts loudly on a broken parse (zero packs,
# zero platforms, a disagreeing inventory) — but the SCAN those numbers are compared against had no
# such floor, so a pathspec that stopped matching printed "clean (0 files scanned; 12 DSL packs, 138
# DSL rules, 5 platforms, 6 npm packages)" and exited 0. Measured 2026-07-29 by redirecting the globs
# in a scratch copy. That green is the worst shape available here: it recites four correct derived
# truths, which reads as proof, while having compared them against nothing.
if [ "$total_files" -eq 0 ]; then
  echo "$SELF: FAILED -- enumerated ZERO tracked *.md/*.html/*.rs files. The derived counts below were" >&2
  echo "  computed but compared against no prose at all, so this run proved nothing. An empty subject" >&2
  echo "  set is a broken guard, never a clean tree." >&2
  exit 1
fi

# Every claim shape is matched by ONE `grep -HnoE` pass over the candidate files and classified in-shell
# afterwards. The obvious alternative -- one grep per (file, shape) pair -- costs 3 + 3*<packs> process
# spawns per file and took over two minutes on Windows for 13 files; a pre-commit guard that slow gets
# disabled just as surely as one that cries wolf.
pack_alt="$(printf '%s|' "${pack_ids[@]}")"
pack_alt="${pack_alt%|}"
q="[${bt}\"]" # a pack id in a per-pack claim is always backtick- or double-quote-delimited
claim_pattern="[0-9]+ (bundled )?DSL packs?\\b|[0-9]+ packs shipped\\b|[0-9]+ DSL rules?\\b"
claim_pattern="$claim_pattern|[0-9]+ platforms?\\b|[0-9]+ packages\\b"
claim_pattern="$claim_pattern|${bt}($pack_alt)${bt} \\([0-9]+ rules?\\)"
claim_pattern="$claim_pattern|${q}($pack_alt)${q} ships [0-9]+ rules?"
claim_pattern="$claim_pattern|[0-9]+-rule ${q}($pack_alt)${q}"

files=()
while IFS= read -r f; do
  [ -n "$f" ] && files+=("$f")
done <<< "$candidate_files"

matches=""
if [ "${#files[@]}" -gt 0 ]; then
  matches="$(grep -HnoE "$claim_pattern" -- "${files[@]}" || true)"
fi

while IFS= read -r row; do
  [ -n "$row" ] || continue
  file="${row%%:*}"
  rest="${row#*:}"
  lineno="${rest%%:*}"
  text="${rest#*:}"

  # First digit run of the match, in pure bash (no subprocess). Unambiguous: every shape either leads
  # with its number or names a pack id first, and pack ids are asserted digit-free above.
  stated="${text#"${text%%[0-9]*}"}"
  stated="${stated%%[!0-9]*}"
  [ -n "$stated" ] || continue
  # Zero carve-out -- see the header. `0 DSL rules` / `(0 rules)` describe the degenerate case.
  [ "$stated" -eq 0 ] && continue

  expected=""
  noun=""
  case "$text" in
    *"DSL pack"*|*"packs shipped"*)
      expected="$pack_count"; noun="bundled DSL packs (tracked rules/dsl/**/*.json)" ;;
    *"DSL rule"*)
      expected="$rules_total"; noun="DSL rules across all bundled packs" ;;
    *platform*)
      expected="$platform_count"; noun="release platforms ($PREBUILD's build-job matrix)" ;;
    *packages*)
      # The only shape generic enough to need a second anchor: bind it to npm via the whole line.
      line="$(sed -n "${lineno}p" "$file")"
      case "$line" in
        *npm*|*NPM*|*Npm*) ;;
        *) continue ;;
      esac
      expected="$npm_package_count"; noun="npm packages (@zzop/cli plus its platform sub-packages)" ;;
    *)
      i=0
      while [ "$i" -lt "$pack_count" ]; do
        pid="${pack_ids[$i]}"
        case "$text" in
          *"${bt}${pid}${bt}"*|*"\"${pid}\""*)
            expected="${pack_rules[$i]}"; noun="rules in the bundled \`$pid\` pack"; break ;;
        esac
        i=$((i + 1))
      done
      [ -n "$expected" ] || abort "matched \"$text\" at $file:$lineno but could not bind it to a pack id -- classification is broken" ;;
  esac

  if [ "$stated" -ne "$expected" ]; then
    report "$file" "$lineno" "$stated" "$expected" "$noun"
  fi
done <<< "$matches"

if [ "$fail" -ne 0 ]; then
  echo "$SELF: FAILED -- the counts above are derived from the code on every run. Fix the prose to match" >&2
  echo "  the code, or (if the code is what is wrong) fix the code -- never widen this guard to hide a drift." >&2
  exit 1
fi

echo "$SELF: clean ($total_files files scanned; $pack_count DSL packs, $rules_total DSL rules, $platform_count platforms, $npm_package_count npm packages)."
