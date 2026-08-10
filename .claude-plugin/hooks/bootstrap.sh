#!/usr/bin/env bash
# SessionStart hook — put a working `zzop-mcp` in the plugin's own data directory, then get out of
# the way.
#
# Why this exists: `plugin.json` used to declare `"command": "zzop-mcp"`, a bare PATH lookup. That
# meant installing the plugin did NOT give you a working server — you had to find, download, and
# place the binary yourself. A 2026-07-24 dogfooding session proved how badly that fails: a
# developer, with an agent helping, ended up with a 0.18.0 binary on PATH while every manifest in
# the repo said 0.22.0, and nothing anywhere pointed at the fix.
#
# WHAT IT DOES NOT DO: it never silently replaces a binary you already have. zzop sells
# deterministic analysis; an engine that swaps itself between sessions would change findings with no
# code change and blow away the analysis cache (a version change moves every cache fingerprint), so
# an update you did not ask for is a bug, not a feature. Missing binary => install it (that is not a
# choice, it is the install). Outdated binary => say so on stdout and exit; SessionStart stdout
# reaches the assistant, so you get told, and you decide.
#
# The binary itself stays network-free by design (see the mcp-distribution decision): every HTTP
# request in the zzop story lives here, in the delivery layer, never in the analyzer that reads your
# source.
#
# PLATFORMS: this is a POSIX shell script, and nothing in it is Git-Bash-specific — the `uname` cases
# below accept MSYS/MinGW/Cygwin environments alike. What is Git-Bash-specific is Claude Code's own
# shell discovery on Windows: it documents Git Bash for shell-form hooks and falls back to PowerShell
# when that is absent, and under the PowerShell fallback this script does not run. Whether it picks
# up some OTHER bash is not documented, so Git for Windows is the supported path. In practice that
# costs a normal setup nothing: a plugin marketplace IS a git clone, so having this plugin means
# having git, and on Windows that overwhelmingly means Git for Windows, which installs bash.exe in
# its own tree next to git.exe. Where it does fail, it fails visibly and harmlessly — a SessionStart
# failure cannot block a session — and the messages below name the exact path to drop a binary at.
# The Node wrapper that would remove the constraint is exactly the dependency zzop removed.
#
# Exit code is ALWAYS 0. A hook that fails loudly on every offline session is worse than the problem
# it solves; real trouble is reported on stderr (which the user sees) and, when there is no binary at
# all, surfaces immediately anyway as an MCP server that will not start.

set -uo pipefail

REPO="eezz4/zzop"
DEST_DIR="${CLAUDE_PLUGIN_DATA:-}"

if [ -z "$DEST_DIR" ]; then
  echo "zzop bootstrap: CLAUDE_PLUGIN_DATA is unset — cannot place the binary." >&2
  exit 0
fi

# One fixed filename on every OS, extension included nowhere: `plugin.json`'s `mcpServers` entry is a
# single command string with no per-OS branch available to it, so the path it names must be the same
# everywhere. Windows runs a PE image by content when handed an absolute path, so the missing `.exe`
# does not stop it from launching.
DEST="$DEST_DIR/zzop-mcp"

case "$(uname -s)" in
  Darwin)
    case "$(uname -m)" in
      arm64 | aarch64) PLATFORM="darwin-arm64" ;;
      *) PLATFORM="darwin-x64" ;;
    esac
    ASSET_EXT=""
    ;;
  Linux)
    case "$(uname -m)" in
      aarch64 | arm64) PLATFORM="linux-arm64-gnu" ;;
      *) PLATFORM="linux-x64-gnu" ;;
    esac
    ASSET_EXT=""
    ;;
  MINGW* | MSYS* | CYGWIN* | Windows_NT)
    PLATFORM="win32-x64-msvc"
    ASSET_EXT=".exe"
    ;;
  *)
    echo "zzop bootstrap: unsupported platform $(uname -s) — zzop-mcp ships for macOS, Linux, and Windows." >&2
    exit 0
    ;;
esac

# The installed build's own version, straight from the binary (`zzop-mcp version` prints
# "zzop-mcp <version>"). Empty when nothing is installed yet, or when what is installed cannot run.
installed_version() {
  [ -x "$DEST" ] || return 0
  "$DEST" version 2>/dev/null | awk 'NR == 1 { print $2 }'
}

# The newest published release tag, without a JSON parser: `tag_name` appears once in the payload for
# the latest release. Prints nothing when offline, rate-limited, or otherwise unhappy — every caller
# treats "nothing" as "do not know", never as "no update".
latest_tag() {
  curl -fsSL --max-time 10 "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null \
    | grep -m1 '"tag_name"' \
    | cut -d'"' -f4
}

HAVE="$(installed_version)"
LATEST="$(latest_tag)"

if [ -n "$HAVE" ]; then
  # Already installed. The ONLY job left is to report a newer release if there is one — never to act
  # on it.
  if [ -z "$LATEST" ]; then
    exit 0 # Offline or rate-limited: silence beats a scary line about a check we could not run.
  fi
  if [ "v$HAVE" = "$LATEST" ]; then
    exit 0
  fi
  echo "zzop-mcp $HAVE is installed; $LATEST is available. This plugin does not update it for you —"
  echo "a new analyzer version changes findings and invalidates the analysis cache, so the timing is"
  echo "yours to pick. To take it: delete $DEST and start a new session, or grab it from"
  echo "https://github.com/$REPO/releases."
  exit 0
