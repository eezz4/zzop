#!/usr/bin/env bash
# io-key kind-vocabulary guard — fails when the io-key kind list ("http routes, env keys, DB
# tables, topics") drifts between its SSOT and the two README rows that restate it. The drift
# class already happened once: packages/cli/README.md's endpoint row shipped without "DB tables".
#
# SSOT = the parenthesized list after "cross-layer io key (" in packages/mcp/src/tools/
# definitions.rs's check_endpoint tool description. Token-level only (same idiom as
# check-site-sdk-tokens.sh): every comma-separated vocabulary token must appear verbatim in BOTH
# packages/cli/README.md's `zzop endpoint` table row and crates/host/README.md's `check_endpoint`
# table row. Prose quality is NOT checked — only that the tokens agree with the SSOT.
set -euo pipefail
cd "$(dirname "$0")/.."

ssot=packages/mcp/src/tools/definitions.rs
[ -f "$ssot" ] || { echo "check-io-key-vocab: missing $ssot" >&2; exit 1; }
vocab="$(grep -oP 'cross-layer io key \(\K[^)]+' "$ssot" | head -n1 || true)"
[ -n "$vocab" ] || { echo "check-io-key-vocab: SSOT anchor 'cross-layer io key (' not found in $ssot — re-anchor this guard." >&2; exit 1; }

fail=0
check_row() { # $1 = file, $2 = table-row anchor (PCRE)
  local file="$1" anchor="$2" row tok
  row="$(grep -P "$anchor" "$file" | head -n1 || true)"
  if [ -z "$row" ]; then
    echo "check-io-key-vocab: no table row matching '$anchor' in $file — re-anchor this guard." >&2
    fail=1
    return
  fi
  local IFS=','
  for tok in $vocab; do
    tok="${tok# }"
    tok="${tok% }"
    case "$row" in
      *"$tok"*) ;;
      *) echo "check-io-key-vocab: $file's row lacks io-key kind token '$tok' (SSOT: $ssot says \"$vocab\")." >&2; fail=1 ;;
    esac
  done
}

check_row crates/host/README.md '^\|\s*`check_endpoint`'

if [ "$fail" -ne 0 ]; then
  echo "check-io-key-vocab: FAILED — sync the README rows with the SSOT vocabulary ($vocab)." >&2
  exit 1
fi
echo "check-io-key-vocab: OK (vocabulary: $vocab)"
