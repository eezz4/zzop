# Packaging — Node-free native binary for Claude Desktop (MCPB) and Claude Code (plugin)

These packaging lanes (MCPB + Claude Code plugin) ship the native `zzop-mcp` server binary specifically
(no Node runtime, no per-server Node tax) — the same binary, packaged two ways. (zzop also ships a
second native binary, the `zzop` CLI, distributed separately via GitHub Releases and npm; see the repo
root [`README.md`](../README.md).) Discovery + version updates are handled by each host's own
mechanism — there is **no custom version-check hook** and no npm.

## Claude Desktop — MCPB bundle (`mcpb/manifest.json`)

Claude Desktop installs a native MCP server from a `.mcpb` file (formerly `.dxt`; a zip of the server
binary + `manifest.json`). Binary server type → no Node/Docker. `${__dirname}` resolves to the unpacked
extension dir; Desktop auto-appends `.exe` on Windows.

**One `.mcpb` per platform** (matching the 5 prebuild targets) — each bundles that platform's single
binary at `bin/zzop-mcp`, so the manifest stays arch-unambiguous (no `platform_overrides` needed):

```
zzop-mcp-<platform>.mcpb   (zip)
├── manifest.json       (this dir's file, with `version` stamped from the release tag)
└── bin/zzop-mcp[.exe]  (the prebuilt binary for that platform)
```

Build per target (in prebuild CI, after `cargo build -p zzop-mcp --release --target <triple>`):

```sh
# stamp version, stage the binary, zip, and structurally validate
mkdir -p out/bin && cp target/<triple>/release/zzop-mcp[.exe] out/bin/
jq --arg v "$VERSION" '.version=$v' packages/mcpb/manifest.json > out/manifest.json
npx -y @anthropic-ai/mcpb validate out/manifest.json   # build-time only; not a runtime dep
(cd out && zip -r ../zzop-mcp-<platform>.mcpb manifest.json bin)
```

Attach the `.mcpb` files to the GitHub release alongside the bare binaries.

> **NOT YET LIVE-VALIDATED.** The manifest is authored to the MCPB v0.3 spec but has not been installed
> into a real Claude Desktop. Before trusting it: `mcpb validate`, then install one `.mcpb` and confirm
> the server starts + tools list. The per-platform-vs-universal bundle choice is settled as per-platform
> here; revisit only if Desktop's install UX makes a universal bundle clearly better.

## Claude Code — plugin (`.claude-plugin/`, `mcpServers` in `plugin.json`)

Claude Code installs plugins from a marketplace (zzop's own `.claude-plugin/marketplace.json`).
`${CLAUDE_PLUGIN_ROOT}` resolves in `plugin.json`'s `mcpServers` server `command` (verified — distinct
from the SessionStart-hook `CLAUDE_PLUGIN_ROOT` gap), and `/reload-plugins` swaps to a new version on
update, so the marketplace handles versioning with no custom hook.

**Install model — PATH binary (deliberately not one-click).** Code plugins have no `platform_overrides`
equivalent, so a single `mcpServers` `command` can't select a per-OS binary from a bundle. A one-click
selection hook (bundle all binaries + a hook that copies the right one into place) was considered and
**dropped as over-engineering**: it carries real cross-platform-shell + hook-timing risk for a
convenience, and the subtraction philosophy favors removing it. Instead the user installs the single
static `zzop-mcp` binary for their platform from GitHub Releases onto `PATH` (documented in
`marketplace.json`), and `plugin.json`'s `mcpServers` invokes bare `zzop-mcp` — Node-free, works on every
platform, no fragile hook. Desktop's MCPB gives the one-click path for the less-technical audience; Code
users are developers who can drop a binary on `PATH`.
