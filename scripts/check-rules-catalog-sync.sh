#!/usr/bin/env bash
# Guards that site/rules.html has not drifted from docs/rules/catalog.md (the machine-checked SSOT).
#
# Two asymmetric checks close the drift class that has actually bitten us — stale crate paths after a
# native-crate split, and rules/ids added to the catalog but never mirrored onto the public site:
#   1. PATHS (site ⊆ catalog): every `*.rs` source path shown on the site must also appear in the
#      catalog. The site may not keep a stale path (e.g. a pre-split `rules-graph/src/cross_layer/…`)
#      or invent one the catalog does not vouch for.
#   2. IDS (catalog → site): every DSL rule id and native-analysis id in the catalog must appear on the
#      site, so a newly-cataloged rule cannot ship undocumented.
# The catalog is itself pinned to the engine by crates/engine/tests/rule_contracts.rs, so this check
# transitively pins the public site to reality. Hand-authored prose on the site is intentionally NOT
# checked — only the machine-derivable facts (ids + source paths).
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

# --- Check 2: every catalog rule/analysis id appears on the site (catalog → site) ---
# Catalog table data rows begin `| ` + a backtick-wrapped id; ids are lowercase [a-z0-9/_-].
catalog_ids="$(grep -oE '^\| `[a-z0-9][a-z0-9/_-]*`' "$catalog" | sed -E 's/^\| `//; s/`$//' | sort -u)"
missing=""
while IFS= read -r id; do
  [ -z "$id" ] && continue
  grep -qF "<code>$id</code>" "$site" || missing="$missing $id"
done <<< "$catalog_ids"
if [ -n "$missing" ]; then
  echo "check-rules-catalog-sync: catalog rule/analysis ids missing from site/rules.html:" >&2
  printf '    %s\n' $missing >&2
  fail=1
fi

if [ "$fail" -ne 0 ]; then
  echo "check-rules-catalog-sync: FAILED — update site/rules.html to mirror docs/rules/catalog.md." >&2
  exit 1
fi

id_count="$(printf '%s\n' "$catalog_ids" | grep -c . || true)"
path_count="$(printf '%s\n' "$site_paths" | grep -c . || true)"
echo "check-rules-catalog-sync: OK (${id_count} catalog ids present on site, ${path_count} site .rs paths vouched by catalog)"
