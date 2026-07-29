#!/usr/bin/env bash
# Guards that site/rules.html has not drifted from docs/rules/catalog.md (the machine-checked SSOT).
#
# Three checks close the drift class that has actually bitten us — stale crate paths after a
# native-crate split, rules/ids added to the catalog but never mirrored onto the public site, and
# phantom filenames in the catalog itself:
#   1. PATHS (site ⊆ catalog): every `*.rs` source path shown on the site must also appear in the
#      catalog. The site may not keep a stale path (e.g. a pre-split `rules-graph/src/cross_layer/…`)
#      or invent one the catalog does not vouch for.
#   2. IDS (catalog <-> site, BOTH ways): every DSL rule id and native-analysis id in the catalog must
#      have its own row on the site, so a newly-cataloged rule cannot ship undocumented — and every
#      site rule row must be an id the catalog vouches for, so a native id cannot be invented on the
#      public site or outlive its own rename there (check 4's "invented or stale" verdict reaches DSL
#      rows only; see check 2's own comment).
#   3. PATHS (catalog ⊆ filesystem): every `*.rs` token in the catalog must resolve to a tracked file
#      (suffix match — tokens may be bare basenames or crate-relative fragments). Rule IDS are pinned
#      to the engine by crates/engine/tests/rule_contracts/, but nothing vouched for the catalog's
#      path prose: `dead.rs` / `reachability.rs` (real files: dead_candidates.rs / unreachable.rs)
#      passed checks 1-2 verbatim onto the public site (found 2026-07-16).
# Two more checks sit below those three: check 4 pins the DSL (id, severity, matcher) triple BOTH
# ways, and check 5 asserts the extraction census is nonzero — every one of checks 1-4 is a set
# difference, and the difference of two empty sets is empty, so a collapsed extraction would read as
# perfect agreement. Each check has its own comment at its own site.
# Hand-authored prose on the site is intentionally NOT checked — only the machine-derivable facts
# (ids + source paths).
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
catalog="$repo_root/docs/rules/catalog.md"
site="$repo_root/site/rules.html"

for f in "$catalog" "$site"; do
  [ -f "$f" ] || { echo "check-rules-catalog-sync: missing $f" >&2; exit 1; }
done

fail=0

# extract <label> <file> <ere> [sed -E script] -> sorted, de-duplicated matches on stdout.
#
# Why this is not a bare `grep -oE ... | sed | sort -u`: grep exits 1 for "no match" and >= 2 for a
# real error, and under `set -euo pipefail` a bare extraction pipeline conflates the two into a SILENT
# death — an extraction that matched nothing aborted this guard AT THE ASSIGNMENT LINE with no message
# at all, which in CI is indistinguishable from an infrastructure failure, and which is exactly the
# state check 5 below exists to explain. The obvious patch (`|| true`) is strictly worse: it swallows
# the real errors too, the same idiom scripts/lib/tracked-grep.sh was written to stop. So the status is
# captured and the two outcomes are told apart: empty is an ordinary result that flows on to check 5,
# a genuine grep failure aborts loudly here.
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
# four `comm -23 <(printf '%s\n' "$a") <(printf '%s\n' "$b")` calls and four `grep -c .` calls
# (2026-07-29). Each `comm` line cost three processes — the comm plus a forked subshell for each
# process substitution — so those eight lines were sixteen spawns, and on this box a spawn is the
# entire cost of this guard: its `user` time is a rounding error beside its `sys` time. comm needed
# sorted inputs; a hash does not, but every set here still arrives from `sort -u`, so the output
# order is unchanged and so is every message printed from it.
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

# --- Check 1: no site .rs path is absent from the catalog (site ⊆ catalog) ---
# Extract *.rs tokens using a path character class — backticks and <code> tags are outside the class, so
# they delimit the token cleanly in both Markdown and HTML.
catalog_paths="$(extract 'catalog .rs paths' "$catalog" '[A-Za-z0-9_./-]+\.rs')"
site_paths="$(extract 'site .rs paths' "$site" '[A-Za-z0-9_./-]+\.rs')"

stale="$(set_diff "$site_paths" "$catalog_paths")"
if [ -n "$stale" ]; then
  echo "check-rules-catalog-sync: site/rules.html references .rs paths not in docs/rules/catalog.md" >&2
  echo "  (stale or invented — align the site with the catalog SSOT):" >&2
  printf '    %s\n' $stale >&2
  fail=1
