# Packaging — Node-free native binary for Claude Desktop (MCPB) and Claude Code (plugin)

These packaging lanes (MCPB + Claude Code plugin) ship the native `zzop-mcp` server binary specifically
(no Node runtime, no per-server Node tax) — the same binary, packaged two ways. (zzop also ships a
second native binary, the `zzop` CLI, distributed separately via GitHub Releases and npm; see the repo
root [`README.md`](../../README.md).) Neither lane updates itself: Desktop cannot (see below), and the
Code plugin deliberately reports a newer release rather than applying it. No npm is involved in either.

Because Desktop has no notifier of its own for a privately distributed bundle, **the binary reports its
own age**: past 90 days from the source it was built from, `initialize` carries an `instructions` string
(and the stderr banner one extra line) saying how old this build is and pointing at the releases page.
It performs no network call and never claims a newer release exists — it cannot know that — and it is
silent on a current build. Details: [`docs/modules/mcp.md`](../../docs/modules/mcp.md#mcp-surface).

## Claude Desktop — MCPB bundle (`mcpb/manifest.json`)

Claude Desktop installs a native MCP server from a `.mcpb` file (formerly `.dxt`; a zip of the server
binary + `manifest.json`). Binary server type → no Node/Docker. `${__dirname}` resolves to the unpacked
extension dir. The win32 bundles' manifests spell `bin/zzop-mcp.exe` explicitly (stamped at assembly
time, below) — Desktop reportedly auto-appends `.exe` on Windows, but that behavior is undocumented,
so the shipped manifests do not rely on it.

**One `.mcpb` per platform** (matching the 5 prebuild targets) — each bundles that platform's single
binary at `bin/zzop-mcp`, so the manifest stays arch-unambiguous (no `platform_overrides` needed):

```
zzop-mcp-<platform>.mcpb   (zip)
├── manifest.json       (this dir's file, with `version` stamped from the release tag and
│                        `compatibility.platforms` narrowed to that bundle's OS)
└── bin/zzop-mcp[.exe]  (the prebuilt binary for that platform)
```

Build per target (in prebuild CI, after `cargo build -p zzop-mcp --release --target <triple>`).
The executable copy of this recipe is `.github/workflows/prebuild.yml` (the `mcpb` bundling step) —
that file is what actually runs, and this block is here to explain it:

```sh
# 1. stamp the version ONCE, into a shared intermediate
jq --arg v "$VERSION" '.version=$v' packages/mcpb/manifest.json > manifest.stamped.json

# 2. then, per bundle: stage the binary, narrow the OS, spell the staged filename, validate, zip
#    ($exe is ".exe" exactly when the staged binary is bin/zzop-mcp.exe, i.e. the win32 bundles)
mkdir -p out/bin && cp target/<triple>/release/zzop-mcp[.exe] out/bin/
jq --arg os "$os" --arg exe "$exe" '.compatibility.platforms=[$os]
  | .server.entry_point+=$exe | .server.mcp_config.command+=$exe' \
  manifest.stamped.json > out/manifest.json
npx -y @anthropic-ai/mcpb@2 validate out/manifest.json  # build-time only; not a runtime dep
cp LICENSE THIRD-PARTY-NOTICES.md out/                  # license obligations ride INSIDE the bundle
(cd out && zip -r ../zzop-mcp-<platform>.mcpb manifest.json bin LICENSE THIRD-PARTY-NOTICES.md)
```

Four things in that recipe are load-bearing and easy to drop when copying it by hand:

- **The win32 manifests spell `bin/zzop-mcp.exe` — `entry_point` and `mcp_config.command` are
  suffix-stamped per bundle from the staged binary's own filename.** The committed manifest says
  `bin/zzop-mcp`; leaning on Desktop's reported-but-undocumented Windows `.exe` auto-append would
  make an unvalidated lane depend on an unwritten behavior, so the stamp removes the dependency.
  `$exe` comes from the same `case` arm that stages the binary, so manifest and file cannot drift.
- **`compatibility.platforms` is stamped PER BUNDLE, never committed.** Until 2026-08-02 the 5 bundles
  carried a byte-identical manifest under 5 filenames, so Desktop had no grounds to refuse a bundle
  installed on the wrong OS. Stamping the field in the committed `manifest.json` cannot fix that — it
  would have to declare all three OSes and would therefore say nothing. `$os` is the platform name's
  first segment (`win32-x64-msvc` → `win32`, `darwin-arm64` → `darwin`, `linux-x64-gnu` → `linux`), so a
  new build-matrix lane is stamped automatically.
- **This blocks OS mis-installs ONLY, not arch mis-installs** (measured 2026-08-02). The schema's
  vocabulary is that three-name OS enum and nothing else: `darwin-arm64` and `darwin-x64` both collapse
  to `darwin`, both linux lanes to `linux`. A `darwin-x64` bundle installed on an arm64 Mac is not
  refusable through this schema — the arch axis is simply not expressible. Do not read the stamp as a
  full compatibility gate.
- **`mcpb@2` is MAJOR-PINNED, not `@latest`.** The CI job that runs it holds `contents: write`, so
  whatever `npx` fetches there can rewrite the very release assets `SHA256SUMS` was computed over; an
  unreviewed major arriving between releases lands in a lane that has no rehearsal. `@2` keeps every
  2.x, and moves the decision to adopt 3 to a human. Because it validates each SHIPPED manifest (they
  differ per bundle now), it doubles as the enum gate on `$os` — a bad first segment fails here instead
  of shipping.

Attach the `.mcpb` files to the GitHub release alongside the bare binaries.

> **NOT YET LIVE-VALIDATED.** The manifest is authored to the MCPB v0.3 spec and each shipped variant
> passes `mcpb validate`, but no `.mcpb` has been installed into a real Claude Desktop. The win32
> manifests now spell `bin/zzop-mcp.exe` explicitly, so the lane no longer depends on Desktop's
> undocumented Windows `.exe` auto-append — but "Desktop actually launches this bundle" remains
> unverified on every OS. Before trusting it: install one `.mcpb` and confirm the server starts +
> tools list. The per-platform-vs-universal bundle choice is settled as per-platform here; revisit
> only if Desktop's install UX makes a universal bundle clearly better.

## Claude Code — plugin (`.claude-plugin/`, `mcpServers` in `plugin.json`)

Claude Code installs plugins from a marketplace (zzop's own `.claude-plugin/marketplace.json`). Unlike
the Desktop lane there is no bundle: `plugin.json`'s `mcpServers` command is
`${CLAUDE_PLUGIN_DATA}/zzop-mcp`, and a `SessionStart` hook (`.claude-plugin/hooks/bootstrap.sh`) puts
that file there by downloading the release asset for the running platform. Nothing goes on `PATH`.
The user-facing steps and the update policy are not restated here — they live in
[`packages/README.md`](../README.md#install-as-a-claude-code-plugin). This section
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
