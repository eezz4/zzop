#!/usr/bin/env bash
# check-subcommand-reachable.sh — every subcommand the binary offers must be reachable from a page a
# user actually reads.
#
# ## The defect this exists for (2026-09-25, external review round 31)
#
# MEASURED: `zzop map` appeared on ZERO user-reachable pages. Its only two mentions in the whole
# repository were `docs/contracts/surface-parity.json` and `docs/modules/mcp.md` — both internal
# contract documents, neither a page anyone reads to learn the tool — and it was in neither CHANGELOG
# nor VERSIONING. The counts that day, over the subject set below:
#
#   analyze 10 · cross 11 · graph 8 · init 8 · explain 6 · contract 6 · facts 5 ·
#   file 4 · coverage 4 · version 4 · diff 2 · MAP 0
#
# Three separate guards already compare FLAGS across `--help`, `config-surface.json` and the source,
# and all three were green: a flag is checked against the command it hangs off, and nobody was
# checking that the command itself is documented. A capability that ships and is described nowhere is
# the same silence as a capability that does not ship, minus the honesty.
#
# ## Why the subject set is what it is
#
# SUBJECTS are the pages a stranger reaches without a checkout: README.md, the published site's
# source (site-src/, which generates site/), and the two docs/ pages written for readers rather than
# for the contract (ARCHITECTURE.md, getting-started.md).
#
# NOT subjects, deliberately: `docs/contracts/` and `docs/modules/` are contract surfaces for adapter
# authors and for this repository's own gates — `zzop map`'s two mentions were both there, which is
# exactly the shape that made the gap invisible. Counting them would have scored `map` as 2 and this
# guard would have shipped green over the defect it was written for.
#
# ## WHAT THIS GUARD CANNOT SEE, stated rather than left to be discovered
#
# It counts a MENTION, not a description. A page that spells `zzop map` inside an unrelated sentence
# satisfies it. The check is a floor against silence, never a claim that the documentation is good —
# and it says nothing at all about flags, output shape, or whether the sentence is still true.
set -uo pipefail
cd "$(dirname "$0")/.."

SUBJECTS=(README.md 'site-src/*' docs/ARCHITECTURE.md docs/getting-started.md)

# The subcommand list comes from the binary's own usage block, never from a list kept here — a list
# kept here would go stale the first time a subcommand is added, and going stale IS this guard's
# subject. Falls back to the source's dispatch when no release binary is around (CI builds one; a
# fresh clone may not have).
ZZOP=./target/release/zzop
if [ -x "$ZZOP" ]; then
  mapfile -t COMMANDS < <("$ZZOP" --help 2>&1 |
    grep -oE '^  [a-z][a-z-]+' | tr -d ' ' | sort -u)
  SOURCE="the binary's own --help"
else
  mapfile -t COMMANDS < <(grep -oE '^\s+"[a-z][a-z-]+" =>' packages/cli-bin/src/main.rs |
    grep -oE '"[a-z][a-z-]+"' | tr -d '"' | sort -u)
  SOURCE="packages/cli-bin/src/main.rs (no release binary present)"
fi

if [ "${#COMMANDS[@]}" -lt 5 ]; then
  echo "check-subcommand-reachable: read only ${#COMMANDS[@]} subcommand(s) from $SOURCE." >&2
  echo "  That is not a repository with fewer commands, it is a broken read — refusing to score." >&2
  exit 1
fi

missing=()
for cmd in "${COMMANDS[@]}"; do
  # `git grep -l` over the subject pathspecs; `|| true` because no-match is exit 1 here and is the
  # answer, not a failure.
  hits="$(git grep -l -- "zzop $cmd" "${SUBJECTS[@]}" 2> /dev/null | wc -l | tr -d ' ')"
  [ "$hits" = "0" ] && missing+=("$cmd")
done

if [ "${#missing[@]}" -gt 0 ]; then
  {
    echo "check-subcommand-reachable: ${#missing[@]} subcommand(s) the binary offers appear on NO page a"
    echo "  user reads:"
    for m in "${missing[@]}"; do echo "    zzop $m"; done
    echo
    echo "  Subject pages: ${SUBJECTS[*]}"
    echo "  docs/contracts/ and docs/modules/ are deliberately NOT subjects — see this file's header."
    echo "  One line in README.md's command block is enough. A capability nobody can find is the same"
    echo "  silence as one that does not ship."
  } >&2
  exit 1
fi

echo "check-subcommand-reachable: clean (${#COMMANDS[@]} subcommands from $SOURCE, each named on at least one user-reachable page)."
