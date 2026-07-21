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
# shared-type check below uses the same grammar with token `[no-projection-change: core]`.
#
# Diff range: ${FINGERPRINT_DIFF_RANGE:-origin/main...HEAD}, overridable via env. CI computes this
# against the PR base (or the previous commit on a direct push) — see .github/workflows/ci.yml.
# Local runs commonly lack a fetched origin/main; that degrades gracefully (skip with a notice,
# exit 0) rather than failing a guard the developer has no way to satisfy.
# Note: on push events CI's range is HEAD~1...HEAD, so a multi-commit direct push can slip earlier
# commits past the range — harmless under the PR-only flow, whose PR run diffs the full branch.
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

range="${FINGERPRINT_DIFF_RANGE:-origin/main...HEAD}"

# Pull the left side out of "A...B" or "A..B" so we can check it resolves before trusting the range.
base_ref="${range%%...*}"
base_ref="${base_ref%%..*}"

if ! git rev-parse --verify --quiet "${base_ref}^{commit}" >/dev/null; then
  echo "check-parser-fingerprint-bump: notice — '$base_ref' does not resolve locally (no fetched origin/main?); skipping."
  exit 0
fi

if ! changed_files="$(git diff --name-only "$range" -- 2>&1)"; then
  echo "check-parser-fingerprint-bump: notice — could not diff range '$range':"
  echo "  $changed_files"
  echo "  skipping."
  exit 0
fi

commit_messages="$(git log --format=%B "$range" -- 2>/dev/null || true)"

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

  # Herestring, never `printf big-blob | grep -q`: under pipefail, grep -q exiting on first match
  # SIGPIPEs printf (exit 141) once the blob exceeds the pipe buffer (~64KB) — a REAL match then
  # reads as pipeline failure. Bit for real: a 79KB squash message made this marker check fail
  # despite the marker being present.
  if grep -qF "[no-projection-change: $crate_name]" <<< "$commit_messages"; then
    echo "check-parser-fingerprint-bump: $crate_name — src/** changed but skipped via [no-projection-change: $crate_name] marker."
    continue
  fi

  fp_diff="$(git diff -U0 "$range" -- "$fp_file" 2>/dev/null | grep -E '^[+-][[:space:]]*pub const PARSER_FINGERPRINT' || true)"
  if [ -z "$fp_diff" ]; then
    echo "check-parser-fingerprint-bump: $crate_name — src/** changed in $range but PARSER_FINGERPRINT (in $fp_file) was not bumped." >&2
    echo "  Stale-cache risk: zzop-cache keys cached analysis results by this fingerprint; an unbumped fingerprint" >&2
    echo "  means a change to what/how this crate extracts could keep being served from a stale cache entry." >&2
    echo "  Fix: bump PARSER_FINGERPRINT (e.g. append a new '+label-vN' segment, or bump an existing segment's version)." >&2
    echo "  Escape hatch: if this change provably does not alter extraction output, add '[no-projection-change: $crate_name]'" >&2
    echo "  to a commit message in the range." >&2
    fail=1
  fi
done

# --- Core shared-type surface (crates/core) ---
# Parser projections ride crates/core's shared types (ImportMap in ir.rs, IoFacts in io/facts.rs,
# key normalization in io/key.rs + the io.rs module root, is_test_file in paths.rs): those types
# are baked into every parser's cached per-file artifact (see zzop-cache's FileIrSlice), so a
# change to them invalidates EVERY parser's cache entries at once — yet no parser crate's own
# src/** changes, so the per-crate fingerprint loop above never fires. The cache-wide invalidator
# for a shared-type change is CACHE_SCHEMA_VERSION (a bump is a bulk wipe — see its doc comment).
# Scope: the projected-surface files only. io/link.rs + io/link/** are deliberately EXCLUDED —
# the cross-layer linker runs fresh on every analyze over already-cached per-file facts and its
# results are never cached, so a link-algorithm change cannot poison a cache entry.
# fragments.rs IS included: its eight fragment types (RouterMountFragment, WrapperDefFragment,
# ClassShapeFragment, ...) are serialized fields of the persisted FileIrSlice, exactly the
# poisoning surface this check guards (its omission at introduction was an opus-review BLOCKING).
CORE_SHARED_FILES=(
  crates/core/src/ir.rs
  crates/core/src/paths.rs
  crates/core/src/io.rs
  crates/core/src/io/facts.rs
  crates/core/src/io/key.rs
  crates/core/src/fragments.rs
)
core_changed=""
for f in "${CORE_SHARED_FILES[@]}"; do
  if grep -qxF "$f" <<< "$changed_files"; then
    core_changed="$core_changed $f"
  fi
