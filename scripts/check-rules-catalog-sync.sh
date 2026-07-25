#!/usr/bin/env bash
# Guards that site/rules.html has not drifted from docs/rules/catalog.md (the machine-checked SSOT).
#
# Three checks close the drift class that has actually bitten us — stale crate paths after a
# native-crate split, rules/ids added to the catalog but never mirrored onto the public site, and
# phantom filenames in the catalog itself:
#   1. PATHS (site ⊆ catalog): every `*.rs` source path shown on the site must also appear in the
#      catalog. The site may not keep a stale path (e.g. a pre-split `rules-graph/src/cross_layer/…`)
#      or invent one the catalog does not vouch for.
#   2. IDS (catalog → site): every DSL rule id and native-analysis id in the catalog must appear on the
#      site, so a newly-cataloged rule cannot ship undocumented.
#   3. PATHS (catalog ⊆ filesystem): every `*.rs` token in the catalog must resolve to a tracked file
#      (suffix match — tokens may be bare basenames or crate-relative fragments). Rule IDS are pinned
#      to the engine by crates/engine/tests/rule_contracts/, but nothing vouched for the catalog's
#      path prose: `dead.rs` / `reachability.rs` (real files: dead_candidates.rs / unreachable.rs)
#      passed checks 1-2 verbatim onto the public site (found 2026-07-16).
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

# --- Check 1: no site .rs path is absent from the catalog (site ⊆ catalog) ---
# Extract *.rs tokens using a path character class — backticks and <code> tags are outside the class, so
# they delimit the token cleanly in both Markdown and HTML.
catalog_paths="$(grep -oE '[A-Za-z0-9_./-]+\.rs' "$catalog" | sort -u)"
site_paths="$(grep -oE '[A-Za-z0-9_./-]+\.rs' "$site" | sort -u)"

stale="$(comm -23 <(printf '%s\n' "$site_paths") <(printf '%s\n' "$catalog_paths") || true)"
if [ -n "$stale" ]; then
  echo "check-rules-catalog-sync: site/rules.html references .rs paths not in docs/rules/catalog.md" >&2
  echo "  (stale or invented — align the site with the catalog SSOT):" >&2
  printf '    %s\n' $stale >&2
  fail=1
fi

# --- Check 2: every catalog rule/analysis id has its OWN ROW on the site (catalog → site) ---
# Catalog table data rows begin `| ` + a backtick-wrapped id; ids are lowercase [a-z0-9/_-].
# The site side is anchored to the row opener `<tr><td><code>id</code></td>`, not a bare
# `<code>id</code>`: the site's hand-written prose cross-references other rules by id, so a loose
# substring let ANOTHER rule's mention testify for a row that had been deleted. Live example on the
# day this was tightened: `jwt-sign-literal-secret`'s row names `hardcoded-secret`, so dropping
# hardcoded-secret's own row entirely would still have read green.
#
# Set comparison, not a grep per id: 181 ids meant 181 process spawns, ~40s of the guard's ~46s
# runtime under Windows msys — and this guard runs from pre-commit on every commit.
catalog_ids="$(grep -oE '^\| `[a-z0-9][a-z0-9/_-]*`' "$catalog" | sed -E 's/^\| `//; s/`$//' | sort -u)"
site_row_ids="$(grep -oE '<tr><td><code>[a-z0-9][a-z0-9/_-]*</code></td>' "$site" \
  | sed -E 's#^<tr><td><code>##; s#</code></td>$##' | sort -u)"
missing="$(comm -23 <(printf '%s\n' "$catalog_ids") <(printf '%s\n' "$site_row_ids") || true)"
if [ -n "$missing" ]; then
  echo "check-rules-catalog-sync: catalog rule/analysis ids missing from site/rules.html:" >&2
  printf '    %s\n' $missing >&2
  fail=1
fi

# --- Check 3: every catalog .rs token resolves to a tracked file (catalog ⊆ filesystem) ---
# A token vouches when some tracked path ends with it ("/token" or the token itself), so bare
# basenames (`graph.rs`) and crate-relative fragments (`scores/compute.rs`) both resolve.
unresolved=""
while IFS= read -r p; do
  [ -z "$p" ] && continue
  if [ -z "$(git -C "$repo_root" ls-files -- "$p" "*/$p")" ]; then
    unresolved="$unresolved $p"
  fi
done <<< "$catalog_paths"
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
catalog_triples="$(grep -oE '^\| `[a-z0-9][a-z0-9/_-]*` \| [a-z]+ \| [a-z][a-z-]*[a-z] \|' "$catalog" \
  | sed -E 's/^\| `//; s/` \| /|/; s/ \| /|/; s/ \|$//' | sort -u)"
site_triples="$(grep -oE '<tr><td><code>[a-z0-9][a-z0-9/_-]*</code></td><td><code>[a-z]+</code></td><td><code>[a-z][a-z-]*[a-z]</code></td>' "$site" \
  | sed -E 's#<tr><td><code>##; s#</code></td><td><code>#|#g; s#</code></td>$##' | sort -u)"

only_catalog="$(comm -23 <(printf '%s\n' "$catalog_triples") <(printf '%s\n' "$site_triples") || true)"
only_site="$(comm -13 <(printf '%s\n' "$catalog_triples") <(printf '%s\n' "$site_triples") || true)"
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

if [ "$fail" -ne 0 ]; then
  echo "check-rules-catalog-sync: FAILED — update site/rules.html to mirror docs/rules/catalog.md." >&2
  exit 1
fi

id_count="$(printf '%s\n' "$catalog_ids" | grep -c . || true)"
path_count="$(printf '%s\n' "$site_paths" | grep -c . || true)"
triple_count="$(printf '%s\n' "$catalog_triples" | grep -c . || true)"
echo "check-rules-catalog-sync: OK (${id_count} catalog ids have their own site row, ${path_count} site .rs paths vouched by catalog, catalog paths resolve, ${triple_count} DSL id|severity|matcher triples match both ways)"
