#!/usr/bin/env bash
# Guards that site/rules.html has not drifted from docs/rules/catalog.md (the machine-checked SSOT).
#
# THE ROWS ARE NO LONGER COMPARED — THEY ARE GENERATED (2026-08-09).
# Until today this guard compared two hand-written surfaces token by token: rule ids both ways, and
# the DSL (id, severity, matcher) triple both ways. Those checks were correct and they were green,
# and the page was still wrong: 40 of 203 rows carried PROSE that no longer matched the catalog, one
# of them for two weeks (`Changemeplease` entered the catalog 2026-07-26 and never reached the site),
# and one row shipped literal markdown backticks to the public page. A token comparison structurally
# cannot see that, because the drift is in the column that has no tokens to compare.
#
# So the second copy is gone instead of being watched: scripts/gen-site-rules.mjs derives every
# `<tbody>` row of every rule table on that page from the catalog's own tables, and check 1 below
# asserts the committed file IS that output. Every property the deleted checks asserted now holds by
# construction:
#   * ids agree BOTH ways        — the rows are written from the catalog's rows, so a cataloged rule
#                                  cannot be missing and a site row cannot be invented;
#   * severity and matcher agree — same reason, same pass;
#   * a renamed/retired id cannot outlive itself on the site — the row is rewritten, not diffed.
# Deleting them with the same commit that added the derivation is the point: a guard left watching a
# value that is now derived is a ghost, and the next reader cannot tell which of the two is the SSOT.
#
# WHAT STILL NEEDS A CHECK HERE (generation cannot answer these)
#   1. GENERATED (site == generator output). The whole row surface, prose included.
#   2. PATHS (site ⊆ catalog). The rows are derived, so this now has exactly one live subject: a `*.rs`
#      path written into the page's HAND-WRITTEN prose. That subject is EMPTY today (measured
#      2026-08-09: 46 `.rs` tokens on the page, all 46 inside generated rows, 0 in prose), so this is a
#      standing net rather than a check that can fail against today's file. Kept because prose is still
#      hand-written and nothing stops the next paragraph from naming a crate path.
#   3. PATHS (catalog ⊆ filesystem). Untouched by generation, and the one check here that ever caught a
#      real defect on its own: `dead.rs` / `reachability.rs` (real files: dead_candidates.rs /
#      unreachable.rs) passed every other check verbatim onto the public site (found 2026-07-16). With
#      the rows derived this is now STRONGER than it was — every path the site publishes comes from the
#      catalog, so vouching for the catalog's tokens vouches for the site's.
#   4. CENSUS (nonzero). Checks 2 and 3 are set differences, and the difference of two empty sets is
#      empty, so a collapsed extraction reads as perfect agreement.
# Hand-written prose on the page — the intro paragraphs, the native-analyses text, the matcher glossary
# table under `<section id="matchers">`, the custom-pack example — is intentionally NOT checked here.
# (The glossary rows are Matcher enum VARIANTS, not rules. They are compared to that enum — both
# directions, with the wire spelling derived from serde kebab-case — by check-matcher-glossary-sync.sh,
# which exists because the needles here structurally cannot see a two-cell row. Until 2026-08-09 that
# line read "nothing in this repo compares them to the enum", and the table was two rows short.)
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
catalog="$repo_root/docs/rules/catalog.md"
site="$repo_root/site/rules.html"
generator="$repo_root/scripts/gen-site-rules.mjs"

for f in "$catalog" "$site" "$generator"; do
  [ -f "$f" ] || { echo "check-rules-catalog-sync: missing $f" >&2; exit 1; }
done

fail=0

# extract <label> <file> <ere> [sed -E script] -> sorted, de-duplicated matches on stdout.
#
# Why this is not a bare `grep -oE ... | sed | sort -u`: grep exits 1 for "no match" and >= 2 for a
# real error, and under `set -euo pipefail` a bare extraction pipeline conflates the two into a SILENT
# death — an extraction that matched nothing aborted this guard AT THE ASSIGNMENT LINE with no message
# at all, which in CI is indistinguishable from an infrastructure failure, and which is exactly the
# state the census check below exists to explain. The obvious patch (`|| true`) is strictly worse: it
# swallows the real errors too, the same idiom scripts/lib/tracked-grep.sh was written to stop. So the
# status is captured and the two outcomes are told apart: empty is an ordinary result that flows on to
# the census, a genuine grep failure aborts loudly here.
# Must be called from a TOP-LEVEL assignment and never nested inside another pipeline: `exit 1` from a
# command substitution exits that subshell, and only a top-level assignment lets `set -e` see it.
extract() {
  local label="$1" file="$2" ere="$3" post="${4:-}" out rc
  set +e
  out="$(grep -oE "$ere" "$file")"
  rc=$?
  set -e
  if [ "$rc" -gt 1 ]; then
    echo "check-rules-catalog-sync: grep failed extracting $label from $file (exit $rc) — aborting" >&2
    echo "  rather than compare two surfaces from a scan that never ran." >&2
    exit 1
  fi
  [ "$rc" -eq 0 ] || return 0
  if [ -n "$post" ]; then
    printf '%s\n' "$out" | sed -E "$post" | sort -u
  else
    printf '%s\n' "$out" | sort -u
  fi
}

