#!/usr/bin/env bash
# Guards against a silently stale zzop-cache: every parser crate under parser/*/ that defines a
# PARSER_FINGERPRINT const bakes its extraction-shape version into the cache key (see each crate's
# own PARSER_FINGERPRINT doc comment for the scheme). If a change touches that crate's src/** but
# never touches the fingerprint's own line, an old cache entry keyed on the unbumped fingerprint
# would keep being served as "still valid" even though what the crate extracts has changed.
#
# Escape hatch: a commit message in the diff range containing `[no-projection-change: <crate-dir>]`
# (e.g. `[no-projection-change: parser-java]`) skips that crate — for changes that provably do not
# alter extraction output (docs, comments, internal refactors with identical results). The core
# stored-shape check below uses the same grammar with token `[no-projection-change: core]`.
#
# The marker must stand ON ITS OWN LINE — leading/trailing whitespace allowed, nothing else on it:
# a git-trailer-like shape, and what nearly every marker in this repo's history already looks like.
# Bit for real 2026-07-24: a bare substring search cannot tell a claim from a mention of one. A
# message recording that a fingerprint had been restamped "rather than using
# [no-projection-change: rules-schema]" — an explicit REFUSAL of the hatch — made this guard
# announce that lane as skipped-by-marker: right verdict (the lane had a real bump), lying reason,
# and the same sentence on a day the bump was missing would wave a stale cache through. Prose can
# mention the token, quote it, or negate it; a line holding nothing but it cannot be written by
# accident, and stays as greppable as before. All four scopes below (parser crates / core stored-shape
# surface / dsl / rules-schema) share the rule — they shared the defect.
#
# Diff range: ${FINGERPRINT_DIFF_RANGE:-origin/main...HEAD}, overridable via env. CI computes this
# against the PR base (or the previous commit on a direct push) — see .github/workflows/ci.yml.
# Local runs commonly lack a fetched origin/main; that degrades gracefully (skip with a notice,
# exit 0) rather than failing a guard the developer has no way to satisfy.
# Note: on push events CI's range is HEAD~1...HEAD, so a multi-commit direct push can slip earlier
# commits past the range — harmless under the PR-only flow, whose PR run diffs the full branch.
#
# WORKING TREE: when the range's right end is HEAD (the default, and every local/CI invocation), the
# file list is taken from the range start to the WORKING TREE rather than to HEAD — see the block
# that computes it for the measurement that forced this. That is what makes this guard runnable from
# .githooks/pre-commit, where it is now wired alongside pre-push and CI.
#
# Escape hatch in the pre-commit lane: the commit-message marker cannot be read there. git runs
# pre-commit BEFORE the message is prepared, so .git/COMMIT_EDITMSG still holds the PREVIOUS commit's
# text — reading it would judge the wrong bytes, the exact defect class this guard's own history is
# made of. So an UNCOMMITTED-only change has no message to carry the marker, and the env hatch
# FINGERPRINT_NO_PROJECTION_CHANGE (space-separated scope tokens) exists for it. It leaves no record
# in history: the commit-message marker stays the recorded form, and this is the lane that would
# otherwise have no hatch at all and so would be answered with `--no-verify`.
#
# And the converse rule, which is what keeps working-tree mode from being decorative: a
# commit-message marker NEVER covers an uncommitted file. A message can only have been written about
# bytes that are in a commit; letting an earlier commit's marker vouch for bytes on disk would hand
# every marker-carrying arc a free pass over the next arc's unexamined edits in the same scope. See
# `uncommitted_files` and `has_skip_marker` for the measurement that forced this.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

