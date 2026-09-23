#!/usr/bin/env bash
# rule-pack-tests-if-touched.sh — run the rule-pack invariant tests when a DSL pack is staged.
#
# ## The defect this exists for (2026-09-09, review ledger V137)
#
# `b75a6dc8` widened three SQL rules' `file_pattern` to `.jsx`/`.mts`/`.cts` and left the fourth --
# `sql/destructive-migration`, whose pattern is PATH-scoped -- untouched. That split a promise the
# first rule's message makes: a `migrations/x.mts` carrying a `DELETE` without `WHERE` is excluded by
# `sql/delete-no-where` (migration path) and not admitted by `sql/destructive-migration` (extension),
# so it is silent under both. The test that states this invariant --
# `pack_sql::language_scope::destructive_migration_admits_every_extension_its_critical_siblings_exclude`
# -- went red in that commit and stayed red, because nothing on this side of a push runs it:
#
#   - the 51 guards in .githooks/pre-commit are text checks; none loads a pack and asserts on it
#   - the detection gate scored 258/258 with the gap present (cases/trees holds no .mts file at all)
#   - CI runs `cargo test` on main and PRs, and this branch is never pushed
#
# So the ONLY machine that could see it was `cargo test --workspace`, which is exactly the axis this
# repo's pre-commit deliberately omits as too slow. This script restores the narrowest useful piece of
# it: one crate, armed only when its subject is staged.
#
# ## Cost and trigger, measured
#
# MEASURED on this machine (2026-09-09): 92s warm for `-p zzop-rule-packs` (most of it linking the
# test binaries, not running them; pack_sql alone runs in 5.2s). `rules/dsl/` was staged in 15 of the
# 102 unpushed commits, so this is ~15% of commits, not every one.
#
# Deliberately NOT a scripts/check-*.sh: those are text guards that every commit affords. This one
# compiles, and the same reasoning that keeps detection-gate-if-touched.sh out of that fleet keeps
# this out of it.
#
# Escape hatch: ZZOP_SKIP_RULE_PACK_TESTS=1. If you use it, the invariants below went unchecked and
# nothing else in this hook will say so.
set -euo pipefail

cd "$(dirname "$0")/.."

STAGED=$(git diff --cached --name-only --diff-filter=ACMR | grep '^rules/dsl/' || true)
if [ -z "$STAGED" ]; then
  exit 0
fi

if [ "${ZZOP_SKIP_RULE_PACK_TESTS:-0}" = "1" ]; then
  echo "pre-commit: rule-pack tests SKIPPED by ZZOP_SKIP_RULE_PACK_TESTS -- pack invariants were NOT"
  echo "  checked against this commit. V137 is what that looks like when it is wrong."
  exit 0
fi

echo "pre-commit: rule-pack tests ARMED -- $(echo "$STAGED" | wc -l | tr -d ' ') staged pack file(s):"
echo "$STAGED" | sed 's/^/    /'
echo "  These carry the invariants no text guard can state: which extensions a rule admits, which"
echo "  paths it excludes, and whether a rule's message promises a disclosure a sibling never emits."
echo "  MEASURED wall clock: ~92s warm, more when the crate is cold. NOT hung."
echo "  To commit without it: ZZOP_SKIP_RULE_PACK_TESTS=1 git commit ...   (the invariants go unchecked)"

export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}"
if ! out=$(cargo test -p zzop-rule-packs 2>&1); then
  echo "pre-commit: rule-pack tests FAILED"
  echo "$out" | tail -40
  exit 1
fi
echo "pre-commit: rule-pack tests clean."
