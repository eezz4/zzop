#!/usr/bin/env bash
# Fixer for `check-license-shipping.sh`: regenerate THIRD-PARTY-NOTICES.md and re-copy the license
# artifacts to every distribution root that needs them. Not a guard — this one WRITES.
#
# ## Why this exists (2026-08-01, measured recurrence)
# Touching a single byte of any workspace `Cargo.toml` breaks the notices' inventory digest, which
# blocks the commit. Fixing it by hand is three steps: regenerate, then copy LICENSE and
# THIRD-PARTY-NOTICES.md into each shipping root (today that is the npm shim, its five platform
# sub-packages, and the .mcpb bundle), then re-run the guard. That sequence was hand-executed twice
# in one session. A sequence executed by hand twice is work that belongs to a script rather than to a
# person: the second occurrence is the signal to change the environment, not to be more careful.
#
# ## Why it has no subject list of its own
# It has NO independent idea of which roots must carry what. It runs `check-license-shipping.sh`,
# reads the paths that guard names as offenders, and copies to exactly those. The guard stays the
# single owner of the policy; this script only makes it green. A second hand-kept list of shipping
# roots is precisely the drift this repo keeps paying for (§5.5), and it would go stale the day a
# sixth platform lane is added — the day it matters most.
#
# ## Usage
#   bash scripts/sync-license-artifacts.sh          # regenerate + sync + verify
#   bash scripts/sync-license-artifacts.sh --check   # verify only (delegates to the guard)
set -euo pipefail
cd "$(dirname "$0")/.."

GUARD=scripts/check-license-shipping.sh
ROOT_LICENSE=LICENSE
NOTICES=THIRD-PARTY-NOTICES.md

if [ "${1:-}" = "--check" ]; then
  exec bash "$GUARD"
fi

for f in "$GUARD" "$ROOT_LICENSE" scripts/gen-third-party-notices.mjs; do
  [ -f "$f" ] || { echo "sync-license-artifacts: $f is missing" >&2; exit 1; }
done

echo "sync-license-artifacts: regenerating $NOTICES ..."
node scripts/gen-third-party-notices.mjs >/dev/null

# The guard's offender sections, by the header each one prints. Every listed path is either a
# distribution ROOT (needs the file created) or a COPY that drifted (needs overwriting) — both are
# answered by writing the canonical bytes to the right place, so one table covers all four.
guard_out="$(bash "$GUARD" 2>&1 || true)"

# Collect indented paths under any of the offender headers. `awk` rather than a grep chain so the
# section boundaries are explicit: a blank line or a non-indented line ends a section.
#
# A listed path must be ONE whitespace-free token. The guard follows each offender list with an
# indented English paragraph explaining the failure, and the first cut of this parser read those
# sentences as paths and `cp`'d the canonical LICENSE onto two files literally named after them.
# It then re-ran the guard, saw green, and reported success — a fixer whose own damage is invisible
# to the check it is verified by. Hence: a path here has no spaces, and anything else ends the
# section, so new prose can never be mistaken for a subject.
mapfile -t offenders < <(
  awk '
    /distribution roots with no LICENSE file:/            { sec="license-root"; next }
    /LICENSE copies that differ from the root/            { sec="license-copy"; next }
    /distribution roots that ship a binary but carry no/  { sec="notices-root"; next }
    /copies that differ from the root THIRD-PARTY/        { sec="notices-copy"; next }
    sec != "" && /^[[:space:]]+[^[:space:]]+$/ { gsub(/^[[:space:]]+/, ""); print sec "\t" $0; next }
    { sec="" }
  ' <<< "$guard_out"
)

if [ "${#offenders[@]}" -eq 0 ]; then
  echo "sync-license-artifacts: nothing to copy (the guard named no offender)."
else
  for row in "${offenders[@]}"; do
    kind="${row%%$'\t'*}"
    path="${row#*$'\t'}"
    case "$kind" in
      license-root) dest="$path/$ROOT_LICENSE"; src="$ROOT_LICENSE" ;;
      license-copy) dest="$path";              src="$ROOT_LICENSE" ;;
      notices-root) dest="$path/$NOTICES";     src="$NOTICES" ;;
      notices-copy) dest="$path";              src="$NOTICES" ;;
      *) echo "sync-license-artifacts: unknown offender kind '$kind'" >&2; exit 1 ;;
    esac
    mkdir -p "$(dirname "$dest")"
    cp "$src" "$dest"
    echo "  wrote $dest"
  done
fi

# Verify by re-running the guard rather than by asserting our own copy succeeded: the guard checks
# byte-identity, gitignore status and the npm `files` array, none of which a `cp` can promise.
echo "sync-license-artifacts: verifying ..."
if ! bash "$GUARD"; then
  echo >&2
  echo "sync-license-artifacts: still RED after syncing. The remaining failure is NOT a stale copy --" >&2
  echo "  read the guard output above (a missing \"files\" entry and an unlicensed crate are the two" >&2
  echo "  it reports that copying cannot fix)." >&2
  exit 1
fi
