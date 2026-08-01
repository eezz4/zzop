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
node scripts/site-graph-data.mjs "$tmp/n.ndjson" "$tmp/l.ndjson"
if ! git diff --quiet -- site/; then
  echo "site/ is stale — regenerating it changed these files:" >&2
  git --no-pager diff --stat -- site/ >&2
  echo "The regeneration has ALREADY been applied to your working tree; commit it." >&2
  fail "site regeneration diff"
fi
echo "site graph matches the tree."

# --- job: detection-benchmark --------------------------------------------------------------------
step "detection-benchmark: scripts/measure/detection-gate.sh"
bash scripts/measure/detection-gate.sh || fail "detection gate"

# --- job: site-render-check ----------------------------------------------------------------------
step "site-render-check: node scripts/site-render-check/check.mjs site"
node scripts/site-render-check/check.mjs site || fail "site render check"

printf '\n\033[1;32mci-local: every mirrored CI job passed.\033[0m\n'
printf 'Not covered here: the guards job (pre-commit owns it).\n'