# --- Precondition (not range-dependent, so it runs even when the diff range can't resolve): every
# parser crate with a src/ MUST define a PARSER_FINGERPRINT const. Without one the crate cannot
# participate in cache keying at all AND the bump check below would silently skip it — the guard
# would turn itself off for exactly the crate most likely to be wrong (a freshly added parser).
missing_fp=0
for crate_dir in parser/*/; do
  crate="${crate_dir%/}"
  crate_name="$(basename "$crate")"
  [ -d "$crate/src" ] || continue
  if ! grep -rqE '^[[:space:]]*pub const PARSER_FINGERPRINT' "$crate/src" 2>/dev/null; then
    echo "check-parser-fingerprint-bump: $crate_name — parser crate has src/ but no 'pub const PARSER_FINGERPRINT'." >&2
    echo "  Every parser crate must declare a PARSER_FINGERPRINT const (mirror parser-typescript's: a" >&2
    echo "  'pub const PARSER_FINGERPRINT: &str' whose doc comment explains the bump scheme). zzop-cache" >&2
    echo "  keys cached per-file results by it; without one, changes to what this crate extracts can be" >&2
    echo "  served from stale cache entries forever — and this guard cannot watch the crate at all." >&2
    missing_fp=1
  fi
done
if [ "$missing_fp" -ne 0 ]; then
  echo "check-parser-fingerprint-bump: FAILED (missing PARSER_FINGERPRINT const)." >&2
  exit 1
fi

# --- Precondition 2 (also not range-dependent): every path in CORE_SHARED_FILES must still EXIST ---
# The stored-shape check further down watches a HAND LIST of literal paths, and the 300-line ratchet
# keeps splitting and relocating files under it (crates/core/src/io/facts.rs is at 264 lines and
# crates/cache/src/store.rs at 240 — each one feature from a mandatory split). A rename is INVISIBLE
# to that check: git's rename detection means the old path never appears in `git diff --name-only`, so
# "a listed path vanished from the diff" is not a signal any diff-based heuristic can read — the entry
# just stops watching anything and the guard keeps exiting 0. Demonstrated 2026-07-26:
# `git mv crates/cache/src/ir_slice.rs crates/cache/src/slice.rs` plus a new field in the renamed file
# -> exit 0. Only an on-disk assertion closes it, which is the same shape as the PARSER_FINGERPRINT
# precondition above; declared HERE rather than at the use site so it runs even when the diff range
# cannot resolve, and so it runs even in the (common) case where no listed file changed at all.
# What unites these paths and what is deliberately excluded from them is documented at the use site
# below, under "Stored-shape surface".
CORE_SHARED_FILES=(
  crates/core/src/ir.rs
  crates/core/src/paths.rs
  crates/core/src/io.rs
  crates/core/src/io/facts.rs
  crates/core/src/io/key.rs
  crates/core/src/fragments.rs
  crates/core/src/finding.rs
  crates/cache/src/ir_slice.rs
  crates/cache/src/store.rs
)
missing_core=0
for f in "${CORE_SHARED_FILES[@]}"; do
  [ -f "$f" ] || { echo "check-parser-fingerprint-bump: core — CORE_SHARED_FILES lists '$f', which does not exist on disk." >&2; missing_core=1; }
done
if [ "$missing_core" -ne 0 ]; then
  echo "  That list is the only thing watching the bytes zzop-cache persists, and a stale entry watches" >&2
  echo "  NOTHING while still reading green. Fix: point the entry at the file's new path (a pure rename" >&2
  echo "  changes no stored shape, so it needs no CACHE_SCHEMA_VERSION bump), split one entry into the" >&2
  echo "  files a split produced, or delete it if the surface is genuinely gone — and if the move also" >&2
  echo "  changed a persisted field set, bump CACHE_SCHEMA_VERSION as usual." >&2
  echo "check-parser-fingerprint-bump: FAILED (stale CORE_SHARED_FILES path)." >&2
  exit 1
fi

range="${FINGERPRINT_DIFF_RANGE:-origin/main...HEAD}"

# Pull both ends out of "A...B" / "A..B" so we can check they resolve before trusting the range, and
# so the commit-message scan below can be pinned to the SAME commit set `git diff` looks at.
base_ref="${range%%...*}"
base_ref="${base_ref%%..*}"
if [ "$range" = "${range/.../}" ]; then
  three_dot=0
  head_ref="${range#*..}"
else
  three_dot=1
  head_ref="${range#*...}"
fi
# "A.." (and a bare single ref, which leaves the expansion unchanged) mean HEAD on the right, same
# as git's own default.
if [ -z "$head_ref" ] || [ "$head_ref" = "$range" ]; then
  head_ref=HEAD
fi

if ! git rev-parse --verify --quiet "${base_ref}^{commit}" >/dev/null; then
  echo "check-parser-fingerprint-bump: notice — '$base_ref' does not resolve locally (no fetched origin/main?); skipping."
  exit 0
fi

# `left_ref` is the range's true starting commit: the merge-base for `A...B`, plain `A` for `A..B`.
# Everything downstream (the file list, the marker scan, the const-value comparison) is anchored on
# it, so all of them judge exactly the same starting point.
if [ "$three_dot" -eq 0 ]; then
  left_ref="$base_ref"
else
  left_ref="$(git merge-base "$base_ref" "$head_ref" 2>/dev/null || echo "$base_ref")"
fi

# --- Which bytes the file list is read from -------------------------------------------------------
# Bit for real 2026-07-26: this used to be `git diff --name-only "$range"` unconditionally — committed
# bytes only. This project's batch flow leaves a whole batch UNCOMMITTED until the end, so on such a
# batch the range contains no new commits, `changed_files` comes back empty, and every check below is
# a `[ -z ... ] && continue`. The guard then prints "OK (checked range origin/main...HEAD)" having
# looked at nothing — measured on a 100%-uncommitted batch that carried a BLOCKING parser change.
# Vacuously green is the worst kind of green: it is indistinguishable from real coverage.
#
# The asymmetry was already half-fixed and nobody noticed: `const_value_changed`'s "after" side has
# always read the WORKING TREE (`const_value ... < "$file"`), so an uncommitted BUMP already counted
# as bumped while the uncommitted CHANGE that needed it did not. One side of the comparison read the
# disk and the other read HEAD.
#
# So when the right end is HEAD — the default, every local run, and CI (where the working tree IS the
# checked-out HEAD, so this is a no-op there) — diff `left_ref` against the working tree: `git diff
# <commit>` with no second ref, which covers staged AND unstaged, matching .githooks/pre-commit's
# stated "the WORKING TREE, not the index" policy.
#
# When the right end is an explicit commit — .githooks/pre-push passes `<remote_sha>...<local_sha>` —
# the committed-only diff is kept, and is the correct question there: a push carries commits, never
# the working tree.
if [ "$head_ref" = HEAD ]; then
  diff_target="$left_ref"
  scope="$range + working tree"
else
  diff_target="$range"
  scope="$range"
fi

if ! changed_files="$(git diff --name-only "$diff_target" -- 2>&1)"; then
  echo "check-parser-fingerprint-bump: notice — could not diff '$diff_target' (from range '$range'):"
  echo "  $changed_files"
  echo "  skipping."
  exit 0
fi

# The UNCOMMITTED half of that list, kept separately for ONE purpose: deciding whether a
# commit-message marker may speak for a file. Empty when the right end is an explicit commit (nothing
# in that question is uncommitted by construction).
#
# Bit for real 2026-07-26, on the very batch working-tree mode was added for: HEAD's message carries
# `[no-projection-change: core]` and `[no-projection-change: parser-typescript]` for changes IT made,
# and the default base is origin/main — so the moment the next batch started editing those same
# scopes uncommitted, the previous commit's markers waved the new, unexamined bytes straight through.
# That is not a hypothetical: it was live when this line was written, and it is the exact shape that
# would have made working-tree mode a guard that reports coverage it does not have. A commit message
# can only ever have been written ABOUT bytes that are in a commit; letting it vouch for bytes that
# are not is a category error, and this repo's flow (marker-carrying arcs, one after another, all
# based on the same origin/main) reproduces it every single time.
uncommitted_files=""
if [ "$head_ref" = HEAD ]; then
  uncommitted_files="$(git diff --name-only HEAD -- 2>/dev/null || true)"
fi

# Every commit message in the range, one line per line, with surrounding whitespace stripped — so
# the own-line marker test is a plain whole-line literal compare and indentation cannot defeat it.
#
# TWO dots, never three, and never the raw `$range`. Bit for real 2026-07-25: `git diff A...B` shows
# merge-base(A,B)..B — the BRANCH side only — but `git log A...B` is the SYMMETRIC DIFFERENCE, so it
# also reads commits that landed on A after the branch point. `changed_files` and this scan then
# judged different commit sets: a branch that changed a parser without bumping its fingerprint went
# GREEN because an unrelated commit merged into main carried `[no-projection-change: core]`. The
# escape hatch is only ever the branch author's to pull.
marker_scan="$(git log --format=%B "${left_ref}..${head_ref}" -- 2>/dev/null | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//' || true)"

# True only when some line in the range IS the marker for scope $1 ("parser-java" / "core" / "dsl" /
# "rules-schema") and nothing else — see the header for why a mid-sentence mention must not count.
#   -x pins the match to a whole line; -F keeps the bracketed token a literal, so no scope name
#      ever needs regex escaping.
#   Herestring, never `printf big-blob | grep -q`: under pipefail, grep -q exiting on first match
#      SIGPIPEs printf (exit 141) once the blob exceeds the pipe buffer (~64KB) — a REAL match then
#      reads as pipeline failure. Bit for real: a 79KB squash message made this check fail despite
#      the marker being present.
#
# FINGERPRINT_NO_PROJECTION_CHANGE is the same hatch for the pre-commit lane, where no commit message
# exists yet to carry a marker (see the header for why .git/COMMIT_EDITMSG must not be read there).
# Space-separated scope tokens, whole-token match — `core` must never be satisfied by `rules-schema`
# merely containing it, and a substring test is how the commit-marker side of this same hatch was
# wrong until 2026-07-24.
#
# Sets SKIP_REASON to WHICH hatch fired, and every call site prints that rather than a hardcoded
# "skipped via the marker". The two hatches are not interchangeable — one is recorded in history, the
# other is not — and this guard's own 2026-07-24 entry above is about a skip line that announced the
# right verdict with a lying reason. Repeating that shape here would be inexcusable.
#
# $1 = scope token, $2 = the files that put this scope in scope (newline- or space-separated). $2 is
# what lets the commit-message marker be REFUSED for uncommitted bytes — see `uncommitted_files`.
# On a refusal, MARKER_VETO_NOTE explains it, and each call site prints that note in its failure
# branch: a marker that silently stops applying would read as "the author forgot the marker", which
# is the wrong diagnosis and the wrong fix.
SKIP_REASON=""
MARKER_VETO_NOTE=""
has_skip_marker() {
  local scope f
  SKIP_REASON=""
  MARKER_VETO_NOTE=""
  for scope in ${FINGERPRINT_NO_PROJECTION_CHANGE:-}; do
    if [ "$scope" = "$1" ]; then
      SKIP_REASON="the FINGERPRINT_NO_PROJECTION_CHANGE=$1 env hatch (no commit-message marker — this skip leaves no record in history)"
      return 0
    fi
  done
  grep -qxF "[no-projection-change: $1]" <<< "$marker_scan" || return 1
  for f in $2; do
    if grep -qxF "$f" <<< "$uncommitted_files"; then
      MARKER_VETO_NOTE="  NOTE: a [no-projection-change: $1] marker IS present in this range, and it does NOT apply here — $f is UNCOMMITTED, and no commit message in the range can have been written about bytes that are in no commit. Either commit this change with the marker in ITS OWN message, or use FINGERPRINT_NO_PROJECTION_CHANGE=$1 for this run."
      return 1
    fi
  done
  SKIP_REASON="the [no-projection-change: $1] commit-message marker"
  return 0
}

# --- Version-token comparison (replaces per-line diff matching) ---------------------------------
# Bit for real 2026-07-25: every check below used to grep the range's diff for a changed line
# STARTING with `pub const <NAME>`. rustfmt breaks that. Once a value grows past the line limit the
# declaration wraps —
#     pub const PARSER_FINGERPRINT: &str =
#         "typescript/swc_core-71.0.5/0.22.0+...+dispatch-branch-symbol-v1";
# — so a correct bump changes ONLY the value line and matches nothing. parser-typescript crossed
# that width in 2026-07-25's batch and the guard went red on a commit that HAD bumped correctly.
# A guard that cries wolf is one someone silences with the escape hatch, which would then assert
# something false and disable the lane for good. So: compare the VALUE the const actually holds at
# each end of the range. Layout-independent by construction — wrapping, indentation and comment
# rewrites cannot affect it, and only a real value change reads as a bump.
#
# Bit for real 2026-07-27: this read the FIRST STRING LITERAL of the declaration, which stops
# describing the value the moment a token is COMPOSED rather than typed. `CACHE_SCHEMA_VERSION` is now
# `concat!(env!("CARGO_PKG_VERSION"), "+rN")` — deriving its release axis instead of hand-copying it,
# after that hand-copy drifted a whole release behind. Measured against that form, the old extractor
# returns `"CARGO_PKG_VERSION"` at BOTH ends of any range: the compared value freezes, so a real `+rN`
# bump reads as "not bumped" and the core lane goes permanently red — and a lane that is red no matter
# what the author does is a lane whose only remaining exit is the escape hatch, i.e. disabled for good
# (this script's 2026-07-25 entry above is about exactly that progression). The mirrored spelling
# `concat!("rN+", env!(...))` fails the other way, silently: the first literal then tracks `+rN` and
# the release axis becomes invisible. Neither ordering is safe to extract this way. So the comparison
# is over the whole value EXPRESSION (everything between `=` and `;`, whitespace-squeezed so wrapping
# is still invisible), and `env!("CARGO_PKG_VERSION")` is resolved per end of the range — see
# `resolve_pkg_version`. A plain string literal yields exactly what it used to.

# Value EXPRESSION of `const <NAME>` in the blob on stdin — everything between `=` and `;`, with
# runs of whitespace collapsed so rustfmt's wrapping/indentation cannot register as a change. Empty
# when the const is absent. Newlines are folded first so a wrapped declaration reads as one statement.
const_value() {
  tr '\n' ' ' \
    | grep -oE "const[[:space:]]+$1[^;]*;" \
    | head -n1 \
    | sed -e 's/^[^=]*=[[:space:]]*//' \
          -e 's/[[:space:]]*;[[:space:]]*$//' \
          -e 's/[[:space:]][[:space:]]*/ /g' \
    || true
}

