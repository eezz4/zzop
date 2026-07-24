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

chmod +x "$TMP" 2>/dev/null

# Move last, so an interrupted download never leaves a half-written file where the MCP server config
# expects an executable.
if ! mv -f "$TMP" "$DEST" 2>/dev/null; then
  rm -f "$TMP"
  echo "zzop bootstrap: could not place the binary at $DEST" >&2
  exit 0
fi

echo "zzop-mcp $LATEST installed at $DEST (first run of this plugin)."
exit 0
