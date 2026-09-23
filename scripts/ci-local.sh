#!/usr/bin/env bash
# Run what CI runs, locally, in one command — "will this break CI?" answered before pushing.
#
# ## Why this exists (2026-08-01, measured)
# `ci.yml` has five jobs. The pre-commit hook covered exactly ONE of them (`guards`). The other four —
# `test` (fmt/clippy/cargo test), `cli-shim-test` (release build + the site-regeneration diff),
# `detection-benchmark`, `site-render-check` — had no local counterpart at all, so the only way to ask
# "did I break CI" was to hand-assemble the commands from the YAML.
#
# That is exactly how v0.28.0's tag went red, and the tag is immutable so it stays red forever:
#   * clippy — the local sweep ran `cargo clippy` WITHOUT `-D warnings`. CI runs it with. Two doc-lint
#     warnings that read as ignorable locally were errors there. Same tool, different flags, different
#     verdict; "I ran clippy" was not the same claim as "CI's clippy passes".
#   * site/ — the release-audit step that regenerates `site/` ran BEFORE two later fixes added a file,
#     and that file is a dep-graph node. The check that would have caught it lives only in CI, because
#     it needs a release build that no per-commit hook can afford.
#
# ## Why it is checked against ci.yml rather than trusted
# A hand-copied command list is a second owner of a fact, which is the drift class this repo keeps
# paying for — a mirror that silently stops mirroring is worse than no mirror, because it answers
# "CI passes" with confidence. So `scripts/check-ci-local-mirrors-ci.sh` binds the two: every
# non-guard `run:` command in `ci.yml` must appear here, and every command here must appear there.
# The guards job is deliberately NOT mirrored here — `pre-commit` already runs it on every commit and
# `check-guards-wired.sh` already binds that list, so duplicating it would be a third owner.
#
# ## Usage
#   bash scripts/ci-local.sh           # everything, in CI's order
#   bash scripts/ci-local.sh --fast    # skip the release build + the two jobs that need it
#
# On this machine the tree-sitter crates need the msvc toolchain; set CARGO to override.
set -euo pipefail
cd "$(dirname "$0")/.."

CARGO="${CARGO:-cargo}"
FAST=0
[ "${1:-}" = "--fast" ] && FAST=1

step() { printf '\n\033[1m=== %s\033[0m\n' "$1"; }
fail() { printf '\n\033[1;31mci-local: FAILED at %s\033[0m\n' "$1" >&2; exit 1; }

# --- job: guards ---------------------------------------------------------------------------------
# Not re-run here (see the header). Stated rather than silently skipped, so a reader of this output
# knows what it does and does not cover.
step "guards — skipped (pre-commit owns them; run: for f in scripts/check-*.sh; do bash \$f; done)"

# EXCEPT this one step of the guards job: it is a `run:` block, not a `scripts/check-*.sh`, so
# pre-commit does NOT run it and the skip above does not cover it. Same enumeration + zero-file
# abort as ci.yml (an unmatched git pathspec yields an EMPTY string, which is detectable; an
# unmatched shell glob yields a literal pattern node quietly accepts, `1..0` and exit 0).
step "guards: adapter example tests"
files="$(git ls-files -- 'examples/adapters/*/test/*.test.js' 'examples/adapters/*/test/*.test.mjs')"
[ -n "$files" ] || fail "adapter example tests enumerated ZERO files — a broken pathspec, never a clean tree"
node --test $files || fail "adapter example tests"

# --- job: test -----------------------------------------------------------------------------------
step "test: cargo fmt --all --check"
$CARGO fmt --all --check || fail "cargo fmt --all --check"

step "test: cargo clippy --workspace --all-targets -- -D warnings"
# The `-D warnings` is the whole point of mirroring this line. Without it the command is a different
# tool that happens to share a name.
$CARGO clippy --workspace --all-targets -- -D warnings || fail "cargo clippy (-D warnings)"

step "test: cargo test --workspace"
$CARGO test --workspace || fail "cargo test --workspace"

if [ "$FAST" = "1" ]; then
  printf '\n\033[1;33mci-local: --fast — skipped the release build, the site-regeneration diff and the site render check.\033[0m\n'
  printf 'Those are the two jobs that caught v0.28.0. Run without --fast before a tag.\n'
  exit 0
fi

# --- job: cli-shim-test --------------------------------------------------------------------------
step "cli-shim-test: cargo build -p zzop-cli-bin --release"
$CARGO build -p zzop-cli-bin --release || fail "release build"

