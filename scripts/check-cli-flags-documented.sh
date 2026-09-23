#!/usr/bin/env bash
# check-cli-flags-documented.sh — every long CLI flag this binary accepts must be findable in at least
# one shipped document.
#
# ## The defect this exists for (2026-09-06, external review lane C)
#
# The review measured flag coverage across the onboarding docs — README 12 of 16, getting-started 8 of
# 16 — and reported `--source-id` as documented in zero of them. Re-measured before this guard was
# written: it is documented in ONE (`packages/README.md`, beside the `zzop file` example it
# disambiguates). So the review's number was wrong and its subject was not: nothing in the fleet had
# the flag surface as a subject at all, in either direction of coverage.
#
# The class is not hypothetical. `site/usage.html` once went days without naming four `--domain`
# values, and a flag no document names comes back as a defect report — an outside reader concludes the
# capability is missing, which is how a review spends a question on a feature that shipped.
#
# ## Why this is a FLOOR and not a completeness rule
#
# The tempting guard is "every flag appears in README". That would be a bloat rule nobody measured a
# need for: README is an install page, not a manual, and this repo removes prose for a living. What can
# be defended is the floor — a flag documented NOWHERE is undiscoverable, and that state is always a
# defect regardless of anyone's taste about which page should carry it.
#
# So this guard locks in what exists rather than demanding more, and it will not fire today. That is
# the point: it is the ratchet under a surface that had none, and the number it protects (zero flags
# undocumented) is the only one on this axis that needs no judgement call.
#
# ## SSOT and the opposite direction
#
# `crates/config/config-surface.json`'s `cliFlags` is the declared flag list, and CHECK A already
# validates every `--flag`-shaped token in shipped MESSAGES against it — that is the other direction
# (a message inventing a flag). This guard is the one that was missing: a flag that exists and that no
# page mentions.
#
# Short flags (`-h`, `-V`) are excluded: they are aliases of long flags that ARE checked, and a
# two-character token cannot be searched for in prose without matching everything.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

# Fail on the UTILITY, not on the data: without this the GNU-only `xargs -d` surfaces as a claim
# about this repo (a deleted file, an empty scan, a silently wrong count). See the lib header.
# shellcheck source=scripts/lib/require-gnu.sh
. "$(dirname "${BASH_SOURCE[0]}")/lib/require-gnu.sh"
require_gnu_xargs "check-cli-flags-documented"

SURFACE=crates/config/config-surface.json
[ -f "$SURFACE" ] || { echo "check-cli-flags-documented: missing $SURFACE -- re-anchor this guard." >&2; exit 1; }

set +e
flags="$(node -e '
  const fs = require("fs");
  const j = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
  const list = Array.isArray(j.cliFlags) ? j.cliFlags : [];
  for (const f of list) if (/^--[a-z][a-z-]+$/.test(f)) console.log(f);
' "$SURFACE")"
rc=$?
set -e
if [ "$rc" -ne 0 ] || [ -z "$flags" ]; then
  echo "check-cli-flags-documented: extracted ZERO long flags from $SURFACE (node exit $rc)." >&2
  echo "  An empty subject is a broken guard, never a CLI with no flags." >&2
  exit 1
fi
n="$(printf '%s\n' "$flags" | awk 'END { print NR }')"

# The doc set is the prose a USER reaches: the front README, docs/, and the site SOURCE.
# `site/` is excluded because it is generated from `site-src/` — counting both would let a flag look
# documented twice while living in one sentence, and a guard that double-counts its own evidence is
# the shape this repo has already had to repair once.
#
# 🔴 It used to be `git ls-files -- '*.md'` — EVERY markdown in the repo, which is 77 files including
# every nested package and rules README. That set made the guard answer a question nobody asked:
# "is this flag's name written down anywhere at all", which a single mention in a package-internal
# README satisfies. `--source-id` sat in exactly that state for two review rounds (ledger V123) —
# named once, in packages/README.md, and in zero of the pages a user is pointed at.
#
# 📏 Counted BEFORE narrowing, because narrowing a guard's subject set is how you accidentally turn
# one reported gap into a backlog (2026-09-08): under the old 77-file set 17/17 flags were
# documented; under this 47-file set it is 16/17, and the single flag that moves is `--source-id`
# itself. Narrowing costs exactly the finding it was narrowed for and nothing else, which is why it
# could be done in the same commit that fixes it. (README.md alone would drop five flags — that is
# the bloat rule the header below refuses.)
set +e
docs="$(git ls-files -- 'README.md' 'docs/**' 'site-src/**')"
rc=$?
set -e
if [ "$rc" -ne 0 ] || [ -z "$docs" ]; then
  echo "check-cli-flags-documented: enumerated ZERO docs (git ls-files exit $rc)." >&2
  echo "  This repo ships documentation; a zero here is a broken pathspec, never a clean tree." >&2
  exit 1