fi

# --- Check 2: catalog rule/analysis ids and site rule rows are the SAME SET (both directions) ---
# Catalog table data rows begin `| ` + a backtick-wrapped id; ids are lowercase [a-z0-9/_-].
# The site side is anchored to the row opener `<tr><td><code>id</code></td>`, not a bare
# `<code>id</code>`: the site's hand-written prose cross-references other rules by id, so a loose
# substring let ANOTHER rule's mention testify for a row that had been deleted. Live example on the
# day this was tightened: `jwt-sign-literal-secret`'s row names `hardcoded-secret`, so dropping
# hardcoded-secret's own row entirely would still have read green.
#
# BOTH directions since 2026-07-26. This was catalog → site only, and check 4 — the one place that
# flags a site id as "invented or stale" — pins DSL rows exclusively (its needle requires the matcher
# column, which the native tables do not have; see its own comment). So a NATIVE id could be invented
# on the public site, or survive there after being renamed/retired in the catalog, with nothing in the
# repo able to see it: the one-directional check counts a site row it never asked the catalog about,
# and check 4 never looks at native rows at all. Set equality here closes the native direction without
# adding a sixth check — the DSL half is redundant with check 4, and redundancy costs nothing against a
# set comparison that is already computed.
#
# The site needle requires the second cell to be `<code>`-wrapped too (`</code></td><td><code>`): every
# rule row — DSL (id|severity|matcher|prose) and native (id|severity|detects) alike — puts a code-wrapped
# severity there, while the site's MATCHER-KINDS glossary table (`line-scan`, `method-scan`,
# `symbol-scan`, `io-scan` at site/rules.html:463-466) uses the identical row opener with plain prose in
# its second cell. Those four are matcher names, not rule ids, and are correctly absent from the catalog;
# without this narrowing the new direction would false-red on them on day one. Measured 2026-07-26 with
# the narrowing in place: 183 ids on each side, zero difference either way.
#
# Set comparison, not a grep per id: 181 ids meant 181 process spawns, ~40s of the guard's ~46s
# runtime under Windows msys — and this guard runs from pre-commit on every commit.
catalog_ids="$(extract 'catalog rule/analysis ids' "$catalog" '^\| `[a-z0-9][a-z0-9/_-]*`' 's/^\| `//; s/`$//')"
site_row_ids="$(extract 'site rule-row ids' "$site" \
  '<tr><td><code>[a-z0-9][a-z0-9/_-]*</code></td><td><code>' \
  's#^<tr><td><code>##; s#</code></td><td><code>$##')"
missing="$(set_diff "$catalog_ids" "$site_row_ids")"
invented="$(set_diff "$site_row_ids" "$catalog_ids")"
if [ -n "$missing" ]; then
  echo "check-rules-catalog-sync: catalog rule/analysis ids missing from site/rules.html:" >&2
  printf '    %s\n' $missing >&2
  fail=1
fi
if [ -n "$invented" ]; then
  echo "check-rules-catalog-sync: site/rules.html rule rows for ids the catalog does not vouch for" >&2
  echo "  (invented, or stale after a rename/retirement — the public site is claiming a rule that" >&2
  echo "  docs/rules/catalog.md, which is machine-pinned to the engine, does not have):" >&2
  printf '    %s\n' $invented >&2
  fail=1
fi

# --- Check 3: every catalog .rs token resolves to a tracked file (catalog ⊆ filesystem) ---
# A token vouches when some tracked path ends with it ("/token" or the token itself), so bare
# basenames (`graph.rs`) and crate-relative fragments (`scores/compute.rs`) both resolve.
#
# ONE `git ls-files` for the whole tree, not one per token (2026-07-29). This was
# `git -C "$repo_root" ls-files -- "$p" "*/$p"` inside the loop: 45 catalog tokens meant 45 git
# processes, and on this box a process spawn is the entire cost — the guard measured 16.4s of which
# `user` time was a rounding error. Same trap check 2's comment above records for its own 181-spawn
# version, and the same fix.
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
  fail=1
fi

