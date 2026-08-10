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

# Preconditions this repo does NOT ship (see docs/demo/break-a-route.md): `corpus/oss/` is gitignored, so
# the two trees must be supplied locally. Checked up front so a missing corpus is reported as a missing
# corpus, not misdiagnosed further down as a build/toolchain failure.
for tree in corpus/oss/fe-vite corpus/oss/be-express; do
  if [ ! -d "$tree" ]; then
    echo "!! missing '$tree' — this demo needs two frontend/backend trees you supply under corpus/oss/." >&2
    echo "   'corpus/oss/' is gitignored and nothing here fetches it; see docs/demo/break-a-route.md." >&2
    exit 1
  fi
done

# The baseline must actually BE the baseline. `corpus/oss/` is gitignored, so if an earlier run died
# before restoring, this file is still carrying the break — and then step 1 prints drift and calls it
# "the two repos agree", which is worse than failing. Refuse instead of measuring a lie.
if ! grep -q "router.put('/user'," "$BE_CTRL"; then
  echo "!! '$BE_CTRL' does not carry the pristine route \`router.put('/user',\`." >&2
  echo "   Either an earlier run of this script left the break in place, or your vendored copy differs" >&2
  echo "   from the RealWorld backend this demo is written against. Restore the file (re-clone the tree)" >&2
  echo "   before running: every number below step 1 is relative to that route." >&2
  exit 1
fi

# Always restore the corpus, even if the analysis fails or the script is interrupted.
#
# BY BYTE SNAPSHOT, not `git checkout --`. That is what this used to do, and it could never work:
# `corpus/oss/` is gitignored (.gitignore), so the file is UNTRACKED and `git checkout -- <path>`
# exits 1 with "did not match any file(s) known to git" — swallowed by the `2>/dev/null || true` that
# was there to make interruption safe. The user's vendored backend was left permanently edited by a
# documentation demo, and this repo's own corpus was found in exactly that state. A copy of the bytes
# depends on nothing about how the tree was obtained, which is the only assumption available for a
# tree the reader supplies.
SNAPSHOT="$(mktemp)"
cp "$BE_CTRL" "$SNAPSHOT"
restore() {
  # `cp` back rather than `mv`, so a second signal arriving mid-restore still leaves the snapshot on
  # disk to try again from; the temp file is removed only once the bytes are known to be back.
  if [ -f "$SNAPSHOT" ]; then
    cp "$SNAPSHOT" "$BE_CTRL" && rm -f "$SNAPSHOT"
  fi
}
trap restore EXIT

# Run the cross-layer join over the pair; on this repo's build the C-parser sources need a working
# cargo toolchain — if `cargo run` fails, say so instead of dying with a bare pipe error.
dump() {
  if ! cargo run --release -q -p zzop-engine --example xlayer_dump -- \
        corpus/oss/fe-vite corpus/oss/be-express 2>/dev/null; then
    echo "!! 'cargo run --example xlayer_dump' failed to build/run — this needs a SOURCE CHECKOUT with a" >&2
    echo "   working Rust toolchain (a released zzop binary cannot run this example)." >&2
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
