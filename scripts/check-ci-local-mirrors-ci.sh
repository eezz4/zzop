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
# The `cargo`/`node`/`npm ci`/`bash scripts/...` invocations in `ci.yml`'s `run:` steps. `node` covers
# BOTH shapes ci.yml actually runs — `node scripts/<file>` and `node --test $files` — because until
# 2026-08-03 the extractor knew only the first, and the three test-runner commands (two `node --test`
# steps, one `npm ci`) sat in ci.yml with no local counterpart while this guard reported "mirrored
# both ways". A needle list that is narrower than the claim is the same defect this guard exists to
# catch, one level down. Deliberately excluded:
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

# A command must BEGIN its line (optionally after `run: `). The first cut of this guard matched the
# bare word anywhere, so six COMMENT sentences ("...runs cargo and never touches the network") read as
# invocations. Comment lines are dropped outright first, for the same reason.
#
# `site-graph-data.mjs` is excluded: it is an internal of the site-regeneration step, and the two files
# legitimately spell it with different temp paths (`/tmp/n.ndjson` vs a `mktemp -d`), so comparing the
# line would compare a path rather than a step.
extract() {
  grep -vE '^[[:space:]]*#' "$1" \
    | grep -hoE '^[[:space:]]*-?[[:space:]]*(run: )?(\$CARGO|cargo|node scripts/[^ ]+|node --test|npm ci|bash scripts/measure/[^ ]+)([^|>&#"]*)?' \
    | sed -E 's/\$CARGO/cargo/' \
    | normalize \
    | grep -vE '^bash scripts/check-|site-graph-data' \
    | sort -u || true
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