# --- Check 4: DSL rule (id, severity, matcher) triples agree BOTH ways (catalog <-> site) ---
# Checks 1-3 are all one-directional containment, which cannot see a severity or matcher that drifted
# on one side, nor a site row for a rule the catalog never had. Both surfaces are fully regular for
# DSL rules -- the catalog's per-pack tables are `| `id` | severity | matcher | prose |` and the site
# mirrors them as three `<code>` cells -- so set equality is affordable here and nowhere else.
# Native analyses are deliberately NOT in this check: their two tables carry different columns
# (`| Id | Default severity | Detects |`), so they have no third token to compare; check 2 covers
# their ids. Measured at introduction: 136 triples on each side, zero drift, zero migration.
catalog_triples="$(extract 'catalog DSL triples' "$catalog" \
  '^\| `[a-z0-9][a-z0-9/_-]*` \| [a-z]+ \| [a-z][a-z-]*[a-z] \|' \
  's/^\| `//; s/` \| /|/; s/ \| /|/; s/ \|$//')"
site_triples="$(extract 'site DSL triples' "$site" \
  '<tr><td><code>[a-z0-9][a-z0-9/_-]*</code></td><td><code>[a-z]+</code></td><td><code>[a-z][a-z-]*[a-z]</code></td>' \
  's#<tr><td><code>##; s#</code></td><td><code>#|#g; s#</code></td>$##')"

only_catalog="$(set_diff "$catalog_triples" "$site_triples")"
only_site="$(set_diff "$site_triples" "$catalog_triples")"
if [ -n "$only_catalog" ] || [ -n "$only_site" ]; then
  echo "check-rules-catalog-sync: DSL rule (id|severity|matcher) triples differ between catalog and site:" >&2
  if [ -n "$only_catalog" ]; then
    echo "  only in docs/rules/catalog.md (cataloged but not mirrored, or mirrored with other values):" >&2
    printf '    %s\n' $only_catalog >&2
  fi
  if [ -n "$only_site" ]; then
    echo "  only in site/rules.html (invented or stale):" >&2
    printf '    %s\n' $only_site >&2
  fi
  echo "  A severity/matcher that drifts on one side is a public lie about what the engine does." >&2
  fail=1
fi

# --- Check 5: the census is nonzero (every check above is a SET DIFFERENCE) ---
# comm of two empty sets is empty, so checks 1/2/4 all pass VACUOUSLY the moment an extraction stops
# matching: two surfaces that both went empty, or a `grep -oE` whose needle stopped fitting a
# regenerated layout, read exactly like two surfaces in perfect agreement. Check 3 has the same
# property (an empty catalog_paths resolves zero phantom paths). Until now these three counts were
# computed and PRINTED but never asserted — the same shape as the 22 zero-byte artifacts this repo
# once read as total success: the number was on screen and nothing was watching it.
# Nonzero is the assertion, deliberately not a pinned expected value: the catalog grows every release
# and a pinned number would be a second SSOT to forget. The exact-agreement question for DSL rows is
# owned by check-rule-desc-tokens.sh, which asserts pack rules == catalog rows == site rows.
id_count="$(count_lines "$catalog_ids")"
site_id_count="$(count_lines "$site_row_ids")"
path_count="$(count_lines "$site_paths")"
triple_count="$(count_lines "$catalog_triples")"
for pair in "catalog rule/analysis ids|$id_count" "site rule-row ids|$site_id_count" "site .rs paths|$path_count" "DSL id/severity/matcher triples|$triple_count"; do
  if [ "${pair##*|}" -eq 0 ]; then
    echo "check-rules-catalog-sync: extracted 0 ${pair%%|*} — every check in this guard is a set" >&2
    echo "  difference, and the difference of two empty sets is empty, so a collapsed extraction reads" >&2
    echo "  as perfect agreement. Fix the extraction (a regenerated catalog/site layout the grep -oE" >&2
    echo "  needles no longer fit), not this assertion." >&2
    fail=1
  fi
done

if [ "$fail" -ne 0 ]; then
  echo "check-rules-catalog-sync: FAILED — update site/rules.html to mirror docs/rules/catalog.md." >&2
  exit 1
fi

echo "check-rules-catalog-sync: OK (${id_count} catalog ids <-> ${site_id_count} site rule rows, same set both ways, ${path_count} site .rs paths vouched by catalog, catalog paths resolve, ${triple_count} DSL id|severity|matcher triples match both ways)"