done
if [ -n "$core_changed" ]; then
  if grep -qF "[no-projection-change: core]" <<< "$commit_messages"; then
    echo "check-parser-fingerprint-bump: core — shared-type surface changed but skipped via [no-projection-change: core] marker."
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
      schema_diff="$(git diff -U0 "$range" -- "$schema_file" 2>/dev/null | grep -E '^[+-][[:space:]]*pub const CACHE_SCHEMA_VERSION' || true)"
      if [ -z "$schema_diff" ]; then
        echo "check-parser-fingerprint-bump: core shared-type surface changed in $range but CACHE_SCHEMA_VERSION (in $schema_file) was not bumped:" >&2
        printf '    %s\n' $core_changed >&2
        echo "  Cache-poisoning risk: every parser bakes these shared types into its cached per-file artifacts; an" >&2
        echo "  unbumped schema version means every parser's stale entries — keyed on fingerprints that never see" >&2
        echo "  crates/core — keep being served as valid even though the shapes they carry have changed." >&2
        echo "  Fix: bump CACHE_SCHEMA_VERSION in $schema_file (a bump bulk-wipes the cache — see its doc comment)." >&2
        echo "  Escape hatch: if this change provably does not alter any projected/cached shape, add" >&2
        echo "  '[no-projection-change: core]' to a commit message in the range." >&2
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
# core shared-type-surface check above. Scope: the interpreter's own src tree only — rules/dsl/*.json
# pack content and DSL rule catalogs elsewhere are not the interpreter itself.
dsl_changed="$(printf '%s\n' "$changed_files" | grep -E '^crates/core/src/dsl/' || true)"
if [ -n "$dsl_changed" ]; then
  if grep -qF "[no-projection-change: dsl]" <<< "$commit_messages"; then
    echo "check-parser-fingerprint-bump: dsl — crates/core/src/dsl/** changed but skipped via [no-projection-change: dsl] marker."
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
      # Same exactly-one enforcement as the core shared-type-surface check above -- reuse its
      # verdict instead of re-deriving a second, possibly-divergent judgment about CACHE_SCHEMA_VERSION.
      echo "check-parser-fingerprint-bump: dsl — could not uniquely resolve CACHE_SCHEMA_VERSION under crates/*/src (found $schema_count); cannot check the escape valve." >&2
      fail=1
    else
      dsl_fp_diff="$(git diff -U0 "$range" -- "$dsl_fp_file" 2>/dev/null | grep -E '^[+-][[:space:]]*(pub[[:space:]]+)?const DSL_INTERPRETER_FINGERPRINT' || true)"
      schema_diff="$(git diff -U0 "$range" -- "$schema_file" 2>/dev/null | grep -E '^[+-][[:space:]]*pub const CACHE_SCHEMA_VERSION' || true)"
      if [ -z "$dsl_fp_diff" ] && [ -z "$schema_diff" ]; then
        echo "check-parser-fingerprint-bump: crates/core/src/dsl/** changed in $range but neither DSL_INTERPRETER_FINGERPRINT (in $dsl_fp_file) nor CACHE_SCHEMA_VERSION (in $schema_file) was bumped:" >&2
        printf '    %s\n' $dsl_changed >&2
        echo "  Stale-cache risk: the DSL interpreter's own semantics are not covered by any pack's content hash or any parser's" >&2
        echo "  PARSER_FINGERPRINT -- an unbumped token means a change to how the interpreter matches/evaluates could keep being" >&2
        echo "  served from a stale per-file findings cache entry." >&2
        echo "  Fix: bump DSL_INTERPRETER_FINGERPRINT's trailing counter in $dsl_fp_file (see its own doc comment for the scheme)." >&2
        echo "  Escape hatch: if this change provably does not alter any DSL rule's findings, add" >&2
        echo "  '[no-projection-change: dsl]' to a commit message in the range." >&2
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
  if grep -qF "[no-projection-change: rules-schema]" <<< "$commit_messages"; then
    echo "check-parser-fingerprint-bump: rules-schema — rules/native/rules-schema/src/** changed but skipped via [no-projection-change: rules-schema] marker."
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
      struct_fp_diff="$(git diff -U0 "$range" -- "$struct_fp_file" 2>/dev/null | grep -E '^[+-][[:space:]]*pub const STRUCTURAL_RULES_VERSION' || true)"
      schema_diff="$(git diff -U0 "$range" -- "$schema_file" 2>/dev/null | grep -E '^[+-][[:space:]]*pub const CACHE_SCHEMA_VERSION' || true)"
      if [ -z "$struct_fp_diff" ] && [ -z "$schema_diff" ]; then
        echo "check-parser-fingerprint-bump: rules/native/rules-schema/src/** changed in $range but neither STRUCTURAL_RULES_VERSION (in $struct_fp_file) nor CACHE_SCHEMA_VERSION (in $schema_file) was bumped:" >&2
        printf '    %s\n' $schema_src_changed >&2
        echo "  Stale-cache risk: zzop-cache folds STRUCTURAL_RULES_VERSION into the ruleset fingerprint for every" >&2
        echo "  Prisma schema/* finding; an unbumped token means a rule-body/message/disable-hint change here could" >&2
        echo "  keep being served from a stale per-file findings cache entry." >&2
        echo "  Fix: bump STRUCTURAL_RULES_VERSION in $struct_fp_file." >&2
        echo "  Escape hatch: if this change provably does not alter any schema/* finding's output, add" >&2
        echo "  '[no-projection-change: rules-schema]' to a commit message in the range." >&2
        fail=1
      fi
    fi
  fi
fi

if [ "$fail" -ne 0 ]; then
  echo "check-parser-fingerprint-bump: FAILED." >&2
  exit 1
fi

echo "check-parser-fingerprint-bump: OK (checked range $range)"
