# Packaging — Node-free native binary for Claude Desktop (MCPB) and Claude Code (plugin)

These packaging lanes (MCPB + Claude Code plugin) ship the native `zzop-mcp` server binary specifically
(no Node runtime, no per-server Node tax) — the same binary, packaged two ways. (zzop also ships a
second native binary, the `zzop` CLI, distributed separately via GitHub Releases and npm; see the repo
root [`README.md`](../../README.md).) Neither lane updates itself: Desktop cannot (see below), and the
Code plugin deliberately reports a newer release rather than applying it. No npm is involved in either.

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

Claude Code installs plugins from a marketplace (zzop's own `.claude-plugin/marketplace.json`). Unlike
the Desktop lane there is no bundle: `plugin.json`'s `mcpServers` command is
`${CLAUDE_PLUGIN_DATA}/zzop-mcp`, and a `SessionStart` hook (`.claude-plugin/hooks/bootstrap.sh`) puts
that file there by downloading the release asset for the running platform. Nothing goes on `PATH`.
The user-facing steps and the update policy are not restated here — they live in
[`crates/host/README.md`](../../crates/host/README.md#install-as-a-claude-code-plugin). This section
covers only why the packaging has that shape.

**Why a hook rather than a bundle.** Code plugins have no `platform_overrides` equivalent, so one
`mcpServers` command string cannot select a per-OS binary out of a bundle — the choice is a hook that
fetches the right asset, or asking the user to place one by hand. Hand placement shipped first and
failed in real use (a stale binary on `PATH` against much newer manifests, with nothing anywhere
pointing at the fix), which is what bought the hook. Its cost is accepted rather than hidden: it is a
POSIX shell script, so on Windows it needs Git Bash — Claude Code's documented shell for command
hooks — and under the PowerShell fallback it does not run. That failure is visible and names the exact
path to drop a binary at, and a plugin marketplace is a git clone anyway, so a Windows user who has
this plugin almost always has Git for Windows' bash. Desktop's MCPB remains the one-click path for the
less-technical audience.
