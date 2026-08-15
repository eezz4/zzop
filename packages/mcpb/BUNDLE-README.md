# zzop-mcp — the bundle you just unpacked

This `.mcpb` ships `zzop-mcp`, a native (Node-free) MCP server for Claude Desktop that runs
deterministic cross-repo contract analysis: which frontend calls hit which backend endpoints (and
which don't) — exact joins, disclosed blind spots, no guessing. This file is the bundle's own
README; it ships inside every bundle because a bundle is installed and read offline, so every link
here is an absolute URL on purpose.

- Project: https://github.com/eezz4/zzop — site: https://eezz4.github.io/zzop/
- The MCP surface (tools, resources, reply contracts):
  https://github.com/eezz4/zzop/blob/main/docs/modules/mcp.md#mcp-surface
- Questions / issues: https://github.com/eezz4/zzop/issues

## Updates are manual

Claude Desktop will not offer you a newer version of a bundle installed from a downloaded `.mcpb`
file. Check https://github.com/eezz4/zzop/releases and re-install the newest bundle. The binary
softens this itself: past 90 days from the source it was built from, its `initialize` handshake
carries an instructions string saying how old this build is and pointing at the releases page — it
performs no network call and never claims a newer release exists (it cannot know that), and it is
silent on a current build.

## macOS — the binaries are UNSIGNED, and Gatekeeper is expected to block them

Neither darwin binary carries a Developer ID signature and no bundle is notarized: `darwin-arm64`
has only the ad-hoc signature the Apple linker inserts on its own, `darwin-x64` has none at all
(inspected on the v0.32.0 bundles, 2026-08-15). A `.mcpb` downloaded by a browser carries the
quarantine attribute; whether it reaches the extracted binary depends on the extractor (quarantine
propagation is expected, not observed — like the block itself: this project has no macOS hardware,
so this whole paragraph is reasoned from the signature inspection, not from a live install). When it
does propagate, Gatekeeper refuses a quarantined executable without a valid Developer ID signature,
so this bundle is expected NOT to run out of the box. Diagnose first, then strip:

```sh
xattr -p com.apple.quarantine <install-dir>/bin/zzop-mcp   # "No such xattr" = quarantine is not your problem
xattr -dr com.apple.quarantine <install-dir>               # otherwise: strip it once, recursively
```

Signing + notarization is a deliberate non-purchase for now, not an oversight: it costs an Apple
Developer membership plus signing secrets in CI, and this repo's release lane deliberately holds
zero secrets. If the official directory review names signing as a requirement, or a real macOS user
reports the block, that trade gets re-judged. (The Windows exe is also unsigned — SmartScreen may
warn — but Windows has a "run anyway" path, so it is a speed bump, not a wall.)

## Privacy

The policy of record is https://eezz4.github.io/zzop/privacy.html — this section covers only the
`zzop-mcp` server binary this bundle ships, and defers to that page for everything else (including
the network calls the *Claude Code plugin* lane makes, which is not in this bundle).

The server binary makes zero network calls — the workspace declares no HTTP client crate at all (in
a repo checkout, `grep -c reqwest Cargo.lock` = 0, and no other HTTP client is in the lock either).
It reads the files of the trees you point it at, writes its analysis cache into a `.zzop/` directory
(created beside the honored `zzop.config.jsonc` — usually the tree root; a config's `cacheDir` can
move it or, set to `null`, disable it), and sends nothing anywhere: no telemetry, no accounts, no
third-party services. Questions: https://github.com/eezz4/zzop/issues

## What else is in this bundle

`manifest.json` (how Claude Desktop launches the server), `bin/zzop-mcp[.exe]` (the server binary
for exactly one platform — the manifest names which), and `LICENSE` + `THIRD-PARTY-NOTICES.md`
(zzop is MIT; the notices cover the statically linked crates whose licenses require the text to
accompany the binary). How the bundle is assembled lives with the maintainers, not here:
https://github.com/eezz4/zzop/blob/main/packages/mcpb/README.md
