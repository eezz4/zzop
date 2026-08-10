#!/usr/bin/env bash
# release-matrix.sh — the ONE reader of .github/workflows/prebuild.yml's build-job matrix
# (jobs.build.strategy.matrix.include[]), shared by every guard that needs to know what the release
# actually builds. WHO sources it is answered by `grep -rn release_matrix_entries scripts/`, never by
# a list kept here — as of 2026-08-08 that is check-asset-name-prose.sh (platform + os, to decide
# which asset gets `.exe`), check-deploy-facts-prose.sh (how many platforms) and
# check-server-json-hashes.sh (the platform values in matrix order, plus the
# `--print-matrix-platforms` wrapper the publish job calls), and those three are named because each
# one illustrates a different injected reading of the same records, not because the set is fixed.
#
# Why this file exists (2026-08-08): each of those guards carried its OWN parser of that one
# block — two hand-rolled awk copies plus this file's ancestor, the structural walk that lived in
# check-server-json-hashes.sh. Reshaping the matrix meant editing three places, and missing one left
# THAT guard quietly answering a different question than the other two. The fix is not merging the
# guards — their scan surfaces and their floors must stay separate, and the measurements that killed
# the merge are recorded in this repo's decision ledger (row D62), not repeated here: measured, the
# merged guard tripped over the deliberate bad-token example in a sibling's own header, and the two
# separate zero-floors collapsed into one that a half-broken glob could satisfy. What is shared here
# is the FACT the guards read; the per-consumer difference is injected as an accessor choice, never
# copied as a second parser.
#
# Same layering contract as scripts/lib/tracked-grep.sh, whose workspace_member_dirs() /
# assert_workspace_members_scanned() pair is the precedent this file is shaped after: sourced (never
# executed) by callers that have already cd'd to the repo root, so `. ./scripts/lib/release-matrix.sh`
# resolves from any of them. NOT a guard itself — it lives outside the `scripts/check-*.sh` glob
# check-guards-wired.sh sweeps, has no independent exit status, and needs no pre-commit/CI wiring.
#
# The parse is STRUCTURAL, not a bare `grep platform:`: it walks jobs: -> build: -> strategy: ->
# matrix: -> include: by indentation, so a `${{ matrix.platform }}` interpolation in a step body, a
# `platform:` key in some other job, or a commented-out entry can never enter the result. Full-line
# comments and blank lines are skipped, and a trailing CR is stripped (this repo checks out CRLF).
#
# FLOORS — release_matrix_entries enforces all of them, so no consumer can go green over a set it
# never read (working-agreements §5.5): the awk must emit its summary line at all; the walk must
# reach include:; the entry count must exceed zero; every entry must name a platform; and the record
# list must be non-empty. Any of these returns 1 with a diagnosis on stderr rather than printing an
# empty list, which every consumer would otherwise read as perfect compliance.

# Internal: emit one `E:<platform><TAB><os>` record per include[] entry in matrix order, plus one
# `#:status=... items=N plats=N` summary line saying how far the walk actually got. Keys may appear
# in any order within an entry, and the first key shares the `- ` line. `os:` is captured because the
# asset-name axis needs it (windows is the only platform whose binaries get `.exe`); dropping it here
# would force that guard to keep a second parser, which is the whole defect being removed.
_release_matrix_awk() {
  awk '
    function indent_of(s,   i) { i = match(s, /[^ ]/); return (i == 0 ? -1 : i - 1) }
    function flush_item() {
      if (started) { print "E:" plat "\t" os; if (plat != "") plats++; items++ }
      plat = ""; os = ""; started = 0
    }
    { sub(/\r$/, "") }
    /^[[:space:]]*#/ { next }
    /^[[:space:]]*$/ { next }
    state == 9 { next }
    { ind = indent_of($0); line = $0 }
    state == 0 && line ~ /^jobs:[[:space:]]*$/                 { state = 1; if (state > deep) deep = state; next }
    state == 1 && line ~ /^[[:space:]]+build:[[:space:]]*$/    { state = 2; if (state > deep) deep = state; jobind = ind; next }
    state >= 2 && ind <= jobind                                { flush_item(); state = 9; next }
    state == 2 && line ~ /^[[:space:]]+strategy:[[:space:]]*$/ { state = 3; if (state > deep) deep = state; strind = ind; next }
    state == 3 && ind <= strind                                { state = 2 }
    state == 3 && line ~ /^[[:space:]]+matrix:[[:space:]]*$/   { state = 4; if (state > deep) deep = state; matind = ind; next }
    state == 4 && ind <= matind                                { state = 2 }
    state == 4 && line ~ /^[[:space:]]+include:[[:space:]]*$/  { state = 5; if (state > deep) deep = state; incind = ind; next }
    state == 5 && ind <= incind                                { flush_item(); state = 9; next }
    state == 5 {
      if (line ~ /^[[:space:]]*-[[:space:]]/) { flush_item(); started = 1; sub(/^[[:space:]]*-[[:space:]]*/, "", line) }
      if (line ~ /^[[:space:]]*platform:[[:space:]]*[A-Za-z0-9._-]+[[:space:]]*$/) {
        v = line; sub(/^[[:space:]]*platform:[[:space:]]*/, "", v); sub(/[[:space:]]+$/, "", v); plat = v
      } else if (line ~ /^[[:space:]]*os:[[:space:]]*[^[:space:]]/) {
        v = line; sub(/^[[:space:]]*os:[[:space:]]*/, "", v); sub(/[[:space:]]+$/, "", v); os = v
      }
    }
    END {
      flush_item()
      st = "ok"
      if (deep == 0) st = "no-jobs-key"
      else if (deep == 1) st = "no-build-job"
      else if (deep == 2) st = "no-strategy-key"
      else if (deep == 3) st = "no-matrix-key"
      else if (deep == 4) st = "no-include-key"
      print "#:status=" st " items=" items + 0 " plats=" plats + 0
    }
  ' "$1"
}

