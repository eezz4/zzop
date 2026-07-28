#!/usr/bin/env bash
# detection-gate.sh — score the labeled detection benchmark and FAIL when it regresses.
#
# This is the CI gate that `cases/` was committed for (2026-07-26) and that nothing ran until
# 2026-07-28. Until now `snapshot.mjs` and `benchmark.mjs` both declared themselves "NOT A GUARD, AND
# NOT WIRED INTO CI" in their own headers, and they were right: a 298-line adjudicated ground truth sat
# in the tree and no run compared anything to it. A benchmark nobody scores is a document, not a gate.
#
# ## Why this is not `scripts/check-*.sh`
# `check-guards-wired.sh` requires every `scripts/check-*.sh` to run in BOTH `.githooks/pre-commit` and
# `ci.yml`. This gate needs a RELEASE build of `zzop-mcp` plus a two-axis run over 15 trees — minutes,
# not the seconds a pre-commit hook may take. Naming it `check-*` would either lie to that meta-guard or
# make every commit unbearable. It is a CI job instead, and it stays out of the guard glob on purpose.
#
# ## Why the corpus is COPIED out of the repo first
# One fixture cannot be committed: `security/vendor-token-committed` only fires on a CONTIGUOUS
# `sk_live_`-shaped literal, which is exactly what GitHub push protection and
# `check-vendor-token-literals.sh` (no escape hatch, by design) both refuse. So the file is gitignored,
# and — measured 2026-07-27, recorded in `.gitignore` — the ANALYZER HONORS .gitignore (ancestor ignores
# included), which means recreating it at its own path changes nothing. It stays invisible.
#
# The `.gitignore` note left this as an open question for whoever wired the gate: synthesize the file
# OUTSIDE the ignored path, or drop the expectation and accept the coverage loss. This script takes the
# first, because the second silently narrows what the benchmark can claim — and because the outcome was
# already measured: "the same corpus copied outside the repo, fixture present, scores 143/143 with zero
# FP". Copying to a temp dir puts the tree beyond every ancestor .gitignore, so the synthesized fixture
# is actually walked.
#
# The generator below writes a contiguous literal while its OWN source never contains one: the vendor
# prefix and the token body are separate shell strings, concatenated at write time. That is the same
# trick `concat!` gives Rust fixtures, and it is why this file passes the vendor-token guard it exists
# to exercise.
#
# usage: bash scripts/measure/detection-gate.sh <path-to-zzop-mcp-binary>
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

bin="${1:-}"
if [ -z "$bin" ] || [ ! -x "$bin" ]; then
  echo "detection-gate: usage: bash scripts/measure/detection-gate.sh <zzop-mcp binary>" >&2
  echo "  (build it first: cargo build -p zzop-mcp --release)" >&2
  exit 1
fi
bin="$(cd "$(dirname "$bin")" && pwd)/$(basename "$bin")"

# Path updated in the same commit that moves the benchmark to its own root directory.
cases_dir=cases
expected="$cases_dir/EXPECTED.jsonc"
for p in "$cases_dir" "$expected"; do
  if [ ! -e "$p" ]; then
    echo "detection-gate: missing $p — the labeled benchmark is what this gate scores." >&2
    exit 1
  fi
done

work="$(mktemp -d)"
# `mktemp -d` lands outside the repository, which is the whole point (see the header): an ancestor
# .gitignore would otherwise keep the synthesized fixture invisible to the walker.
trap 'rm -rf "$work"' EXIT

mkdir -p "$work/corpus"
cp -R "$cases_dir/." "$work/corpus/"
# Any cache the local tree happens to carry must not ride along — a warm `.zzop/` from a different
# binary would be read as this run's own, and the first thing a stale cache does is look like a clean
# score.
find "$work/corpus" -type d -name '.zzop' -prune -exec rm -rf {} + 2>/dev/null || true

# The one fixture the tracked tree cannot hold. Prefix and body are separate strings HERE and one
# contiguous literal THERE — see the header for why both halves of that sentence matter.
secret_prefix='sk_live_'
secret_body='51a9Xk2mQd7RtY0pLvB3nZ8w'
fixture="$work/corpus/trees/api-be/services/be-security.hardcoded-secret.ts"
mkdir -p "$(dirname "$fixture")"
cat > "$fixture" <<EOF
// be-security/hardcoded-secret — bad: a secret-shaped literal assigned to a secret-named key. good:
// injected via a config object (no literal in source).
export const bad = { apiKey: '${secret_prefix}${secret_body}' };

export function good(cfg: { apiKey: string }) {
  return cfg.apiKey;
}
EOF

echo "detection-gate: scoring $cases_dir through $(basename "$bin")"
node scripts/measure/snapshot.mjs \
  --label ci \
  --bin "$bin" \
  --config "$work/corpus/zzop.config.jsonc" \
  --runs "$work/runs"

# `benchmark.mjs`'s exit code IS the verdict — it is nonzero on any FN or FP, and it prints each one
# with its anchor. No re-derivation of the score here: a second opinion about what counts as a
# regression is a second owner of the ground truth.
node scripts/measure/benchmark.mjs \
  --run "$work/runs/ci" \
  --expected "$expected"
