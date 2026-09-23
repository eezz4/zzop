#!/usr/bin/env bash
# Guard: `scripts/ci-local.sh` must mirror every non-guard command `ci.yml` runs — both directions.
#
# ## Why
# `ci-local.sh` exists so "will this break CI?" is one command instead of a hand-assembly from YAML.
# A mirror that silently stops mirroring is WORSE than no mirror: it answers "CI passes" with
# confidence while omitting the step that fails. v0.28.0's tag went red on exactly that shape — a
# local clippy run without CI's `-D warnings` read as green and the tag is immutable, so it stays red.
#
# So the two files are bound here, both ways:
#   ci.yml has a command ci-local.sh lacks  -> a CI job with no local counterpart, silently
#   ci-local.sh has a command ci.yml lacks  -> local runs something CI does not, and the extra
#                                             failure trains people to ignore this script
#
# ## What counts as a "command"
# The `cargo`/`node`/`npm ci`/`bash scripts/...`/`bash docs/...` invocations in `ci.yml`'s `run:`
# steps. `node` covers
# BOTH shapes ci.yml actually runs — `node scripts/<file>` and `node --test $files` — because until
# 2026-08-03 the extractor knew only the first, and the three test-runner commands (two `node --test`
# steps, one `npm ci`) sat in ci.yml with no local counterpart while this guard reported "mirrored
# both ways". A needle list that is narrower than the claim is the same defect this guard exists to
# catch, one level down. It happened a SECOND time on 2026-09-02: a demo wired into ci.yml as
# `bash docs/demo/break-a-route-shipped.sh` was invisible to this needle, and an external reviewer
# proved it by deleting the mirroring line from ci-local.sh and watching this guard report clean.
# `bash docs/` is now a needle for the same reason `bash scripts/measure/` is: a demo that gates CI
# is work, not plumbing. Deliberately excluded:
#   * `bash scripts/check-*.sh` — the guards job. `pre-commit` runs those on every commit and
#     `check-guards-wired.sh` already binds that list; mirroring them here would be a third owner.
#   * `git`/`echo`/`set` plumbing inside multi-line steps — not the work, just the shell around it.
#   * the `zzop graph` invocations inside the site-regeneration step — they are that step's INTERNALS,
#     and both files spell them; comparing them would compare a temp-file path.
set -euo pipefail
cd "$(dirname "$0")/.."

CI=".github/workflows/ci.yml"
LOCAL="scripts/ci-local.sh"

for f in "$CI" "$LOCAL"; do
  [ -f "$f" ] || { echo "check-ci-local-mirrors-ci: $f is missing" >&2; exit 1; }
done

# Normalize a command line to its comparable core: collapse whitespace, drop a leading `run: `.
normalize() { sed -E 's/^[[:space:]]*-?[[:space:]]*run:[[:space:]]*//; s/[[:space:]]+/ /g; s/^ //; s/ $//'; }

# The comparison is a MULTISET, not a set. `sort -u` here until 2026-09-12 (review ledger V167), and
# the dedup was itself a blind spot: normalization maps ci.yml's TWO `node --test $files` steps --
# the adapter-kit examples and the npm shim spawn test -- onto one identical string, so `-u` folded
# them and the guard could not tell two steps from one. Measured: deleting ci-local.sh's CLI-shim
# block left this guard printing `clean (11 ... mirrored both ways)`. What goes missing that way is
# ci.yml's own "the ONLY thing proving the published shim spawns the binary".
#
# The distinguishing content is the `files=` line each step builds its argument from, which this
# guard does not read -- so counting occurrences is what is available, and it is enough: two steps
# on one side and one on the other is now a difference. `comm` on sorted input with duplicates
# reports the surplus copy, which is the shape this needs.
#
# This is the third time this guard's needle has been too narrow (see the `npx` note below and the
# bare-word note above it). The pattern across all three: a normalization chosen to make matching
# ROBUST also makes two different things look the same, and the guard reports the collision as
# agreement.

