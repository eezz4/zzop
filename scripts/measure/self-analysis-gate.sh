#!/usr/bin/env bash
# self-analysis-gate.sh — run `zzop analyze` on THIS repo, pin the result at zero findings, and prove
# first that the run can still see anything at all.
#
# The verdict logic, the canary and every assertion live in `self-analysis-gate.mjs` next to this file;
# read that header for WHY zero rather than a baseline, and for the three ways a canary can silently
# fail to plant (all three were measured by hand before this was written). This script exists to do the
# two things a shell must: build the binary it runs, and guarantee the probe target is restored even if
# the node process dies where its own `finally` cannot reach.
#
# ## Why this rides `cli-shim-test` rather than being a sixth CI job
# That job already runs `cargo build -p zzop-cli-bin --release` for the npm shim, and already points the
# resulting binary at this repo's committed `zzop.config.jsonc` for the dep-graph lane. The binary is
# therefore free there, and this gate is the FINDINGS half of the same dogfood run — the two halves
# reading the same config from the same job is the honest arrangement. A sixth job would pay a second
# release build (minutes) to answer a question that costs seconds once the binary exists, and would let
# the two halves of one claim drift onto different refs' worth of caching. If this gate ever grows to
# where it dominates that job's wall clock, split it then and say so here.
#
# ## Why it is NOT a scripts/check-*.sh
# `check-guards-wired.sh` requires every `scripts/check-*.sh` to be wired into BOTH `.githooks/pre-commit`
# and `ci.yml`'s guards job. This needs a release build, which is not a per-commit hook's budget — the
# same reason `detection-gate.sh` beside it is not named `check-*` either.
#
# usage: bash scripts/measure/self-analysis-gate.sh    (takes no arguments — see below)
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

# A binary path passed here is REFUSED rather than ignored, for the reason recorded at length in
# `detection-gate.sh`: when that gate accepted one, a leftover 0.24.0 binary scored a green 143/143
# against a 0.25.0 tree. A gate that measures whatever is lying in target/ measures nothing.
if [ "$#" -ne 0 ]; then
  echo "self-analysis-gate: this script takes NO arguments (got: $*)." >&2
  echo '  It builds what it runs (`cargo build -p zzop-cli-bin --release`).' >&2
  exit 1
fi

CONFIG=zzop.config.jsonc
if [ ! -f "$CONFIG" ]; then
  echo "self-analysis-gate: $CONFIG is missing — that committed config is the whole subject of this run." >&2
  exit 1
fi

# PREFLIGHT. The canary's planting machinery is `scripts/measure/plant-revert.mjs`, and every refusal in
# it fails in the same direction when it rots: quietly, into a green result. So its selftest runs first,
# before the release build — it needs no binary, it costs one Node spawn plus a child, and a gate whose
# canary can no longer refuse a no-op mutation is a gate that would report this repo clean without
# having looked. Same relationship `detection-gate.sh` has with `harness-selftest.sh`.
#
# It is invoked HERE rather than as its own step in ci.yml/ci-local.sh deliberately: those two files are
# bound command-for-command by `scripts/check-ci-local-mirrors-ci.sh`, whose extractor matches
# `bash scripts/measure/<x>` and `node scripts/<x>` at the START of a line in EITHER file. A new
# top-level command therefore has to be added to both or the guard goes red — and a preflight belongs to
# the gate it preflights, not to the job list.
node scripts/measure/plant-revert-selftest.mjs

CARGO="${CARGO:-cargo}"

# The artifact path is READ BACK FROM CARGO rather than spelled `target/release/zzop`. That spelling is
# wrong under $CARGO_TARGET_DIR, wrong under a `.cargo/config.toml` `build.target-dir`, and wrong on
# Windows (`.exe`). In `cli-shim-test` this build is already done and cargo returns immediately; the
# call is here for the PATH as much as for the bytes.
echo "self-analysis-gate: building zzop-cli-bin --release (this gate builds what it runs)"
bin="$($CARGO build -p zzop-cli-bin --release --message-format=json-render-diagnostics | node -e '
  let exe = null;
  for (const l of require("fs").readFileSync(0, "utf8").split("\n")) {
    if (!l.trim()) continue;
    let m;
    try { m = JSON.parse(l); } catch { continue; }
    // A lib or build-script unit reports executable:null; only the bin target names a file to run.
    if (m.reason === "compiler-artifact" && m.executable && (m.target && m.target.kind || []).includes("bin")) exe = m.executable;
  }
  if (!exe) {
    console.error("self-analysis-gate: cargo emitted NO executable artifact for zzop-cli-bin.");
    process.exit(1);
  }
  process.stdout.write(exe);
')"
if [ ! -x "$bin" ]; then
  echo "self-analysis-gate: cargo named '$bin' but it is not executable." >&2
  exit 1
fi

# LAST-RESORT RECOVERY. The canary restores its probe targets from the bytes it read itself, in a
# `finally` and on SIGINT/SIGTERM/SIGHUP — but not if the node process is killed outright. So it also
# leaves each target's pre-plant bytes as `$backup.<n>` plus that target's path in `$backup.<n>.path`,
# and deletes the pair once it has verified that file's revert. Anything left behind means the probe
# died mid-flight. `plant-revert-recover.sh` is the reader of that sidecar pair — one owner per side of
# the contract, and it is a script rather than a function here because the module's selftest recovers
# from a deliberately aborted child through the SAME loop.
work="$(mktemp -d)"
backup="$work/probe-target.orig"
recover() {
  bash scripts/measure/plant-revert-recover.sh "$backup"
  rm -rf "$work"
}
trap recover EXIT

node scripts/measure/self-analysis-gate.mjs --bin "$bin" --config "$CONFIG" --backup "$backup"
