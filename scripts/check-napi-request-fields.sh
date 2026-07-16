#!/usr/bin/env bash
# napi AnalyzeRequest field-table guard — fails when docs/modules/napi.md's AnalyzeRequest field
# table drifts from the real wire contract (crates/facade/src/request.rs's `pub struct
# AnalyzeRequest`). The struct is #[serde(rename_all = "camelCase")], so struct snake_case field
# names are converted to camelCase before comparing.
#
# SET comparison, bidirectional: a struct field missing from the table and a table row naming no
# real field each fail loudly, so a rename shows as one missing + one extra. (A bare count pin was
# rejected: it cannot catch a rename, which keeps the count identical while both names are wrong.)
# Prose quality is NOT checked — only field-name parity.
set -euo pipefail
cd "$(dirname "$0")/.."

src=crates/facade/src/request.rs
doc=docs/modules/napi.md
[ -f "$src" ] || { echo "check-napi-request-fields: missing $src" >&2; exit 1; }
[ -f "$doc" ] || { echo "check-napi-request-fields: missing $doc" >&2; exit 1; }

# Struct-bounded extraction: only `pub <name>:` lines between `pub struct AnalyzeRequest {` and its
# closing brace (other request structs in the same file must not leak in), snake_case -> camelCase.
struct_fields="$(awk '/^pub struct AnalyzeRequest \{/{inside=1; next} inside && /^\}/{exit} inside' "$src" \
  | grep -oP '^\s*pub \K[a-z0-9_]+(?=:)' \
  | sed -E 's/_([a-z])/\U\1/g' | sort -u)"
[ -n "$struct_fields" ] || { echo "check-napi-request-fields: no pub fields extracted from $src's AnalyzeRequest — re-anchor this guard." >&2; exit 1; }

# Doc-table extraction: the first markdown table after the "`AnalyzeRequest` (" intro line; column 1
# backtick tokens only (the `| Field |` header and `|---|` separator carry no backtick token). Stops
# at the table's end (first non-`|` line once inside the table).
doc_fields="$(awk '/^`AnalyzeRequest` \(/{intro=1} intro && /^\|/{intable=1} intable && !/^\|/{exit} intable{print}' "$doc" \
  | grep -oP '^\|\s*`\K[A-Za-z0-9_]+(?=`)' | sort -u)"
[ -n "$doc_fields" ] || { echo "check-napi-request-fields: no field table found after the '\`AnalyzeRequest\` (' intro line in $doc — re-anchor this guard." >&2; exit 1; }

fail=0
missing="$(comm -23 <(printf '%s\n' "$struct_fields") <(printf '%s\n' "$doc_fields"))"
if [ -n "$missing" ]; then
  echo "check-napi-request-fields: AnalyzeRequest fields (in $src) missing from $doc's field table:" >&2
  printf '    %s\n' $missing >&2
  fail=1
fi
extra="$(comm -13 <(printf '%s\n' "$struct_fields") <(printf '%s\n' "$doc_fields"))"
if [ -n "$extra" ]; then
  echo "check-napi-request-fields: $doc's field table lists fields that AnalyzeRequest (in $src) does not have:" >&2
  printf '    %s\n' $extra >&2
  fail=1
fi

if [ "$fail" -ne 0 ]; then
  echo "check-napi-request-fields: FAILED — sync $doc's AnalyzeRequest field table with $src." >&2
  exit 1
fi
count="$(printf '%s\n' "$struct_fields" | wc -l | tr -d '[:space:]')"
echo "check-napi-request-fields: OK ($count fields, struct and doc table in sync)"