# A command must BEGIN its line (optionally after `run: `). The first cut of this guard matched the
# bare word anywhere, so six COMMENT sentences ("...runs cargo and never touches the network") read as
# invocations. Comment lines are dropped outright first, for the same reason.
#
# `site-graph-data.mjs` is excluded: it is an internal of the site-regeneration step, and the two files
# legitimately spell it with different temp paths (`/tmp/n.ndjson` vs a `mktemp -d`), so comparing the
# line would compare a path rather than a step.
#
# `npx` joined the needle on 2026-08-08. It was absent, so ci.yml's playwright-install line was not
# excluded — it was INVISIBLE, and the reason it need not be mirrored was written in the OTHER file
# (`ci-local.sh`'s comment), which is the guard's scope being justified outside the guard. The live
# cost of that was not this one line but the shape: a future `npx <new check>` in CI would have had no
# local counterpart and this script would still have reported "mirrored both ways". Now the whole `npx`
# family is seen and exactly ONE line is exempted, by its full text, with the liveness assert below.
PLAYWRIGHT_INSTALL="npx --prefix scripts/site-render-check playwright install --with-deps chromium"

# Stale-exemption assert: an exemption that outlives its subject silently absolves whatever takes the
# same shape next. Sibling guards (overclaim, rule-desc, tree-sitter, committed-config, english) all
# make a dead exemption RED; this one used to be the odd fleet member with a bare pass-through.
if ! grep -qF "$PLAYWRIGHT_INSTALL" "$CI"; then
  echo "check-ci-local-mirrors-ci: the exempted line is gone from $CI:" >&2
  echo "  $PLAYWRIGHT_INSTALL" >&2
  echo "  A stale exemption absolves the next command that happens to match it. Delete it here." >&2
  exit 1
fi

extract() {
  grep -vE '^[[:space:]]*#' "$1" \
    | grep -hoE '^[[:space:]]*-?[[:space:]]*(run: )?(\$CARGO|cargo|node scripts/[^ ]+|node --test|npm ci|npx |bash scripts/measure/[^ ]+|bash docs/[^ ]+)([^|>&#"]*)?' \
    | sed -E 's/\$CARGO/cargo/' \
    | normalize \
    | grep -vE '^bash scripts/check-|site-graph-data' \
    | grep -vFx "$PLAYWRIGHT_INSTALL" \
    | sort || true
}

ci_cmds="$(extract "$CI")"
local_cmds="$(extract "$LOCAL")"

# Non-emptiness floor on BOTH sides: an extraction that stopped matching would make this guard
# vacuously green, which is the exact failure it exists to prevent.
ci_n="$(printf '%s\n' "$ci_cmds" | grep -c . || true)"
local_n="$(printf '%s\n' "$local_cmds" | grep -c . || true)"
if [ "$ci_n" -lt 4 ] || [ "$local_n" -lt 4 ]; then
  echo "check-ci-local-mirrors-ci: extracted $ci_n command(s) from $CI and $local_n from $LOCAL." >&2
  echo "  One of the extractions broke; an empty side would make this comparison meaningless." >&2
  exit 1
fi

missing_local="$(comm -23 <(printf '%s\n' "$ci_cmds") <(printf '%s\n' "$local_cmds") || true)"
missing_ci="$(comm -13 <(printf '%s\n' "$ci_cmds") <(printf '%s\n' "$local_cmds") || true)"

rc=0
if [ -n "$missing_local" ]; then
  echo "check-ci-local-mirrors-ci: ci.yml runs command(s) $LOCAL does not:" >&2
  printf '%s\n' "$missing_local" | sed 's/^/  /' >&2
  echo "  A CI job with no local counterpart is a job you can only fail remotely." >&2
  rc=1
fi
if [ -n "$missing_ci" ]; then
  echo "check-ci-local-mirrors-ci: $LOCAL runs command(s) ci.yml does not:" >&2
  printf '%s\n' "$missing_ci" | sed 's/^/  /' >&2
  echo "  Local failing on something CI does not trains people to ignore this script." >&2
  rc=1
fi
[ "$rc" -eq 0 ] || exit 1

echo "check-ci-local-mirrors-ci: clean ($ci_n non-guard command(s) mirrored both ways)."