fi

# Nothing installed (or what is there cannot report a version) — this is the install, so do it.
if [ -z "$LATEST" ]; then
  echo "zzop bootstrap: no zzop-mcp installed and the release lookup failed (offline? proxy?)." >&2
  echo "  Install it by hand from https://github.com/$REPO/releases as $DEST" >&2
  exit 0
fi

URL="https://github.com/$REPO/releases/download/$LATEST/zzop-mcp-$PLATFORM$ASSET_EXT"
TMP="$DEST.download"

mkdir -p "$DEST_DIR" 2>/dev/null

if ! curl -fsSL --max-time 300 -o "$TMP" "$URL" 2>/dev/null; then
  rm -f "$TMP"
  echo "zzop bootstrap: failed to download $URL" >&2
  echo "  Install it by hand from https://github.com/$REPO/releases as $DEST" >&2
  exit 0
fi

# Integrity check against the release's SHA256SUMS, BEFORE the file is ever made executable.
#
# Scope, stated plainly because it is narrow: this catches a truncated or corrupted download, and it
# is a hook for anyone who obtained the digest through another channel. It does NOT defend against a
# compromised release origin — an attacker who can swap the binary can swap SHA256SUMS beside it —
# and TLS already refuses MITM. Closing the origin axis needs a signature rooted outside this release
# page; that is a separate decision, because verifying one needs more than the bash+curl this file
# is allowed to assume (see PLATFORMS above).
#
# A MISSING SHA256SUMS is accepted: releases up to and including v0.29.1 carry no such asset
# (VERSIONING.md's phrasing for the same boundary), later releases do, and refusing to install from
# the old ones would break the hook for every existing release. A PRESENT-but-mismatched digest
# is refused outright — that is the case this exists for, and it must never degrade to a warning.
#
# Every path through this block that does NOT end in a verdict says so out loud. Until 2026-08-10
# only the no-hasher branch did; a missing SHA256SUMS and a sums file without our asset's entry both
# skipped the whole check in silence, indistinguishable from "verified" to anyone auditing the
# install. A non-verdict is not a failure (the install proceeds either way), but it must never be
# mistaken for a check that ran.
if sums="$(curl -fsSL --max-time 60 "https://github.com/$REPO/releases/download/$LATEST/SHA256SUMS" 2>/dev/null)"; then
  asset="zzop-mcp-$PLATFORM$ASSET_EXT"
  expected="$(printf '%s\n' "$sums" | awk -v a="$asset" '{ n = $2; sub(/^\.\//, "", n); if (n == a) { print $1; exit } }')"
  if [ -n "$expected" ]; then
    actual=""
    if command -v sha256sum >/dev/null 2>&1; then
      actual="$(sha256sum "$TMP" | awk '{print $1}')"
    elif command -v shasum >/dev/null 2>&1; then
      actual="$(shasum -a 256 "$TMP" | awk '{print $1}')"
    fi
    # No hasher on this machine is a NON-verdict, not a pass: say so rather than implying a check ran.
    if [ -z "$actual" ]; then
      echo "zzop bootstrap: no sha256sum/shasum available; the download was NOT verified." >&2
    elif [ "$actual" != "$expected" ]; then
      rm -f "$TMP"
      echo "zzop bootstrap: REFUSED $URL -- sha256 does not match the release's SHA256SUMS." >&2
      echo "  expected $expected" >&2
      echo "  actual   $actual" >&2
      echo "  Nothing was installed. Re-run to retry, or install by hand from" >&2
      echo "  https://github.com/$REPO/releases after checking the release page." >&2
      exit 0
    fi
  else
    echo "zzop bootstrap: checksum entry for $asset not found in $LATEST's SHA256SUMS; the download was NOT verified." >&2
  fi
else
  echo "zzop bootstrap: $LATEST publishes no SHA256SUMS (or fetching it failed); the download was NOT verified." >&2
fi

chmod +x "$TMP" 2>/dev/null

# Move last, so an interrupted download never leaves a half-written file where the MCP server config
# expects an executable.
if ! mv -f "$TMP" "$DEST" 2>/dev/null; then
  rm -f "$TMP"
  echo "zzop bootstrap: could not place the binary at $DEST" >&2
  exit 0
fi

# The restart line below is not decoration — it is the difference between "installed" and "working".
# A client fixes its MCP tool list when the session starts, and this hook runs inside that startup:
# on the session that FIRST downloads the binary, `mcpServers.zzop` is registered correctly and the
# server is healthy, yet no zzop tool is listed, because the list was settled before this download
# finished. Measured 2026-07-25 on Windows: the hook ran, fetched the binary, and every tool
# answered — but only after a restart. Without this line the honest reading of that session is
# "I installed the plugin and it does nothing", which is where a first-time user leaves. Every later
# session takes the already-installed branch above and never prints it.
echo "zzop-mcp $LATEST installed at $DEST (first run of this plugin)."
echo "Restart Claude Code once to load the zzop tools: this session's tool list was fixed before the"
echo "download finished, so they appear from the next session on."
exit 0
