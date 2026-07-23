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

# git ls-files (TRACKED only) so an untracked/gitignored local corpus checkout under parser/ can't
# spoof a phantom crate into the requirement -- same tracked-only rationale as the isolation guards.
while IFS= read -r -d '' toml; do
  name=$(grep -m1 -E '^name[[:space:]]*=' "$toml" | sed -E 's/.*"([^"]+)".*/\1/')
  [ -n "$name" ] || { echo "check-version-lists-parsers: $toml has no [package] name -- cannot verify." >&2; exit 1; }
  count=$((count + 1))

  # version_string() stamps each parser as `zzop-parser-<x>={}`; the literal token to find is
  # `<name>=`. A grep -F fixed-string match on that exact token (name + '=') is enough -- it can't
  # collide across crates because the names are distinct and each carries its own '=' in the format!.
  if ! grep -qF "${name}=" "$VERSION_RS"; then
    echo "check-version-lists-parsers: ($name, $VERSION_RS) -- parser crate not reported by version_string()"
    missing=1
  fi
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

echo "check-version-lists-parsers: clean ($count parser crates reported)."
