#!/usr/bin/env bash
# check-dep-closure.sh — which workspace crates can REACH a vendored parser through normal dependency
# edges, censused against a baseline so every change to that set is a reviewable diff.
#
# ## The defect this exists for (2026-09-08, review ledger V122)
#
# `check-swc-isolation.sh` and its three siblings answer their question by grepping Cargo.toml lines.
# That sees a DIRECT declaration and nothing else. So `zzop-config` — a front end whose own manifest
# says it "only ever produces request JSON" — declared `zzop-engine`, inherited swc and ten parser
# crates transitively, and all four guards stayed green. 📏 Measured that day: `zzop-config`'s normal
# closure held 224 crates, ten of them parsers.
#
# The sentence people read those guards as making ("the parsers are isolated") was true only of direct
# edges. `cargo metadata` knows the resolved graph, and the resolved graph is the only thing that can
# answer transitively — so this guard asks it.
#
# ## Why a baseline rather than an allowlist
#
# There is no defensible a-priori rule for who MAY reach swc: the product chain legitimately does
# (`zzop-engine` parses, and facade/summary/cli-bin/mcp are built on it). What is defensible is that the
# set does not grow by accident. So this records the set and fails on any difference, in either
# direction — the same shape as scripts/policy-census.txt, and for the same reason: a number nobody
# would notice changing is not a guard.
#
# A DROP is good news that still fails, on purpose: it means an edge was removed, and the commit that
# removes it should say so by regenerating the baseline.
#
# ## What this guard CANNOT see -- read this before quoting a green run
#   - `syn` is deliberately NOT censused. It reaches almost every crate in the workspace as a
#     PROC-MACRO dependency (serde_derive and friends), and the closure cannot tell that apart from
#     `zzop-parser-rust` using it as a Rust parser, which is what check-syn-isolation.sh actually
#     guards. A baseline for it would be twenty lines of unrelated dependency churn and would ratchet
#     on `serde`'s dependency tree. The text guard stays the only instrument on that axis, with the
#     blind spot it has always had.
#   - Feature-conditional edges: `--all-features` is used, so this reports the WIDEST closure. A crate
#     that only reaches a parser under a feature nobody enables still appears here. That is the safe
#     direction for this question and the wrong one for a shipping-size claim.
#   - This is about REACHABILITY, not use. A crate that can reach swc and never calls it is listed.
#     The four text guards are what answer "is it used", and neither guard subsumes the other.
#
# Invalidation drill (2026-09-08, run): re-adding `zzop-engine` to crates/config/Cargo.toml's
# `[dependencies]` makes this guard name `zzop-config` under three targets and exit 1, while all four
# text-based isolation guards stay green — which is the whole reason this file exists.
#
# `--update` regenerates the baseline. No deps beyond cargo + node.
set -euo pipefail
cd "$(dirname "$0")/.."

BASELINE=scripts/dep-closure-baseline.txt

# The censused targets. Each is a vendored PARSER whose confinement is an architecture claim this repo
# makes out loud; `syn` is absent for the reason in the header.
TARGETS=(swc_core ruff_python_parser tree-sitter)

actual="$(node scripts/lib/dep-closure.mjs "${TARGETS[@]}")"

# An empty census is a broken enumeration, not a workspace where nothing reaches a parser. This repo
# ships a parser per language; the product chain reaches them by construction.
line_count="$(printf '%s\n' "$actual" | grep -c . || true)"
if [ "$line_count" -lt 10 ]; then
  echo "check-dep-closure: FAILED -- the census returned only $line_count line(s) across ${#TARGETS[@]}" >&2
  echo "targets. The product chain reaches every vendored parser by construction, so a number this" >&2
  echo "small means cargo metadata or the traversal stopped working, not that the closure shrank." >&2
  exit 1
fi

if [ "${1:-}" = "--update" ]; then
  {
    echo "# Workspace crates whose NORMAL dependency closure reaches a vendored parser."
    echo "# Regenerate: bash scripts/check-dep-closure.sh --update   (see that script's header for why"
    echo "# this is a baseline and not an allowlist, and why syn is not censused)."
    printf '%s\n' "$actual"
  } > "$BASELINE"
  echo "check-dep-closure: baseline regenerated ($line_count entries) -> $BASELINE"
  exit 0
fi

if [ ! -f "$BASELINE" ]; then
  echo "check-dep-closure: missing $BASELINE -- run: bash scripts/check-dep-closure.sh --update" >&2
  exit 1
fi

expected="$(grep -v '^#' "$BASELINE" | grep -c . || true)"
diff_out="$(diff <(grep -v '^#' "$BASELINE" | grep .) <(printf '%s\n' "$actual") || true)"
if [ -n "$diff_out" ]; then
  echo "check-dep-closure: the parser reachability census has drifted from $BASELINE" >&2
  printf '%s\n' "$diff_out" | sed 's/^/  /' >&2
  echo >&2
  echo "A '>' line is a crate that can now reach a vendored parser and could not before -- check whether" >&2
  echo "it wants the whole engine or one symbol (review ledger V122: it was one symbol, and moving that" >&2
  echo "symbol to zzop-core took zzop-config's closure from 224 crates to 24)." >&2
  echo "A '<' line is an edge that went away: good news, and the commit that removed it should say so --" >&2
  echo "    bash scripts/check-dep-closure.sh --update" >&2
  exit 1
fi

echo "check-dep-closure: clean ($expected reachability entries across ${#TARGETS[@]} vendored parsers)."