fi
n_docs="$(printf '%s\n' "$docs" | awk 'END { print NR }')"

undocumented=""
for f in $flags; do
  set +e
  hit="$(printf '%s\n' "$docs" | xargs -d '\n' grep -l -F -- "$f" 2> /dev/null | awk 'END { print NR }')"
  set -e
  [ "${hit:-0}" -eq 0 ] && undocumented="$undocumented $f"
done

if [ -n "$undocumented" ]; then
  echo "check-cli-flags-documented: flag(s) this binary accepts that NO shipped document names:" >&2
  echo " $undocumented" >&2
  echo "  A flag no page mentions is one no user can find, and it comes back as a bug report saying" >&2
  echo "  the capability is missing. Name it wherever a reader would already be looking -- beside the" >&2
  echo "  subcommand it modifies -- not in a list added for this guard's benefit." >&2
  exit 1
fi


# ── CHECK B: the binary's own `--help` must name every long flag the surface declares.
#
# ## The defect this exists for (2026-09-07, review ledger V69)
#
# CHECK A above reads the surface JSON and asks whether a DOCUMENT names each flag. That leaves one
# direction unwatched, and it was open: `--version` sat in `cliFlags`, the binary accepted it and
# printed `zzop 0.34.0`, and `zzop --help` never named it. A user reading the help text concluded the
# flag did not exist while the binary answered it -- the same undiscoverable state CHECK A exists to
# prevent, one surface over. The guard could not see it because both of its inputs (the JSON and the
# docs) agreed; the text that disagreed was never read.
#
# Help text is not "a document" for CHECK A's purpose and must not be folded into it: a doc set answers
# "can a reader find this", while help answers "does the tool admit to it", and a flag can pass one and
# fail the other. Two questions, two checks, one subject list.
#
# Short flags stay excluded here for CHECK A's reason: `-V`/`-h` are aliases of long flags that ARE
# checked. The help text is free to spell them beside their long form -- it does -- but their absence
# is not a finding.
# The binary is resolved by TRYING BOTH NAMES, never by assuming one. Until 2026-09-15 this line read
# `target/release/zzop.exe` alone, so on every non-Windows machine and in CI -- where the `guards` job
# has no build step at all -- CHECK B took the skip branch while a perfectly good `target/release/zzop`
# sat beside it. External review round 18 measured the consequence: the check had not run ONCE since it
# landed on 2026-09-07, and the OK line below said `named by \`zzop --help\`` on every one of those runs.
#
# The same `.exe` hardcode was found and fixed in `scripts/measure/wire-key-census.mjs` on 2026-09-14
# (recorded at `one-point-oh.md`'s surface census) and the sweep reached two instruments of three;
# `scripts/measure/readme-result-block.sh:118` was already correct. This was the one left.
set +e
help_text=""
rc=127
for candidate in target/release/zzop target/release/zzop.exe; do
  [ -x "$candidate" ] || continue
  help_text="$("$candidate" --help 2>&1)"
  rc=$?
  break
done
set -e
check_b_ran=0
if [ "$rc" -ne 0 ] || [ -z "$help_text" ]; then
  echo "check-cli-flags-documented: CHECK B SKIPPED -- no runnable target/release/zzop[.exe] (exit $rc)." >&2
  echo "  Not a failure: a fresh clone has no built binary and this guard must not demand a release" >&2
  echo "  build. It is announced rather than silent, because a check that quietly does nothing is the" >&2
  echo "  shape this repo has had to repair before." >&2
