#!/usr/bin/env bash
# require-gnu-toolchain.sh — the guards assume GNU userland. Say so BEFORE they fail confusingly.
#
# ## The defect this exists for (2026-09-24, review ledger V340, external review round 29)
#
# MEASURED on this machine: with a bare `PATH=/usr/bin:/bin`, MOST of `scripts/check-*.sh` fail; with
# Homebrew's GNU tools in front, all of them pass. And NOTHING in the repository declared that
# dependency — `grep -lE 'gnubin|GNU (grep|sed|awk)' scripts/check-*.sh .githooks/pre-commit
# scripts/ci-local.sh` returned zero.
#
# That gap has already cost a wrong verdict, and the brief preamble records it: `check-shipped-off-sync`
# reported that a file did not mention `dead-candidates` when the file mentions it ten times, because
# BSD awk cannot read the multi-line string in its program. A guard's red light was a fact about the
# shell, not about the repository — and the only thing standing between a reader and that conclusion
# was remembering a paragraph in a document.
#
# 🔴 THE COUNTS THAT USED TO BE ON THOSE TWO LINES (`39 of the 54`) WENT STALE THE SAME DAY THEY WERE
# WRITTEN: a guard was deleted hours later and the fleet became 53. A count of the fleet does not
# belong in a comment that nothing recounts -- and this one sat in the header of a file whose whole
# subject is the fleet. Recount both halves in one run:
#
#   ls scripts/check-*.sh | wc -l
#   for g in scripts/check-*.sh; do PATH=/usr/bin:/bin bash "$g" > /dev/null 2>&1 || echo "$g"; done | wc -l
#
# (The second line needs this precondition BYPASSED to mean anything, which is why it is written out
# rather than wired: a guard that refuses to run cannot measure how many guards would fail.)
#
# ## Why one precondition rather than a check inside each guard
#
# Twenty-two to thirty-nine guards would each carry the same four lines, and the first one to fail
# would still be the one that gets read. The useful place is BEFORE the fleet runs, where one message
# can explain every failure that would otherwise follow. It is not a `scripts/check-*.sh` for the same
# reason: `check-guards-wired.sh` requires those to run inside the guard job, and this has to precede
# it. It lives in `scripts/lib/`, which that meta-guard deliberately excludes.
#
# ## WHAT IT CANNOT SEE: a name shadowed in the CALLER's shell (2026-09-25, review ledger V413)
#
# These probes run inside bash, so they resolve every name through PATH -- which is exactly right for
# the fleet, because `bash scripts/check-*.sh` resolves the same way and shell functions are not
# exported. It is also why this file is blind to one real case: if the shell that INVOKES the fleet has
# `grep` or `find` defined as a function or an alias, that name never reaches PATH there, and no
# prelude can change it. MEASURED on 2026-09-25 in one such shell: `grep` was ugrep 7.8.4 and `find`
# was bfs 4.1.1 with the gnubin prelude exported, while `bash -c 'grep --version'` in the same shell
# printed GNU grep 3.12. Nothing here can detect that from in here; the remedy is `command grep` and a
# `type grep` before trusting an inline measurement, and its owner is the build skill's §0.

# ## It tests CAPABILITY, not provenance
#
# On CI (ubuntu) grep/sed/awk already ARE GNU, so asking "is this Homebrew" would be a false red there.
# Each probe below runs the exact construct a guard depends on and checks the answer. A probe that
# passes on both platforms is not carrying its weight and should be deleted rather than kept for
# symmetry.
# ## HOW THIS PROBE LIST IS DERIVED (2026-09-25, review ledger V380)
#
# Round 30 said three coreutils-only commands were outside these probes -- `timeout`, `sha256sum`,
# `nproc` -- and could not make any of them fail. MEASURED here, and the reason is sharper than the
# claim: NO GUARD INVOKES ANY OF THE THREE. Every hit is inside a comment or an awk pattern:
#
#   grep -nE '(^|[|;&(]|\$\()\s*(timeout|sha256sum|nproc|realpath|numfmt|shuf|tac)\b' \
#     scripts/check-*.sh .githooks/*
#
# -> two hits on 2026-09-25, both inside comments. So the probe list is COMPLETE by that derivation
# today, and the finding's real content is not the three names: it is that the list was chosen from
# memory. Re-derive with the line above when a guard is added, and probe what it actually calls.
#
# No guard was built for this. The population the derivation returns is currently EMPTY, and a guard
# over an empty population cannot prove itself red -- this repo's own anti-vacuity rule. When the
# first real invocation lands, that is the moment the check earns its existence.

