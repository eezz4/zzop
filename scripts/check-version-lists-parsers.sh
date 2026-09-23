#!/usr/bin/env bash
# version-lists-parsers guard — fails when a parser crate under parser/*/ is not reported by
# crates/facade/src/version.rs::version_string() (the string reported by `zzop version` and by the
# MCP server's serverInfo). That function hand-lists every parser's PARSER_FINGERPRINT in a format!,
# so adding a parser crate and wiring it into dispatch/parser_fingerprint (both COMPILER-enforced,
# so those can't be silently forgotten) still leaves version_string() — the one host-facing parser
# inventory with no compiler backstop — able to under-report the build's parser set.
#
# This already happened once: commit 46b53a3 ("post-release parity: thread parser-sql into
# version()") fixed exactly this omission, and it was detected only AFTER release. parser-csharp is
# present today only because someone remembered. This guard replaces "someone remembered" with a
# mechanical glob-a-dir-against-a-sink check, the same shape as check-guards-wired.sh (glob
# scripts/check-*.sh, assert each is wired) and check-tree-sitter-isolation.sh's allowlist probe.
#
# Source of truth = each parser/*/Cargo.toml's `name` (e.g. zzop-parser-csharp), NOT the directory
# basename or the Language enum: the name is what the crate publishes and what version_string()
# stamps as its `zzop-parser-<x>=` token, so keying on it catches a name/dir divergence too.
#
# No deps beyond git + grep + sed. Exit 1 on any parser crate missing from version.rs, listing each.
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION_RS=crates/facade/src/version.rs

if [ ! -f "$VERSION_RS" ]; then
  echo "check-version-lists-parsers: $VERSION_RS -- missing (moved/renamed?). This guard asserts every" >&2
  echo "  parser crate is reported by version_string(); it cannot run without that file." >&2
  exit 1
fi

missing=0
count=0

# The subject is the SHIPPED half of this file, and narrowing it to that is the whole repair.
#
# Until 2026-09-06 the check was `grep -F` over the whole file, and an external review showed a comment
# could satisfy it. Fixing only that turned out to be the smaller half: this file's own `#[cfg(test)]`
# module lists every expected token as a string literal, so the TEST vouched for the shipped format!
# string. Rename the format token alone and the guard stays green — the assertion that would have
# caught it lives in `cargo test`, which had not run for 47 commits when this was found (the gap
# `scripts/unseen-commits.sh` now prints). A guard whose evidence is the test it is standing in for is
# not a second opinion.
#
# So: everything above the first `#[cfg(test)]`, minus `//` comment lines. POSIX bracket class rather
# than `\s` — this is a BRE, where `\s` is not a space class and the strip would silently do nothing,
# which is how the first attempt at this fix passed its own canary.
set +e
code="$(awk '/^#\[cfg\(test\)\]/ { exit } !/^[[:space:]]*\/\// { print }' "$VERSION_RS")"
rc=$?
set -e
if [ "$rc" -ne 0 ] || [ -z "$code" ]; then
  echo "check-version-lists-parsers: could not read the shipped half of $VERSION_RS (awk exit $rc)." >&2
  echo "  An empty subject is a broken guard, never a file with no code in it. If the test module" >&2
  echo "  moved above the shipped code, re-anchor this split." >&2
  exit 1
fi

# git ls-files (TRACKED only) so an untracked/gitignored local corpus checkout under parser/ can't
# spoof a phantom crate into the requirement -- same tracked-only rationale as the isolation guards.
while IFS= read -r -d '' toml; do
  # `|| true` is load-bearing, not hygiene: `grep` exits 1 when a manifest has no `name =` line, and
  # under `set -e -o pipefail` that status propagates out of the command substitution and kills the
  # script ON THIS LINE — so the diagnosis below, the one written for exactly that manifest, could
  # never speak. Measured 2026-08-14 by pointing the needle at a key no manifest carries: without it
  # the guard exited 1 with ZERO bytes of output (failed closed, but mute, i.e. indistinguishable on
  # screen from a real finding); with it the "$toml has no [package] name" line actually prints.
  # `sed` exiting 0 on empty input does not rescue this: under pipefail the floor's reachability is
  # decided by the last FAILING stage, not the last stage.
  name=$(grep -m1 -E '^name[[:space:]]*=' "$toml" | sed -E 's/.*"([^"]+)".*/\1/' || true)
  [ -n "$name" ] || { echo "check-version-lists-parsers: $toml has no [package] name -- cannot verify." >&2; exit 1; }
  count=$((count + 1))

  # version_string() stamps each parser as `zzop-parser-<x>={}`; the literal token to find is
  # `<name>=`. A grep -F fixed-string match on that exact token (name + '=') is enough -- it can't
  # collide across crates because the names are distinct and each carries its own '=' in the format!.
  # Comment lines are stripped BEFORE the match, once, into `code` above. `grep -F` reads whole files,
  # so a `// ... zzop-parser-sql= ...` note satisfied this check while the real token had been renamed
  # (external review, 2026-09-06) -- a guard a comment can satisfy is one a comment can also break.
  # `case` rather than a pipe into `grep -q`: check-shell-pipe-sigpipe.sh bans that shape repo-wide,
  # and a fixed-string test over a variable needs no process at all.
  case "$code" in
    *"${name}="*) ;;
    *)
      echo "check-version-lists-parsers: ($name, $VERSION_RS) -- parser crate not reported by version_string()"
      missing=1
      ;;
  esac
done < <(git ls-files -z -- 'parser/*/Cargo.toml')

if [ "$missing" -ne 0 ]; then
  echo
  echo "check-version-lists-parsers: every parser/*/ crate must appear in"
  echo "crates/facade/src/version.rs::version_string() as a 'zzop-parser-<x>={}' fingerprint token,"
  echo "so every host (zzop version, MCP serverInfo) reports the complete parser build set."
  echo "Add the crate's PARSER_FINGERPRINT to that format! (and its arg list) -- see commit 46b53a3"
  echo "for the parser-sql precedent this guard exists to prevent recurring."
  exit 1
fi

# An empty subject set is a broken scan, not a clean tree — same class this repo has already paid for
# twice. Measured 2026-07-28: redirecting the pathspec printed "clean (0 parser crates reported)".
if [ "$count" -eq 0 ]; then
  echo "check-version-lists-parsers: FAILED -- found ZERO parser crates. The parser/*/Cargo.toml scan"
  echo "matched nothing, so no crate was checked against version_string(). This repo ships parsers; a"
  echo "zero here is a broken enumeration, not an empty workspace."
  exit 1
fi

echo "check-version-lists-parsers: clean ($count parser crates reported)."
