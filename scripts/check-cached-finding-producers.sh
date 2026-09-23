#!/usr/bin/env bash
# check-cached-finding-producers.sh — a cached per-file entry must have exactly TWO finding producers.
#
# ## What this is holding up (2026-09-06, review ledger V31)
#
# `cache::cache_relevant_disabled_rules` narrows which disabled rule ids move the per-file cache key,
# so that toggling a whole-graph rule no longer invalidates every file (measured: disabling `circular`
# turned 2,298 cache hits into 2,298 misses). That narrowing is only safe because the set of rules that
# can WRITE a cached per-file finding is knowable: `eval_packs` (DSL packs) and `schema_findings`
# (the schema-structural family), both in `pipeline/fresh.rs`.
#
# `vocabulary_fingerprint` refuses the same kind of narrowing next door, and its reason is right: a
# per-lane subset is normally "a hand-maintained CLAIM about which lane consumes what, and nothing makes
# the claim fail when a consumer moves". THIS FILE IS THAT SOMETHING. A third producer appended to the
# same `findings` vector would silently widen the id space the cache key must cover, and the failure it
# causes is the expensive direction: a warm cache serving a finding the current config disabled.
#
# ## Why a text scan of one function and not a runtime assertion
#
# The defect is a WRITE that gets added, not a value that comes out wrong. A runtime test can only see
# ids that a fixture happens to produce, so a new producer with no coverage passes it; the source is
# where the producer becomes visible the moment it exists. The cost is the usual one and is stated: this
# reads a shape, so a producer that reaches `findings` by another spelling (a helper that takes `&mut
# findings`, a second binding merged later) is outside what it can see. It prints what it matched for
# exactly that reason -- a reader who does not recognize the list has found the blind spot.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

SRC=crates/engine/src/pipeline/fresh.rs
[ -f "$SRC" ] || {
  echo "check-cached-finding-producers: missing $SRC -- re-anchor this guard." >&2
  exit 1
}

# Every line that binds or extends the artifact's `findings` vector.
matched="$(grep -nE '(let \([[:space:]]*mut findings|findings\.extend\()' "$SRC" | tr -d '\r' || true)"

if [ -z "$matched" ]; then
  echo "check-cached-finding-producers: found NO findings producer in $SRC -- the shape this guard" >&2
  echo "  reads has changed, and a guard that matches nothing reports green. Re-anchor it." >&2
  exit 1
fi

echo "check-cached-finding-producers: producers read ->"
echo "$matched" | sed 's/^/    /'

count="$(printf '%s\n' "$matched" | grep -c . || true)"
if [ "$count" -ne 2 ]; then
  echo "" >&2
  echo "check-cached-finding-producers: expected 2 producers, found $count." >&2
  echo "" >&2
  echo "A cached per-file entry's findings come from eval_packs and schema_findings, and" >&2
  echo "cache::cache_relevant_disabled_rules narrows the cache key on exactly that fact. If a third" >&2
  echo "producer is correct, widen that function's id derivation IN THE SAME COMMIT and update this" >&2
  echo "count -- otherwise a warm cache will serve findings the current config disabled." >&2
  exit 1
fi

for needle in 'eval_packs(' 'schema_findings('; do
  case "$matched" in
  *"$needle"*) ;;
  *)
    echo "" >&2
    echo "check-cached-finding-producers: the two producers are no longer $needle" >&2
    echo "  cache::cache_relevant_disabled_rules derives the cached id space from these two names." >&2
    exit 1
    ;;
  esac
done

echo "check-cached-finding-producers: exactly the two known producers."