# set_diff <A> <B> — the lines of A that are not in B, in A's own (already sorted) order.
# count_lines <set> — how many non-empty lines it has.
#
# Both are pure bash (associative array + a read loop), and that is the whole point: they replaced
# `comm -23 <(printf '%s\n' "$a") <(printf '%s\n' "$b")` calls and `grep -c .` calls (2026-07-29).
# Each `comm` line cost three processes — the comm plus a forked subshell for each process
# substitution — and on this box a spawn is the entire cost of this guard: its `user` time is a
# rounding error beside its `sys` time. comm needed sorted inputs; a hash does not, but every set here
# still arrives from `sort -u`, so the output order is unchanged and so is every message printed from it.
set_diff() {
  local -A inb=()
  local x
  while IFS= read -r x; do [ -n "$x" ] && inb["$x"]=1; done <<< "$2"
  while IFS= read -r x; do
    [ -n "$x" ] || continue
    [ -n "${inb[$x]:-}" ] || printf '%s\n' "$x"
  done <<< "$1"
}

count_lines() {
  local n=0 x
  while IFS= read -r x; do [ -n "$x" ] && n=$((n + 1)); done <<< "$1"
  printf '%s' "$n"
}

# --- Check 1: the site's rule rows ARE the generator's output ---------------------------------------
# One command decides the whole row surface, prose included. `--check` never writes; the failure output
# names each differing row id and the column at which the two texts part (a whole-file diff is useless
# here — a single row runs past 3500 columns).
#
# node is REQUIRED, not optional. Skipping this check when node is absent would turn the one check that
# reads the prose into a no-op on exactly the machine that cannot run it, and a guard that skips is a
# guard that is off. Same stance, same wording, as check-license-shipping.sh takes for its own
# generator.
if ! command -v node >/dev/null 2>&1; then
  echo "check-rules-catalog-sync: \`node\` not found, so site/rules.html's generated rule rows could" >&2
  echo "  not be verified against docs/rules/catalog.md. This guard does not skip a check it cannot" >&2
  echo "  run — a skipped check is an off check. (No cargo, no network and no npm install are needed" >&2
  echo "  here; only \`node\` and \`git\` are.)" >&2
  fail=1
elif ! gen_out="$(node "$generator" --check 2>&1)"; then
  printf '%s\n' "$gen_out" >&2
  fail=1
else
  gen_summary="$gen_out"
fi

# --- Check 2: no site .rs path is absent from the catalog (site ⊆ catalog) ---
# Extract *.rs tokens using a path character class — backticks and <code> tags are outside the class, so
# they delimit the token cleanly in both Markdown and HTML. See the header for why this check's live
# subject is now the page's hand-written prose only.
catalog_paths="$(extract 'catalog .rs paths' "$catalog" '[A-Za-z0-9_./-]+\.rs')"
site_paths="$(extract 'site .rs paths' "$site" '[A-Za-z0-9_./-]+\.rs')"

stale="$(set_diff "$site_paths" "$catalog_paths")"
if [ -n "$stale" ]; then
  echo "check-rules-catalog-sync: site/rules.html references .rs paths not in docs/rules/catalog.md" >&2
  echo "  (stale or invented — the rule rows are generated from the catalog, so this is hand-written" >&2
  echo "  prose naming a path the catalog does not vouch for):" >&2
  printf '    %s\n' $stale >&2
  fail=1
fi