step "cli-shim-test: site graph is regenerated from the tree it claims to draw"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
./target/release/zzop graph --domain dep --format cosmograph-nodes --config zzop.config.jsonc > "$tmp/n.ndjson"
./target/release/zzop graph --domain dep --format cosmograph-links --config zzop.config.jsonc > "$tmp/l.ndjson"
# Snapshot BEFORE regenerating, and compare against that — not against HEAD. The claim this step makes
# is "regenerating site/ changes nothing", and `git diff -- site/` does not test that claim on a dirty
# tree: it reports every uncommitted site/ edit, regeneration or not. That is not hypothetical — this
# script advertises itself as dirty-tree-safe thirteen lines below, and on 2026-08-06 a hand edit to
# site/usage.html (a new `graph --domain` value, which no generator owns) made this step fail on three
# consecutive runs while regeneration was provably a no-op each time. A guard that cannot be made green
# by doing the thing it asks for teaches people to skip it.
# BOTH paths. `site-graph-data.mjs` writes `site-src/graph/page.html` and nothing under `site/`
# (its own header says the shipped page moved to `gen-site.mjs` on 2026-08-18), so copying only
# `site/` compared the generator's output against a directory it does not write — this step could
# not fail for three weeks, and did not, while the committed page said 1,677 nodes for a 1,683-node
# tree. Review ledger V102.
cp -R site "$tmp/site-before"
mkdir -p "$tmp/site-src-before/graph"
cp site-src/graph/page.html "$tmp/site-src-before/graph/page.html"
node scripts/site-graph-data.mjs "$tmp/n.ndjson" "$tmp/l.ndjson"
if ! diff -rq "$tmp/site-before" site > "$tmp/site-drift" 2>&1 ||
  ! diff -q "$tmp/site-src-before/graph/page.html" site-src/graph/page.html >> "$tmp/site-drift" 2>&1; then
  echo "site/ is stale — regenerating it changed these files:" >&2
  cat "$tmp/site-drift" >&2
  echo "The regeneration has ALREADY been applied to your working tree; commit it." >&2
  fail "site regeneration diff"
fi
echo "site graph matches the tree."

step "cli-shim-test: zzop analyzes zzop — findings pinned at zero, with a canary"
# The findings half of the dogfood run (the step above is the dep-graph half). It plants a real
# violation in its probe target, requires the run to find it, reverts from the bytes it read itself,
# and only then trusts `findings.total == 0`. Restoring from those bytes rather than `git checkout --`
# is what keeps YOUR uncommitted work in that file — this script is meant to be run on a dirty tree.
bash scripts/measure/self-analysis-gate.sh || fail "self-analysis gate"

step "cli-shim-test: docs/demo/break-a-route-shipped.sh"
# The runnable half of the break-a-route demo. It ASSERTS the join state at each step, so it is a
# gate and not a narration: if the shipped pair or the join changes, this goes red instead of the
# page quietly describing a run that no longer happens. Rides this job for the release build above.
bash docs/demo/break-a-route-shipped.sh || fail "break-a-route demo"

step "cli-shim-test: CLI shim tests"
# The ONLY thing proving the @zzop/cli npm shim can spawn the native binary at all. Needs this
# job's release build — the shim's dev-fallback resolution checks target/release only, no debug
# fallback. Same enumeration + zero-file abort as the adapter step.
files="$(git ls-files -- 'packages/cli/test/*.test.js' 'packages/cli/test/*.test.mjs')"
[ -n "$files" ] || fail "CLI shim tests enumerated ZERO files — a broken pathspec, never a clean tree"
node --test $files || fail "CLI shim tests"

# --- job: detection-benchmark --------------------------------------------------------------------
step "detection-benchmark: scripts/measure/detection-gate.sh"
bash scripts/measure/detection-gate.sh || fail "detection gate"

step "detection-benchmark: scripts/measure/readme-result-block.sh"
# The README reproduction block is a COPY of numbers only a run knows, and it shipped stale in
# v0.34.0 with every guard green. With nothing staged this runs unconditionally, which is what a
# local mirror of a CI job should do.
bash scripts/measure/readme-result-block.sh || fail "README result block"

# --- job: site-render-check ----------------------------------------------------------------------
step "site-render-check: npm ci --prefix scripts/site-render-check"
# Installs from the committed lockfile (offline from npm's local cache once populated). The CI job's
# `npx ... playwright install --with-deps chromium` line is deliberately NOT mirrored: it downloads a
# browser, which locally is one-time setup rather than a per-run step — run it by hand once if
# check.mjs below cannot find chromium. That exemption is now NAMED and liveness-checked in
# `check-ci-local-mirrors-ci.sh` — until 2026-08-08 `npx` was not an extraction needle at all, so the
# line was invisible rather than exempted and this comment was the only thing standing in for the
# guard's own scope. Any OTHER `npx` command in ci.yml is compared like every other command.
npm ci --prefix scripts/site-render-check || fail "npm ci (site-render-check)"

step "site-render-check: node scripts/site-render-check/check.mjs site"
node scripts/site-render-check/check.mjs site || fail "site render check"

# --- verification stamp ---------------------------------------------------------------------------
# Written ONLY here, and only after every job above passed. Its reader is this repo's review-ledger
# meter (a maintainer-side script, not published), which printed "100% of my share" on 2026-09-07
# while the tree was red. Those are two different measurements: that percent counts
# REVIEW FINDINGS LANDED, and nothing in that view ever measured the tree. The meter cannot run this
# script (minutes), so it reads what this one leaves behind: which commit was verified, and when.
#
# It reports STALENESS, never "green" — a stamp records that a run happened at a sha, not a claim about
# the working tree now. .zzop/ is already gitignored, so the stamp is local by construction and cannot
# travel to another checkout to be read as a claim about that tree.
mkdir -p .zzop
printf '%s %s\n' "$(git rev-parse HEAD)" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" > .zzop/last-verified

printf '\n\033[1;32mci-local: every mirrored CI job passed.\033[0m\n'
printf 'Not covered here: the guards job (pre-commit owns it).\n'