set -uo pipefail

_missing=()

# PCRE. Ten guards use `\K` or a lookahead; BSD grep has no -P at all.
# Herestring, not `printf | grep -q`: under pipefail a -q match exits early and SIGPIPEs the
# producer, which flips the pipeline's verdict on a large enough input. check-shell-pipe-sigpipe
# caught that here, in a file whose whole subject is tools behaving differently than they look.
grep -qP 'x' <<< 'x' 2> /dev/null || _missing+=("grep -P (PCRE) — BSD grep has none; 10 guards use \\K or lookahead")

# GNU sed's -i takes no mandatory suffix argument; BSD's does, and silently eats the next token.
_t="$(mktemp)"; printf 'a\n' > "$_t"
sed -i 's/a/b/' "$_t" 2> /dev/null && [ "$(cat "$_t")" = "b" ] || _missing+=("sed -i without a suffix argument — BSD sed requires one")
rm -f "$_t"

# `awk -v var=<value containing a newline>`. BSD awk refuses it with `newline in string`, and that is
# how `check-shipped-off-sync` came to report that a file does not mention `dead-candidates` when it
# mentions it ten times: that guard passes its whole id list through `-v ids="$native_ids"`.
#
# 🔴 This probe was WRONG TWICE before it was right, and both times a canary caught it rather than a
# reading. First it asserted the length of a multi-line string literal was 3 when the continuation
# makes it 2 — red under GNU awk. Corrected, it then passed under BOTH awks, because BSD awk accepts a
# multi-line literal perfectly well; the construct it rejects is this one. A probe that cannot tell
# the two platforms apart is worse than no probe: it reports a capability it never tested.
awk -v _p="$(printf 'a\nb')" 'BEGIN { if (split(_p, _r, "\n") == 2) exit 0; exit 1 }' 2> /dev/null \
  || _missing+=("awk -v with a newline in the value — BSD awk refuses it (this one produced a WRONG ANSWER, not an error)")

# Associative arrays. Five guards use them; macOS ships bash 3.2, which has no `declare -A`.
# Probed in a SUBSHELL of the current bash, because that is the interpreter the guards will run under.
( declare -A _probe 2> /dev/null ) || _missing+=("bash with declare -A (bash >= 4) — macOS ships 3.2; 5 guards use it")

# `xargs -d`. BSD xargs has no such flag.
xargs -d '\n' true <<< 'a' 2> /dev/null || _missing+=("xargs -d — BSD xargs has none")

if [ "${#_missing[@]}" -gt 0 ]; then
  {
    echo "require-gnu-toolchain: this shell cannot run the guards, and their failures would NOT be"
    echo "  facts about the repository. Missing ${#_missing[@]} capability/-ies:"
    echo
    for _m in "${_missing[@]}"; do echo "    - $_m"; done
    echo
    echo "  Install once:   brew install bash coreutils findutils gnu-sed grep gawk"
    echo "  Then put them in front, in this shell and in any hook that runs the guards:"
    echo '    G=/opt/homebrew/opt'
    echo '    export PATH="$G/bash/bin:$G/coreutils/libexec/gnubin:$G/findutils/libexec/gnubin:$G/gnu-sed/libexec/gnubin:$G/grep/libexec/gnubin:$G/gawk/libexec/gnubin:$PATH"'
    echo
    echo "  \`bash\` is first and is spelled differently on purpose: it is the ONE formula in that list"
    echo "  with no \`libexec/gnubin\` directory — brew puts it at \`\$G/bash/bin\`. A PATH line built by"
    echo "  pattern from the other five omits it, and then this same message prints again with only the"
    echo "  \`declare -A\` line left, which reads as a failed install rather than an incomplete remedy."
    echo
    echo "  Those two lines are the whole fix. They are spelled out here rather than linked because"
    echo "  this file ships: a contributor hitting this message has the repository and nothing else,"
    echo "  and a pointer into the maintainer's private notes would be a dead end for them."

  } >&2
  exit 1
fi