else
  check_b_ran=1
  unnamed=""
  for f in $flags; do
    case "$help_text" in
      *"$f"*) ;;
      *) unnamed="$unnamed $f" ;;
    esac
  done
  if [ -n "$unnamed" ]; then
    echo "check-cli-flags-documented: flag(s) the surface declares that \`zzop --help\` never names:" >&2
    echo " $unnamed" >&2
    echo "  The binary accepts these and its own help text does not admit to them, so a reader who" >&2
    echo "  checks --help concludes they do not exist. Name the flag beside the subcommand it belongs" >&2
    echo "  to, or drop it from cliFlags -- but do not leave the two surfaces disagreeing." >&2
    exit 1
  fi
fi


# ── CHECK C: every long flag the surface declares must be READ by the argv parser.
#
# ## The defect this exists for (2026-09-07, review ledger V86)
#
# CHECK A asks whether a document names the flag. CHECK B asks whether `--help` names it. Neither
# reads the CODE, so the flag could stop being parsed at all and both stay green. Measured: replacing
# `flag == "--fold"` in `packages/cli-bin/src/cli/run.rs` with a dead spelling left all 79 cli-bin
# tests passing AND this guard reporting OK -- `grep -c -- '--fold' packages/cli-bin/tests/cli.rs` is
# 0, so nothing anywhere executed that flag. A flag documented on three surfaces and accepted by none
# is worse than an undocumented one: the reader has every reason to believe it works.
#
# Three questions, three checks, one subject list. They are deliberately not folded together -- a flag
# can pass any one and fail another, and a merged check would report the wrong remedy.
#
# The needle is the quoted flag literal anywhere under the CLI crate's src/. That is coarse on purpose:
# this is a FLOOR ("some code mentions this flag"), not a claim that the mention is a working parse.
# Proving the parse is what an e2e test does, and this guard's job is to notice when there is nothing
# to prove. The search covers all of src/ rather than cli/ alone because `--verbose` is parsed in
# main.rs -- a narrower needle would have reported a false violation on its first run.
set +e
parser_src="$(find packages/cli-bin/src -name '*.rs' 2> /dev/null)"
rc=$?
set -e
if [ "$rc" -ne 0 ] || [ -z "$parser_src" ]; then
  echo "check-cli-flags-documented: enumerated ZERO parser sources under packages/cli-bin/src." >&2
  echo "  This repo ships a CLI; a zero here is a broken path, never a CLI with no source." >&2
  exit 1
fi
unparsed=""
for f in $flags; do
  set +e
  hit="$(printf '%s\n' "$parser_src" | xargs -d '\n' grep -l -F -- "\"$f\"" 2> /dev/null | awk 'END { print NR }')"
  set -e
  [ "${hit:-0}" -eq 0 ] && unparsed="$unparsed $f"
done
if [ -n "$unparsed" ]; then
  echo "check-cli-flags-documented: flag(s) the surface declares that NO CLI source mentions:" >&2
  echo " $unparsed" >&2
  echo "  The flag is documented and advertised in --help, and nothing in the binary reads it, so a" >&2
  echo "  user who follows the docs gets a usage error. Either wire it up or drop it from cliFlags --" >&2
  echo "  a promise on three surfaces and an implementation on none is the worst of the four states." >&2
  exit 1
fi

# The success line states only what RAN. It used to claim `named by \`zzop --help\`` unconditionally,
# including on the skip branch -- a guard asserting the one check it had just announced it skipped, on
# stderr, where the fleet's stdout log never shows it.
if [ "$check_b_ran" -eq 1 ]; then
  echo "check-cli-flags-documented: OK ($n long flags — each named in at least one of $n_docs tracked docs, named by \`zzop --help\`, and mentioned by the CLI source)."
else
  echo "check-cli-flags-documented: OK ($n long flags — each named in at least one of $n_docs tracked docs, and mentioned by the CLI source; CHECK B did not run, see stderr)."
fi
