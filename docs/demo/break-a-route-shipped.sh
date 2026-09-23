#!/usr/bin/env bash
# Break-a-route, on a pair this repository SHIPS: rename one backend route and watch the cross-layer
# join name the drift on both sides -- the frontend call that now reaches nothing, and the backend
# route nobody calls.
#
# Usage (from the repo root):   bash docs/demo/break-a-route-shipped.sh
#
# ## Why this exists beside break-a-route.sh
#
# That script runs on two independently-authored RealWorld repos, which is the stronger evidence and
# the reason to keep it. It is also unreachable from a fresh clone: it needs `corpus/oss/`, which is
# gitignored and which no lane here fetches, and it builds a `cargo` example, so a released binary is
# not enough. Measured cost of that gap: 35 tags have been cut (`git tag | wc -l`) with this project's
# HEADLINE capability having no runnable public demonstration.
#
# This one runs on `docs/demo/pair/`, which is committed, and needs nothing but a `zzop` binary. It is
# smaller evidence on purpose -- four routes and calls, not two real applications -- and it says so
# rather than pretending otherwise. Read it as "the claim is real and you can watch it", and read the
# other one as "here it is on code neither I nor you wrote".
#
# ## Why the pair is shaped the way it is
#
# An earlier version of this pair had the frontend call `/api/profile` and the backend register
# `/api/profile`: identical strings, which an external reviewer correctly read as "two greps would do
# this", and which the demo never argued against. So the pair now puts the `/api` prefix on the MOUNT
# rather than on the routes, and asks for one dynamic segment that the two sides spell differently
# (`:id` against an interpolated `${id}`). The path the frontend calls now appears nowhere in the
# backend as text -- step 0 runs that search in front of you -- and the join still pairs them.
#
# ## It asserts rather than narrates
#
# Every number below is checked BEFORE it is printed, and a mismatch exits non-zero. A demo that only
# PRINTS is a document that rots: this repository shipped a README block whose six numbers had drifted
# from the run they claimed to be, through four releases, with every guard green.
#
# CI runs this on pushes to `main` and on pull requests (`.github/workflows/ci.yml`), and
# `scripts/ci-local.sh` runs it locally -- which is the only lane that sees it while work sits on a
# development branch.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

PAIR="docs/demo/pair"
BE_ROUTES_REL="be/src/routes.ts"
FE_PATH="/api/profile"
OLD_ROUTE="router.put('/profile', handler);"
NEW_ROUTE="router.put('/account', handler);"

command -v node > /dev/null || { echo "!! node is required (used to read the JSON replies)." >&2; exit 1; }

BIN="target/release/zzop"
[ -x "$BIN" ] || BIN="target/release/zzop.exe"
if [ ! -x "$BIN" ]; then
  if command -v zzop > /dev/null; then
    BIN="$(command -v zzop)"
  else
    echo "!! no zzop binary. Build one with 'cargo build --release -p zzop-cli-bin', or install @zzop/cli." >&2
    exit 1
  fi
fi

# Work on a COPY. The pair is committed, so editing it in place would leave the repo dirty if this
# script is interrupted -- and the edit is the whole point of the demo, so an interrupted run would
# leave a route renamed in git. A temp copy depends on nothing about how the repo was obtained.
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
cp -R "$PAIR/fe" "$WORK/fe"
cp -R "$PAIR/be" "$WORK/be"

echo "== 0. two trees, and they do not even spell the contract the same way ================"
echo "   frontend: $PAIR/fe      backend: $PAIR/be      (neither imports the other)"
grep -n "fetch('$FE_PATH'" "$WORK/fe/src/api.ts" | head -1 | sed 's/^/     fe /'
grep -n "$OLD_ROUTE" "$WORK/be/src/routes.ts" | head -1 | sed 's/^/     be /'
echo
echo "   Now search the WHOLE backend for the path the frontend actually calls:"
# grep exits 1 when it finds nothing, which is precisely the case being asserted here, so the
# non-match must not abort the script under `set -e` / pipefail.
hits="$(grep -ro "$FE_PATH" "$WORK/be/src/" | wc -l | tr -d ' ')" || hits=0
echo "     grep -r '$FE_PATH' be/src/   ->  $hits hits"
[ "$hits" = "0" ] || { echo "!! this demo claims that search finds nothing, and it found $hits." >&2; exit 1; }
echo "   The prefix arrives once, at the mount: app.use('/api', router). And one route takes an id"
echo "   the frontend interpolates. No text search can pair these two files."
echo

# `zzop init` writes the starter config into each tree -- the same first step any real user takes.
# Nothing here edits those configs afterwards: the join below is what the DEFAULTS find.
"$BIN" init "$WORK/fe" > /dev/null
"$BIN" init "$WORK/be" > /dev/null

echo "== 1. baseline: the two agree ========================================================"
"$BIN" cross "$WORK/fe" "$WORK/be" > "$WORK/before.json"
node "$PAIR/../report.mjs" "$WORK/before.json" baseline

echo
echo "== 2. rename ONE backend route ======================================================="
echo "   -  $OLD_ROUTE"
echo "   +  $NEW_ROUTE"
node -e '
const fs = require("fs");
const [p, a, b] = process.argv.slice(1);
const s = fs.readFileSync(p, "utf8");
if (s.split(a).length - 1 !== 1) { console.error("!! the route to rename was not found exactly once"); process.exit(1); }
fs.writeFileSync(p, s.split(a).join(b), "utf8");
' "$WORK/$BE_ROUTES_REL" "$OLD_ROUTE" "$NEW_ROUTE"
echo "   The frontend is untouched. It never referred to any backend symbol -- only to a path string"
echo "   that no longer resolves, which is exactly why nothing on its own side can notice."

echo
echo "== 3. what the join says now ========================================================="
"$BIN" cross "$WORK/fe" "$WORK/be" > "$WORK/after.json"
node "$PAIR/../report.mjs" "$WORK/after.json" drifted

echo
echo "== done. The temp copy is removed; docs/demo/pair/ was never edited. ================="