# --- Check 3: every catalog .rs token resolves to a tracked file (catalog ⊆ filesystem) ---
# A token vouches when some tracked path ends with it ("/token" or the token itself), so bare
# basenames (`graph.rs`) and crate-relative fragments (`scores/compute.rs`) both resolve.
#
# ONE `git ls-files` for the whole tree, not one per token (2026-07-29). This was
# `git -C "$repo_root" ls-files -- "$p" "*/$p"` inside the loop: 45 catalog tokens meant 45 git
# processes, and on this box a process spawn is the entire cost — the guard measured 16.4s of which
# `user` time was a rounding error.
#
# The rewrite is a restatement of what those two pathspecs MEAN, not a new rule. `-- "$p"` matches a
# tracked path equal to the token; `-- "*/$p"` matches one ending in "/" + token (git pathspec
# wildcards are matched without FNM_PATHNAME, so a single `*` spans directory separators). So the
# question is exactly "is the token a whole tracked path, or a tracked path's suffix at a `/`
# boundary", which is decided here by hashing every `/`-boundary suffix of every tracked path once
# and looking each token up. Deliberately NOT also accepting a token as a directory PREFIX (something
# `-- "$p"` would technically do for a directory literally named `foo.rs`): that case cannot occur —
# the extractor only emits `*.rs` tokens — and widening the accept set is how a phantom filename gets
# vouched for.
#
# The tracked list is materialized and asserted non-empty BEFORE awk sees it, and that assertion is
# load-bearing rather than decorative. The two-input awk idiom below tells its inputs apart with
# `FNR == NR`, which is true for the first file — and ALSO true for every record of the second file
# when the first one is empty. So an empty `git ls-files` would not make this check fail loudly; it
# would quietly load the CATALOG tokens into `full[]`, find every token present in it, and report
# zero phantom paths. A green. That is the same silent-empty-scan class every other floor in this
# repo exists to close, and it is a hazard this rewrite introduced (the per-token loop it replaced
# had no such state to confuse — an empty ls-files made every token unresolved, i.e. loud).
tracked_list="$(git -C "$repo_root" ls-files)"
if [ -z "$tracked_list" ]; then
  echo "check-rules-catalog-sync: git ls-files produced ZERO tracked paths — check 3 would have" >&2
  echo "  nothing to resolve catalog .rs tokens against and would read as green. Aborting rather" >&2
  echo "  than report a verdict from an empty scan." >&2
  exit 1
fi
unresolved="$(awk '
  FNR == NR {
    full[$0] = 1
    s = $0
    while ((i = index(s, "/")) > 0) { s = substr(s, i + 1); suffix[s] = 1 }
    next
  }
  $0 == "" { next }
  ($0 in full) || ($0 in suffix) { next }
  { printf " %s", $0 }
' <(printf '%s\n' "$tracked_list") <(printf '%s\n' "$catalog_paths"))"
if [ -n "$unresolved" ]; then
  echo "check-rules-catalog-sync: catalog .rs tokens that match no tracked file (phantom filenames):" >&2
  printf '    %s\n' $unresolved >&2
  echo "  The rule rows on the public site are generated from these tokens, so a phantom here ships." >&2
  fail=1
fi

# --- Check 4: the census is nonzero (checks 2 and 3 are SET DIFFERENCES) ---
# The difference of two empty sets is empty, so both of those pass VACUOUSLY the moment an extraction
# stops matching: two surfaces that both went empty read exactly like two surfaces in perfect
# agreement. Until 2026-07-29 these counts were computed and PRINTED but never asserted — the same
# shape as the 22 zero-byte artifacts this repo once read as total success.
# Nonzero is the assertion, deliberately not a pinned expected value: the catalog grows every release
# and a pinned number would be a second SSOT to forget. The exact-agreement question for row COUNTS is
# owned by check 1, which regenerates them.
path_count="$(count_lines "$site_paths")"
catalog_path_count="$(count_lines "$catalog_paths")"
for pair in "site .rs paths|$path_count" "catalog .rs paths|$catalog_path_count"; do
  if [ "${pair##*|}" -eq 0 ]; then
    echo "check-rules-catalog-sync: extracted 0 ${pair%%|*} — checks 2 and 3 are set differences, and" >&2
    echo "  the difference of two empty sets is empty, so a collapsed extraction reads as perfect" >&2
    echo "  agreement. Fix the extraction (a regenerated catalog/site layout the grep -oE needles no" >&2
    echo "  longer fit), not this assertion." >&2
    fail=1
  fi
done

if [ "$fail" -ne 0 ]; then
  echo "check-rules-catalog-sync: FAILED — regenerate the site from the catalog SSOT:" >&2
  echo "    node scripts/gen-site-rules.mjs" >&2
  exit 1
fi

echo "check-rules-catalog-sync: OK (${gen_summary#gen-site-rules: OK -- }; ${path_count} site .rs paths vouched by catalog, ${catalog_path_count} catalog paths all resolve to tracked files)"