# The workspace package version at one end of the range — the value `env!("CARGO_PKG_VERSION")` expands
# to for every crate here (all inherit `version.workspace = true`; see VERSIONING.md, "How versions are
# produced"). $1 = a git ref, or EMPTY for the working tree, matching `const_value_changed`'s own
# left-is-a-ref / right-is-the-disk asymmetry. Emitted WITH its quotes so it drops into a value
# expression as the literal it replaces.
workspace_version() {
  local blob
  if [ -z "${1:-}" ]; then
    blob="$(cat Cargo.toml 2>/dev/null || true)"
  else
    blob="$(git show "$1:Cargo.toml" 2>/dev/null || true)"
  fi
  printf '%s\n' "$blob" | awk '
    /^\[/ { in_wp = ($0 ~ /^\[workspace\.package\]/); next }
    in_wp && /^[[:space:]]*version[[:space:]]*=/ {
      if (match($0, /"[^"]*"/)) { print substr($0, RSTART, RLENGTH); exit }
    }'
}

# Substitutes `env!("CARGO_PKG_VERSION")` in the value expression on stdin with the quoted version $1,
# so a RELEASE bump reads here as the value change it really is rather than as "nothing moved".
# Empty $1 (unreadable Cargo.toml at that end) is a no-op pass-through: both ends then keep the
# unresolved token and only the hand-bumped axis is compared — a narrower question, never a false bump.
resolve_pkg_version() {
  if [ -z "${1:-}" ]; then
    cat
    return 0
  fi
  sed -e "s|env![[:space:]]*([[:space:]]*\"CARGO_PKG_VERSION\"[[:space:]]*)|$1|g"
}

# Path of the file that declared `<const $1>` anywhere under pathspec `<$2>` at $left_ref — empty
# when the const did not exist in that scope at all. PATH-INDEPENDENT ON PURPOSE: the const's file
# is not a stable identity. The 300-line ratchet routinely relocates a const into a freshly split
# module, and a `git mv` must read as neither a bump nor a failure.
const_file_at_left() {
  git grep -lE "^[[:space:]]*(pub[[:space:]]+)?const $1" "$left_ref" -- "$2" 2>/dev/null \
    | head -n1 \
    | cut -d: -f2- \
    || true
}

# Did `<const $2>` — looked up under pathspec `<$3>` at the range start, read from `<file $1>` now —
# hold a different value at the start of the range than it does today? The "now" side reads the
# WORKING TREE, so an uncommitted bump counts as bumped: the pre-commit hook must be able to bless
# the change it is about to commit.
#
# Exit: 0 = value changed (bumped)  |  1 = value identical (not bumped)  |  2 = UNRESOLVABLE.
# Bit for real 2026-07-25: this used to read `git show "$left_ref:$fixed_path"` and let a failure
# fall through as `before=""`, which compares unequal to any real value and so read as A BUMP. A
# plain `git mv` of the file holding the const — exactly what the 300-line ratchet keeps prompting —
# turned a missing bump GREEN. "The const could not be read at the range start" and "the value
# changed" are different events and must not share a verdict; scope-resolving the left side removes
# the rename case entirely, and whatever is left is reported as its own failure rather than a pass.
const_value_changed() {
  local file="$1" name="$2" scope="$3" left_file before after
  left_file="$(const_file_at_left "$name" "$scope")"
  after="$(const_value "$name" < "$file" 2>/dev/null | resolve_pkg_version "$(workspace_version "")" || true)"
  if [ -z "$left_file" ]; then
    # The const did not exist anywhere in this scope at the range start: the crate/surface is new in
    # this range, so no cache entry keyed on an older value can exist. Nothing to compare — pass.
    return 0
  fi
  before="$(git show "$left_ref:$left_file" 2>/dev/null | const_value "$name" | resolve_pkg_version "$(workspace_version "$left_ref")")"
  if [ -z "$before" ] || [ -z "$after" ]; then
    echo "check-parser-fingerprint-bump: $name — could not read its value at $(if [ -z "$before" ]; then echo "the range start ($left_ref:$left_file)"; else echo "HEAD ($file)"; fi)." >&2
    echo "  The declaration must be a single 'const $name ... = <expr>;' statement — a plain string literal, or" >&2
    echo "  a compile-time composition of them (e.g. concat!(env!(\"CARGO_PKG_VERSION\"), \"+rN\")) — for this" >&2
    echo "  guard to compare the two ends of the range. Unreadable is reported as a failure, never as a bump." >&2
    return 2
  fi
  [ "$before" != "$after" ]
}

fail=0
for crate_dir in parser/*/; do
  crate="${crate_dir%/}"
  crate_name="$(basename "$crate")"
  [ -d "$crate/src" ] || continue

  fp_file="$(grep -rlE '^[[:space:]]*pub const PARSER_FINGERPRINT' "$crate/src" 2>/dev/null | head -n1 || true)"
  # Cannot be empty: the precondition loop above already failed loudly for a missing const.
  if [ -z "$fp_file" ]; then
    echo "check-parser-fingerprint-bump: $crate_name — PARSER_FINGERPRINT vanished mid-run?" >&2
    fail=1
    continue
  fi

  crate_changed="$(printf '%s\n' "$changed_files" | grep -F "$crate/src/" || true)"
  [ -z "$crate_changed" ] && continue

  if has_skip_marker "$crate_name" "$crate_changed"; then
    echo "check-parser-fingerprint-bump: $crate_name — src/** changed but skipped via $SKIP_REASON."
    continue
  fi

  fp_rc=0
  const_value_changed "$fp_file" PARSER_FINGERPRINT "$crate/src" || fp_rc=$?
  if [ "$fp_rc" -eq 2 ]; then
    fail=1   # unresolvable — const_value_changed already said why
  elif [ "$fp_rc" -ne 0 ]; then
    echo "check-parser-fingerprint-bump: $crate_name — src/** changed in $scope but PARSER_FINGERPRINT (in $fp_file) was not bumped." >&2
    if [ -n "$MARKER_VETO_NOTE" ]; then echo "$MARKER_VETO_NOTE" >&2; fi
    echo "  Stale-cache risk: zzop-cache keys cached analysis results by this fingerprint; an unbumped fingerprint" >&2
    echo "  means a change to what/how this crate extracts could keep being served from a stale cache entry." >&2
    echo "  Fix: bump PARSER_FINGERPRINT (e.g. append a new '+label-vN' segment, or bump an existing segment's version)." >&2
    echo "  Escape hatch: if this change provably does not alter extraction output, add '[no-projection-change: $crate_name]'" >&2
    echo "  to a commit message in the range, ON A LINE OF ITS OWN — a mention inside a sentence does not count." >&2
    echo "  Nothing committed yet, so no message can carry the marker? FINGERPRINT_NO_PROJECTION_CHANGE=$crate_name" >&2
    echo "  is the env form of the same hatch — it leaves no record in history; see this script's header." >&2
    fail=1
  fi
done

# --- Stored-shape surface (`core` scope) ---
# Everything whose DEFINITION fixes the bytes zzop-cache persists. A change here invalidates EVERY
# parser's entries at once — yet no parser crate's own src/** changes, so the per-crate fingerprint
# loop above never fires. The cache-wide invalidator is CACHE_SCHEMA_VERSION (a bump is a bulk wipe —
# see its doc comment). Scope token stays `core` (unchanged hatch spelling) even though the list now
# reaches outside crates/core: what unites these files is "defines a persisted shape", not a crate.
#
# Two groups:
#  1. crates/core shared types parser projections ride: ImportMap/SourceSymbol/ReExport/QueryCallSite
#     (ir.rs), IoFacts (io/facts.rs), key normalization (io/key.rs + the io.rs module root),
#     is_test_file (paths.rs), the eight fragment types (fragments.rs), and Finding/Severity
#     (finding.rs — the payload of the FINDINGS entry, the cache's second entry kind).
#  2. zzop-cache's own definitions of the persisted containers: FileIrSlice (ir_slice.rs) and the
#     IrEntry/FindingsEntry envelopes (store.rs).
#
# Bit for real 2026-07-26: this list watched only group 1 while its own comment cited zzop-cache's
# FileIrSlice — the file that DEFINES the stored shape was the one file not watched. A commit adding
# a `#[serde(default)]` field to ir_slice.rs alone touches neither group 1 nor crates/core/src/dsl/**,
# so nothing demanded a CACHE_SCHEMA_VERSION bump, and serde then deserializes every pre-existing
# entry WITHOUT ERROR into the field's default. For `function_spans` that default is an empty vec, and
# `MethodScan::after_in_same_function`'s absent-fact degrade does NOT drop the pairing — so the false
# positives that fact exists to remove come back silently. v0.24.0 made caching default-on, so every
# user now has a warm cache for that to happen against. Same reasoning covers finding.rs and store.rs:
# a field added to Finding or to either entry envelope is invisible to ruleset_fingerprint (which
# hashes pack content + interpreter/structural version tokens, never a Rust struct's field set) and to
# FORMAT_VERSION (which nobody is forced to bump), so CACHE_SCHEMA_VERSION is again the only lever.
#
# Deliberately EXCLUDED:
#  - io/link.rs + io/link/** — the cross-layer linker runs fresh on every analyze over already-cached
#    per-file facts; its results are never cached, so a link-algorithm change cannot poison an entry.
#  - crates/cache/src/hash.rs — a digest128 change moves every entry FILENAME and every content_hash,
#    so old entries stop being found. That is a cold cache, not a stale hit: a miss is always safe.
#  - crates/cache/src/key.rs — still excluded, but NOT for the reason this list used to give. The old
#    text said CacheKey "is never written to disk (store.rs copies its fields into the entry
#    individually), so it has no stored shape". That was true of the old store.rs and stopped being
#    true on 2026-07-26: `IrEntry`/`FindingsEntry` now hold `#[serde(flatten)] key: IrKey` /
#    `key: CacheKey`, so key.rs's field NAMES and ORDER are literally the entry's JSON keys, and
#    `cache_key_struct!` derives `digest_input()` (the entry filename's input) from that same field
#    list. key.rs does define stored shape.
#    The verdict survives on the MISS-SAFETY argument instead: every mutation of a key type degrades to
#    a cache MISS, never to a stale HIT. Add / rename / reorder a field and `digest_input()` changes,
#    so the lookup addresses a different entry filename and the old entry is never found; and even on a
#    filename collision every read compares the STORED key value against the requested one and treats a
#    mismatch as a miss. `#[serde(default)]` — the mechanism that makes the ir_slice.rs/store.rs case
#    silent — cannot bite here either, because a key is what the lookup is BUILT from, not a field set
#    read back out of an entry and trusted. A miss is always safe, so CACHE_SCHEMA_VERSION buys
#    nothing. key.rs's real failure mode is a KEYING GAP — an input that should be in the key and is
#    not — whose fix is threading the field through ir_path()/get_ir(); listing it here would advertise
#    the wrong remedy. (Do not reason from the old premise: it is the argument that changed, not the
#    conclusion.)
#
# Noise check before widening (git log per file, whole history): store.rs 2 commits, ir_slice.rs 4,
# finding.rs 5 — these are cold files, so the added false-red surface is near zero and the
# `[no-projection-change: core]` hatch covers the doc-comment-only edits that do land.
#
# The CORE_SHARED_FILES array itself is declared with the preconditions at the top of this script,
# together with the on-disk existence assertion that keeps a rename from silently emptying it.
core_changed=""
for f in "${CORE_SHARED_FILES[@]}"; do
  if grep -qxF "$f" <<< "$changed_files"; then
    core_changed="$core_changed $f"
  fi
done
if [ -n "$core_changed" ]; then
  if has_skip_marker core "$core_changed"; then
    echo "check-parser-fingerprint-bump: core — stored-shape surface changed but skipped via $SKIP_REASON."
  else
    schema_files="$(grep -rlE '^[[:space:]]*pub const CACHE_SCHEMA_VERSION' crates/*/src 2>/dev/null || true)"
    schema_count="$(printf '%s' "$schema_files" | grep -c . || true)"
    schema_file="$(printf '%s\n' "$schema_files" | head -n1)"
    if [ -z "$schema_file" ]; then
      echo "check-parser-fingerprint-bump: core — no 'pub const CACHE_SCHEMA_VERSION' found under crates/*/src; cannot verify the cache-wide bump." >&2
      fail=1
    elif [ "$schema_count" -ne 1 ]; then
      # Exactly-one enforcement: a second definition would make this check silently diff the
      # wrong file and miss a real bump-miss.
      echo "check-parser-fingerprint-bump: core — expected exactly one 'pub const CACHE_SCHEMA_VERSION' under crates/*/src, found $schema_count:" >&2
      printf '    %s\n' $schema_files >&2
      fail=1
    else
      core_rc=0
      const_value_changed "$schema_file" CACHE_SCHEMA_VERSION crates || core_rc=$?
      if [ "$core_rc" -eq 2 ]; then
        fail=1   # unresolvable — const_value_changed already said why
      elif [ "$core_rc" -ne 0 ]; then
        echo "check-parser-fingerprint-bump: core stored-shape surface changed in $scope but CACHE_SCHEMA_VERSION (in $schema_file) was not bumped:" >&2
        if [ -n "$MARKER_VETO_NOTE" ]; then echo "$MARKER_VETO_NOTE" >&2; fi
        printf '    %s\n' $core_changed >&2
        echo "  Cache-poisoning risk: these files DEFINE the bytes zzop-cache persists (FileIrSlice and the shared" >&2
        echo "  types it embeds, Finding, and the IrEntry/FindingsEntry envelopes). An unbumped schema version means" >&2
        echo "  stale entries — keyed on fingerprints that never see any of these files — keep being served as valid" >&2
        echo "  even though the shapes they carry have changed. #[serde(default)] makes that SILENT: a missing field" >&2
        echo "  deserializes to an empty default that is indistinguishable from 'genuinely has none'." >&2
        echo "  Fix: bump CACHE_SCHEMA_VERSION's hand-held '+rN' counter in $schema_file (its release axis is" >&2
        echo "  derived from CARGO_PKG_VERSION and does not move inside a cycle; a bump bulk-wipes the cache —" >&2
        echo "  see its doc comment for both axes)." >&2
        echo "  Escape hatch: if this change provably does not alter any projected/cached shape, add" >&2
        echo "  '[no-projection-change: core]' to a commit message in the range, ON A LINE OF ITS OWN — a mention" >&2
        echo "  inside a sentence does not count. Nothing committed yet? FINGERPRINT_NO_PROJECTION_CHANGE=core is" >&2
        echo "  the env form of the same hatch — it leaves no record in history; see this script's header." >&2
        fail=1
      fi
    fi
  fi
fi

# --- DSL interpreter surface (crates/core/src/dsl) ---
# A DSL pack's own JSON content already self-invalidates via `{pack:?}` in the ruleset fingerprint,
# but the *interpreter* that walks that JSON (matcher evaluation, the suppress-marker window,
# MethodScan's trigger-in-loop containment gate, ...) is pure Rust logic with no pack content to
# hash — a semantics-only change here alters findings for byte-identical source AND identical pack
# content, invisible to every parser's own PARSER_FINGERPRINT and to a pack's own hash alike. The
# invalidator for that gap is DSL_INTERPRETER_FINGERPRINT (crates/engine/src/cache.rs — see its own
# doc comment for the bump scheme); CACHE_SCHEMA_VERSION (a bulk wipe) is also accepted, same as the
# core stored-shape-surface check above. Scope: the interpreter's own src tree only — rules/dsl/*.json
# pack content and DSL rule catalogs elsewhere are not the interpreter itself.
dsl_changed="$(printf '%s\n' "$changed_files" | grep -E '^crates/core/src/dsl/' || true)"
if [ -n "$dsl_changed" ]; then
  if has_skip_marker dsl "$dsl_changed"; then
    echo "check-parser-fingerprint-bump: dsl — crates/core/src/dsl/** changed but skipped via $SKIP_REASON."
  else
    dsl_fp_files="$(grep -rlE '^[[:space:]]*(pub[[:space:]]+)?const DSL_INTERPRETER_FINGERPRINT' crates/*/src 2>/dev/null || true)"
    dsl_fp_count="$(printf '%s' "$dsl_fp_files" | grep -c . || true)"
    dsl_fp_file="$(printf '%s\n' "$dsl_fp_files" | head -n1)"
    schema_files="$(grep -rlE '^[[:space:]]*pub const CACHE_SCHEMA_VERSION' crates/*/src 2>/dev/null || true)"
    schema_count="$(printf '%s' "$schema_files" | grep -c . || true)"
    schema_file="$(printf '%s\n' "$schema_files" | head -n1)"
    if [ -z "$dsl_fp_file" ]; then
      echo "check-parser-fingerprint-bump: dsl — no '(pub )const DSL_INTERPRETER_FINGERPRINT' found under crates/*/src; cannot verify the interpreter-semantics bump." >&2
      fail=1
    elif [ "$dsl_fp_count" -ne 1 ]; then
      echo "check-parser-fingerprint-bump: dsl — expected exactly one 'const DSL_INTERPRETER_FINGERPRINT' under crates/*/src, found $dsl_fp_count:" >&2
      printf '    %s\n' $dsl_fp_files >&2
      fail=1
    elif [ -z "$schema_file" ] || [ "$schema_count" -ne 1 ]; then
      # Same exactly-one enforcement as the core stored-shape-surface check above -- reuse its
      # verdict instead of re-deriving a second, possibly-divergent judgment about CACHE_SCHEMA_VERSION.
      echo "check-parser-fingerprint-bump: dsl — could not uniquely resolve CACHE_SCHEMA_VERSION under crates/*/src (found $schema_count); cannot check the escape valve." >&2
      fail=1
    else
      dsl_rc=0
      const_value_changed "$dsl_fp_file" DSL_INTERPRETER_FINGERPRINT crates || dsl_rc=$?
      schema_rc=0
      const_value_changed "$schema_file" CACHE_SCHEMA_VERSION crates || schema_rc=$?
      if [ "$dsl_rc" -eq 0 ] || [ "$schema_rc" -eq 0 ]; then
        : # one of the two invalidators moved — covered
      elif [ "$dsl_rc" -eq 2 ] || [ "$schema_rc" -eq 2 ]; then
        fail=1   # unresolvable — const_value_changed already said why
      else
        echo "check-parser-fingerprint-bump: crates/core/src/dsl/** changed in $scope but neither DSL_INTERPRETER_FINGERPRINT (in $dsl_fp_file) nor CACHE_SCHEMA_VERSION (in $schema_file) was bumped:" >&2
        if [ -n "$MARKER_VETO_NOTE" ]; then echo "$MARKER_VETO_NOTE" >&2; fi
        printf '    %s\n' $dsl_changed >&2
        echo "  Stale-cache risk: the DSL interpreter's own semantics are not covered by any pack's content hash or any parser's" >&2
        echo "  PARSER_FINGERPRINT -- an unbumped token means a change to how the interpreter matches/evaluates could keep being" >&2
        echo "  served from a stale per-file findings cache entry." >&2
        echo "  Fix: bump DSL_INTERPRETER_FINGERPRINT's trailing counter in $dsl_fp_file (see its own doc comment for the scheme)." >&2
        echo "  Escape hatch: if this change provably does not alter any DSL rule's findings, add" >&2
        echo "  '[no-projection-change: dsl]' to a commit message in the range, ON A LINE OF ITS OWN — a mention" >&2
        echo "  inside a sentence does not count. Nothing committed yet? FINGERPRINT_NO_PROJECTION_CHANGE=dsl is" >&2
        echo "  the env form of the same hatch — it leaves no record in history; see this script's header." >&2
        fail=1
      fi
    fi
  fi
fi

# --- Structural rule-schema surface (rules/native/rules-schema/src) ---
# zzop_rules_schema's native (non-DSL) Prisma rule logic has no pack JSON to hash into the ruleset
# fingerprint the way a DSL pack does -- its version counter is STRUCTURAL_RULES_VERSION
# (rules/native/rules-schema/src/structural.rs), folded into the fingerprint via
# `schema_structural_fingerprint()` in crates/engine/src/cache.rs. A change anywhere under this
# crate's src/** (a rule body, a MESSAGE template, disable-hint text, the shared schema IR types it
# walks) can change `schema/*` finding content for byte-identical source without touching that
# fingerprint unless STRUCTURAL_RULES_VERSION itself is bumped; CACHE_SCHEMA_VERSION (a bulk wipe)
# is also accepted, same escape valve as the two checks above.
schema_src_changed="$(printf '%s\n' "$changed_files" | grep -E '^rules/native/rules-schema/src/' || true)"
if [ -n "$schema_src_changed" ]; then
  if has_skip_marker rules-schema "$schema_src_changed"; then
    echo "check-parser-fingerprint-bump: rules-schema — rules/native/rules-schema/src/** changed but skipped via $SKIP_REASON."
  else
    struct_fp_files="$(grep -rlE '^[[:space:]]*pub const STRUCTURAL_RULES_VERSION' rules/native/*/src 2>/dev/null || true)"
    struct_fp_count="$(printf '%s' "$struct_fp_files" | grep -c . || true)"
    struct_fp_file="$(printf '%s\n' "$struct_fp_files" | head -n1)"
    schema_files="$(grep -rlE '^[[:space:]]*pub const CACHE_SCHEMA_VERSION' crates/*/src 2>/dev/null || true)"
    schema_count="$(printf '%s' "$schema_files" | grep -c . || true)"
    schema_file="$(printf '%s\n' "$schema_files" | head -n1)"
    if [ -z "$struct_fp_file" ]; then
      echo "check-parser-fingerprint-bump: rules-schema — no 'pub const STRUCTURAL_RULES_VERSION' found under rules/native/*/src; cannot verify the bump." >&2
      fail=1
    elif [ "$struct_fp_count" -ne 1 ]; then
      echo "check-parser-fingerprint-bump: rules-schema — expected exactly one 'pub const STRUCTURAL_RULES_VERSION' under rules/native/*/src, found $struct_fp_count:" >&2
      printf '    %s\n' $struct_fp_files >&2
      fail=1
    elif [ -z "$schema_file" ] || [ "$schema_count" -ne 1 ]; then
      echo "check-parser-fingerprint-bump: rules-schema — could not uniquely resolve CACHE_SCHEMA_VERSION under crates/*/src (found $schema_count); cannot check the escape valve." >&2
      fail=1
    else
      struct_rc=0
      const_value_changed "$struct_fp_file" STRUCTURAL_RULES_VERSION rules/native || struct_rc=$?
      schema_rc=0
      const_value_changed "$schema_file" CACHE_SCHEMA_VERSION crates || schema_rc=$?
      if [ "$struct_rc" -eq 0 ] || [ "$schema_rc" -eq 0 ]; then
        : # one of the two invalidators moved — covered
      elif [ "$struct_rc" -eq 2 ] || [ "$schema_rc" -eq 2 ]; then
        fail=1   # unresolvable — const_value_changed already said why
      else
        echo "check-parser-fingerprint-bump: rules/native/rules-schema/src/** changed in $scope but neither STRUCTURAL_RULES_VERSION (in $struct_fp_file) nor CACHE_SCHEMA_VERSION (in $schema_file) was bumped:" >&2
        if [ -n "$MARKER_VETO_NOTE" ]; then echo "$MARKER_VETO_NOTE" >&2; fi
        printf '    %s\n' $schema_src_changed >&2
        echo "  Stale-cache risk: zzop-cache folds STRUCTURAL_RULES_VERSION into the ruleset fingerprint for every" >&2
        echo "  Prisma schema/* finding; an unbumped token means a rule-body/message/disable-hint change here could" >&2
        echo "  keep being served from a stale per-file findings cache entry." >&2
        echo "  Fix: bump STRUCTURAL_RULES_VERSION in $struct_fp_file." >&2
        echo "  Escape hatch: if this change provably does not alter any schema/* finding's output, add" >&2
        echo "  '[no-projection-change: rules-schema]' to a commit message in the range, ON A LINE OF ITS OWN — a" >&2
        echo "  mention inside a sentence does not count. Nothing committed yet?" >&2
        echo "  FINGERPRINT_NO_PROJECTION_CHANGE=rules-schema is the env form of the same hatch — it leaves no" >&2
        echo "  record in history; see this script's header." >&2
        fail=1
      fi
    fi
  fi
fi

if [ "$fail" -ne 0 ]; then
  echo "check-parser-fingerprint-bump: FAILED." >&2
  exit 1
fi

echo "check-parser-fingerprint-bump: OK (checked $scope)"
