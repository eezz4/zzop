#!/usr/bin/env bash
# Break-a-route demo: change ONE backend route in an independently-authored RealWorld pair and watch
# zzop's cross-layer join catch the contract drift on BOTH sides — the frontend call that now hits
# nothing, and the backend route nobody calls — while the frontend still compiles and its tests pass.
#
# Usage (from the repo root):   bash docs/demo/break-a-route.sh
#
# The two source trees are independent repos vendored under corpus/oss/:
#   - fe-vite     (React + valtio + react-query, MirageJS-mocked)  — the frontend
#   - be-express  (Express + Prisma)                               — the backend
# Neither imports the other; the only thing that ties them together is the HTTP contract, which lives
# in runtime strings on both sides — invisible to either repo's own type-checker.
set -euo pipefail
cd "$(dirname "$0")/../.."

BE_CTRL="corpus/oss/be-express/src/app/routes/auth/auth.controller.ts"

# Always restore the corpus, even if the analysis fails or the script is interrupted.
restore() { git checkout -- "$BE_CTRL" 2>/dev/null || true; }
trap restore EXIT

# Run the cross-layer join over the pair; on this repo's build the C-parser sources need a working
# cargo toolchain — if `cargo run` fails, say so instead of dying with a bare pipe error.
dump() {
  if ! cargo run --release -q -p zzop-engine --example xlayer_dump -- \
        corpus/oss/fe-vite corpus/oss/be-express 2>/dev/null; then
    echo "!! 'cargo run --example xlayer_dump' failed to build/run — check your Rust toolchain." >&2
    exit 1
  fi
}

echo "### 1. Baseline — the two repos agree"
dump | grep -E "^=== edges|PUT /api/user" || true
echo

echo "### 2. The 'innocent' backend refactor: PUT /user  ->  PUT /users/me"
# One route-path change — the kind of REST tidy-up nobody reviews twice.
sed -i "s|router.put('/user',|router.put('/users/me',|" "$BE_CTRL"
grep -n "router.put(" "$BE_CTRL" | head -1
echo

echo "### 3. Re-run zzop — the contract drift is caught on BOTH sides"
dump | grep -E "^=== edges|^=== unprovided|^=== unconsumed|PUT /api/user|PUT /api/users/me" || true
echo

echo "### 4. Meanwhile the frontend is UNCHANGED"
echo "    corpus/oss/fe-vite/src/pages/Settings.jsx still reads:"
grep -n "axios.put" corpus/oss/fe-vite/src/pages/Settings.jsx || true
echo "    -> tsc/vite build sees a string literal '/user' and is perfectly happy;"
echo "       the MirageJS mock in src/server.js still mocks PUT /user, so the FE tests pass too."
echo "       Nothing in the frontend repo can observe that the backend moved the route."
echo
echo "(corpus restored on exit)"