# release_matrix_entries <label> <workflow-path>
#
# Prints one TAB-separated `<platform><TAB><os>` record per build-matrix entry, in matrix order.
# Returns 1 — diagnosis on stderr, nothing on stdout — if the file is missing or any floor above
# fails. <label> names the calling guard, so the diagnosis says WHO could not read what.
release_matrix_entries() {
  local label="$1" file="$2"

  if [ ! -f "$file" ]; then
    echo "$label: $file -- missing. The release build matrix this guard derives from lives there." >&2
    return 1
  fi

  local raw status items keys records
  raw="$(_release_matrix_awk "$file")"

  status="$(sed -n 's/^#:status=\([a-z-]*\) .*$/\1/p' <<< "$raw")"
  items="$(sed -n 's/^#:.* items=\([0-9][0-9]*\) .*$/\1/p' <<< "$raw")"
  keys="$(sed -n 's/^#:.* plats=\([0-9][0-9]*\)$/\1/p' <<< "$raw")"
  records="$(sed -n 's/^E://p' <<< "$raw")"

  if [ -z "$status" ] || [ -z "$items" ] || [ -z "$keys" ]; then
    echo "$label: the build-matrix extraction produced no summary line at all. That is" >&2
    echo "  scripts/lib/release-matrix.sh's own awk failing, not a tree problem -- do not silence it" >&2
    echo "  by dropping the check." >&2
    return 1
  fi
  if [ "$status" != ok ]; then
    echo "$label: could not reach the build matrix in $file (stopped at: $status)." >&2
    echo "  The shared reader walks jobs: -> build: -> strategy: -> matrix: -> include: and reads each" >&2
    echo "  entry's platform:/os:. A renamed job or key breaks that walk, and every check derived from" >&2
    echo "  it would then be asserted over ZERO platforms -- vacuously true, which is worse than false." >&2
    echo "  Re-point the walk in scripts/lib/release-matrix.sh; do not hardcode a platform list." >&2
    return 1
  fi
  if [ "$items" -eq 0 ]; then
    echo "$label: read 0 entries out of $file's build matrix. Same class as the anchor failure above:" >&2
    echo "  an empty subject set makes every check derived from it pass without reading anything." >&2
    return 1
  fi
  if [ "$items" -ne "$keys" ]; then
    echo "$label: $file's build matrix has $items include[] entr(ies) but $keys 'platform:' key(s)." >&2
    echo "  Every matrix entry must name a platform -- the release asset names, the .mcpb bundle names" >&2
    echo "  and the npm sub-package names are all built from that value, so an entry without one builds" >&2
    echo "  a binary nothing can name. Fix the matrix; this reader will not guess which entry was meant." >&2
    return 1
  fi
  if [ -z "$records" ]; then
    echo "$label: extracted 0 matrix records despite a well-formed summary -- the awk emitted counts it" >&2
    echo "  did not emit values for. Broken extraction, not a clean tree." >&2
    return 1
  fi

  printf '%s\n' "$records"
}

# release_matrix_platforms <records>
# Projection: the `platform` field of every record release_matrix_entries printed, one per line,
# order preserved. Pure bash, no subprocess — on this box a spawn costs more than the whole parse.
release_matrix_platforms() {
  local rec
  while IFS= read -r rec; do
    [ -n "$rec" ] || continue
    printf '%s\n' "${rec%%$'\t'*}"
  done <<< "$1"
}

# release_matrix_count <records>
# Projection: how many entries the matrix declares. Prints 0 for an empty argument — callers get a
# non-empty record list or a hard failure out of release_matrix_entries, so a 0 here can only mean
# the caller ignored that contract; it is not a value any consumer should ever treat as clean.
release_matrix_count() {
  local rec n=0
  while IFS= read -r rec; do
    [ -n "$rec" ] || continue
    n=$((n + 1))
  done <<< "$1"
  printf '%s\n' "$n"
}
