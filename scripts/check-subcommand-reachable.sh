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
# subject. Falls back to the source's dispatch when no release binary is around (the `guards:` CI job
# does NOT build one — it checks out and runs guards — so the fallback is the path CI actually takes; a
# fresh clone may not have).
ZZOP=./target/release/zzop
if [ -x "$ZZOP" ]; then
  mapfile -t COMMANDS < <("$ZZOP" --help 2>&1 |
    grep -oE '^  [a-z][a-z-]+' | tr -d ' ' | sort -u)
  SOURCE="the binary's own --help"
else
  # The dispatch arms are `Some("analyze") => …`, not `"analyze" => …`. The first spelling of this
  # needle omitted `Some(` and therefore matched ZERO arms — and because the floor below refuses to
  # score a read it does not believe, this guard failed its FIRST CI run (2026-10-02) rather than
  # passing vacuously. It had been green locally the whole time because a release binary was present
  # and the binary branch was taken; the fallback had never once run. The comment above this block
  # said "CI builds one" — the `guards:` job checks out and runs guards, it does not build.
  # `help` is excluded on purpose: it IS a dispatch arm (`Some("help") | Some("--help") | Some("-h")`)
  # but the binary's own `--help` does not list itself, and that listing is this guard's definition of
  # "a subcommand a user can reach". Keeping it would make the two read paths disagree by one forever.
  # 📏 Found by the cross-check below on its first run (binary 17, source 18).
  #
  # NOT anchored on `=>`: an arm may be an OR-pattern. `version` is dispatched as
  # `Some("version") | Some("--version") | Some("-V") =>`, so a needle ending in `) *=>` reads 16 of
  # the binary's 17 and silently stops checking `version`. Match every `Some("…")` on a dispatch
  # line instead, then drop the flag spellings (they start with a dash and are not subcommands).
  mapfile -t COMMANDS < <(grep -E 'Some\("[a-z-]' packages/cli-bin/src/main.rs |
    grep -oE 'Some\("[a-z][a-z-]*"\)' | grep -oE '"[a-z][a-z-]*"' | tr -d '"' | grep -vxF help | sort -u)
  SOURCE="packages/cli-bin/src/main.rs (no release binary present)"
fi

# 🔴 CROSS-CHECK, not just a floor. The two read paths must agree: if a release binary is present we
# ALSO run the source needle and compare counts. A silent divergence is how this guard shipped
# checking 16 of 17 — green on both paths, each reading a different set. 📏 2026-10-02: binary 17,
# source 16, missing `version` (an OR-pattern arm the first needle's `) *=>` anchor could not see).
if [ -x "$ZZOP" ]; then
  SRC_COUNT="$(grep -E 'Some\("[a-z-]' packages/cli-bin/src/main.rs |
    grep -oE 'Some\("[a-z][a-z-]*"\)' | grep -oE '"[a-z][a-z-]*"' | tr -d '"' | grep -vxF help | sort -u | wc -l | tr -d ' ')"
  if [ "$SRC_COUNT" != "${#COMMANDS[@]}" ]; then
    echo "check-subcommand-reachable: the two read paths DISAGREE — binary ${#COMMANDS[@]}, source $SRC_COUNT." >&2
    echo "  CI takes the SOURCE path (the guards job does not build), so a divergence means CI checks" >&2
    echo "  a different set than you do locally. Fix the needle in this file, not the binary." >&2
    exit 1
  fi
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
