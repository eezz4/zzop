#!/usr/bin/env bash
# preflight-build-env — refuse to start a long build in an environment that will kill it.
#
# WHY THIS EXISTS
# `target/debug` grew to 83 GB / 13,737 files on 2026-08-04 and link.exe began failing with an EMPTY
# diagnostic: cargo reported `linking with link.exe failed: exit code 1` and the linker's own note was
# blank, followed by "the Visual Studio build tools may need to be repaired". Every one of those
# sentences points at a broken toolchain. The toolchain was fine. The bloat was making each link slow
# enough that the build outlived its window and the linker died mid-write.
#
# The cost of that misdirection is the point: the remedy was already recorded in this repo's own
# working agreements (delete target/debug at arc boundaries, keep release), and it still took a
# measurement to get from the symptom to it — because nothing connected "link.exe died with no message"
# to "your build directory is enormous". A diagnosis nobody can reach from the symptom is not a
# defence. This script is the connection.
#
# WHAT IT DOES NOT DO
# It does not delete anything on its own. `target/` is the user's build cache and a rebuild is minutes
# of their time, so the decision stays theirs — this only refuses to pretend the environment is ready,
# and prints the exact command. Pass --clean to opt in.
#
# USAGE
#   bash scripts/preflight-build-env.sh           # check, exit 1 with the remedy if over budget
#   bash scripts/preflight-build-env.sh --clean   # check, and remove target/debug if over budget
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

# The ceiling is a BUDGET, not a measurement of when linking breaks — that threshold is machine- and
# workspace-dependent and nobody has measured it. 40 GB sits comfortably above a healthy full workspace
# build (measured 24 GB with every test binary linked on 2026-08-04) and well below the 83 GB that was
# observed killing links. A build that has crossed it is carrying generations of dead rlibs, which is
# the condition this guards, whether or not that particular machine is close to failing yet.
BUDGET_GB=40

DIR=target/debug
if [ ! -d "$DIR" ]; then
  echo "preflight-build-env: no $DIR — nothing to check."
  exit 0
fi

# `du -s` in KiB, portable across the Git-Bash/coreutils combinations this repo is built on.
#
# `|| true` is load-bearing. The trailing `awk` exits 0 even on empty input, so the usual "the floor is
# reachable because the LAST producer cannot fail" reading says this line is safe — and it is wrong
# here. Under `set -e -o pipefail` the pipeline takes the status of the last FAILING stage, and the
# failing stage is `du`, at the FRONT. `du` exits non-zero when it cannot read the tree, which is
# exactly the case the diagnosis below is written for: without `|| true` the script died on THIS
# assignment with ZERO output and exit 1, and "could not size" never printed (measured 2026-08-14 by
# pointing $DIR at a path that does not exist). The realistic version is worse than the drill — a
# partially unreadable target/ makes `du` print a usable total AND exit non-zero, so the run died
# holding the answer, with its stderr already sent to /dev/null one stage earlier.
KB=$(du -sk "$DIR" 2>/dev/null | awk '{print $1}' || true)
if [ -z "${KB:-}" ]; then
  echo "preflight-build-env: could not size $DIR — treating as unknown, not as clean." >&2
  exit 1
fi
GB=$((KB / 1024 / 1024))

if [ "$GB" -lt "$BUDGET_GB" ]; then
  echo "preflight-build-env: $DIR is ${GB}GB (budget ${BUDGET_GB}GB) — clear."
  exit 0
fi

cat >&2 <<EOF
preflight-build-env: $DIR is ${GB}GB, over the ${BUDGET_GB}GB budget.

cargo never deletes old-fingerprint rlibs, so this directory accumulates generations. Past that size
linking gets slow enough that a long build can be killed mid-link, and what you will see then is NOT a
disk error — it is:

    error: linking with \`link.exe\` failed: exit code: 1
      = note:                      <- empty
    note: the Visual Studio build tools may need to be repaired

Those sentences are misdirection. The toolchain is fine. Reclaim and rebuild:

    rm -rf target/debug          # keep target/release: the detection gates reuse it

Or re-run this with --clean to do exactly that.
EOF

if [ "${1:-}" = "--clean" ]; then
  echo "preflight-build-env: --clean given, removing $DIR ..." >&2
  rm -rf "$DIR"
  echo "preflight-build-env: removed. The next build is cold." >&2
  exit 0
fi
exit 1
