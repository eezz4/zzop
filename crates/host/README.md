# zzop CLI + zzop-mcp reference

The Node-free way to run zzop: two self-contained binaries with no Node.js runtime, no npm install —
`zzop` (a plain CLI for direct/CI use) and `zzop-mcp` (an MCP server over stdio for MCP clients). They
are two separate PACKAGES, each building exactly one bin — `packages/cli-bin` → `zzop`,
`packages/mcp` → `zzop-mcp` — over this shared library. `zzop-host` (this directory) is **lib-only: it
declares no `[[bin]]` at all**; it is the dispatch both packages call, which is what keeps a CLI query
and an MCP tool call on the same analysis path. Full reference:
[docs/modules/mcp.md](../../docs/modules/mcp.md).

Prebuilt per-platform binaries are attached to GitHub Releases, and there are three other install
lanes — the repo README carries the canonical list for repo readers, so it is not restated here:
[Quick start](../../README.md#quick-start). Building from a source checkout (below) remains an option.

## Build

```sh
cargo build -p zzop-cli-bin -p zzop-mcp --release   # builds BOTH bins: `zzop` and `zzop-mcp`
```

The binaries land at `target/release/zzop` and `target/release/zzop-mcp` (`.exe` on Windows).

## Use as a CLI

```sh
zzop analyze ./my-repo
zzop analyze-envelope ./envelope.json  # Mode A: envelope REPLACES native parsing
zzop validate-envelope ./envelope.json # offline: well-formed? {valid,issues}, exit 0/1
zzop validate-rule-pack ./pack.json    # offline: pack loads + regexes compile? exit 0/1
zzop cross ./frontend ./backend
zzop cross --config ./zzop.config.jsonc
zzop endpoint users ./frontend ./backend
zzop manifest ./frontend ./backend > contracts.json   # structural contract manifest: identity only, no file/line
zzop diff ./contracts.json ./contracts.new.json       # what MOVED between two runs (bucket transitions first)
zzop contract                 # list the embedded authoring contracts
zzop contract config-surface  # print one to stdout (raw bytes, pipe-safe)
zzop explain sql/nplus1       # print one bundled DSL rule's compiled-in data (id/pack/severity/message/…)
                              # Also answers for a schema issue label (e.g. schema/dead-model) by naming
                              # the family gate that disables it; an io-scan rule's marker is printed
                              # with its native-parse-only condition.
```

Prints pretty-printed JSON to stdout; a failure prints to stderr and exits non-zero.

`manifest`/`diff` are the structural-drift pair: commit a manifest next to the code, and a later
`diff` reports what MOVED (a route leaving `edges` for `unprovidedConsumes` is a broken contract, not
just a `-`). They are CLI-only — no MCP tool twin — and `diff` refuses two manifests produced by
different zzop builds unless you pass `--allow-tool-drift`. Full contract:
[docs/modules/facade.md](../../docs/modules/facade.md#structural-drift-zzop-manifest--zzop-diff).

## Register as an MCP server

Point your MCP client at the built binary's `mcp` subcommand. For a `.mcp.json`-style config:

```json
{
  "mcpServers": {
    "zzop": {
      "command": "/absolute/path/to/zzop/target/release/zzop-mcp",
      "args": ["mcp"]
    }
  }
}
```

No further configuration is required — pass an absolute repo path in each tool call. If the target repo
has a `zzop.config.jsonc`, it's auto-discovered and honored; otherwise zero-config defaults apply
(bundled rule packs + git-derived signals included).

`zzop-mcp` is also listed on the official MCP registry (`registry.modelcontextprotocol.io`) as
`io.github.eezz4/zzop` (see [`server.json`](../../server.json), published by CI on every release) —
discoverable there by MCP clients/subregistries with no registration step of your own; it points at the
same `.mcpb` bundles above, no separate install path. In the committed `server.json` the versions are
release-gated to match the workspace version, but each entry's `fileSha256` is a placeholder: CI
recomputes it (and re-stamps the version and download URL) from the actual uploaded assets at release
time, since no hash of a not-yet-built asset can be committed honestly.

## Install as a Claude Code plugin

The repo doubles as a self-hosted plugin marketplace (`.claude-plugin/marketplace.json` +
`.claude-plugin/plugin.json`'s `mcpServers` field declare the MCP server):

1. `/plugin marketplace add eezz4/zzop`
2. `/plugin install zzop@zzop` — note these are two separate steps; adding the marketplace only puts
   the plugin in the catalog.
3. Start a new session. A `SessionStart` hook (`.claude-plugin/hooks/bootstrap.sh`) downloads the
   `zzop-mcp` binary for your platform from [GitHub
   Releases](https://github.com/eezz4/zzop/releases) into the plugin's own data directory, and
   `plugin.json`'s `mcpServers` runs it from there. Nothing to place on `PATH`. **That first session
   will not list the zzop tools** — a session's tool list is settled before the download finishes — so
   restart Claude Code once and they appear; the hook prints this too.

**Updates are reported, never applied.** Once a binary is installed the hook only tells you when a
newer release exists — a new analyzer version changes findings and invalidates the analysis cache, so
when to take it is your call. To take it, delete the binary the hook names and start a new session.

**Windows: the hook needs a POSIX shell.** Claude Code documents Git Bash for shell-form hooks and
falls back to PowerShell when it is absent — and this script cannot run under PowerShell. The script
itself is not Git-Bash-specific (it recognizes MSYS/MinGW/Cygwin alike), but whether Claude Code
discovers some other bash is not documented, so treat Git for Windows as the supported path. It
usually costs nothing: a marketplace IS a git clone, so you already have git, and on Windows that
overwhelmingly means Git for Windows, which installs `bash.exe` next to `git.exe`.

Where it does fail, you get one `hook error` line per session and no binary. It cannot block your
session, but it will not stop nagging either. Two ways out:

- Install [Git for Windows](https://gitforwindows.org/) (this is the one-step fix), **or**
- Disable this plugin and register the server yourself. Download `zzop-mcp-win32-x64-msvc.exe` from
  [Releases](https://github.com/eezz4/zzop/releases) and point a project `.mcp.json` at it:
  `{"mcpServers":{"zzop":{"command":"C:\\path\\to\\zzop-mcp.exe","args":["mcp"]}}}`. Placing the
  binary while leaving the plugin enabled does NOT silence the hook — the hook is what fails, not
  the download.

There is no PowerShell twin of the hook, deliberately. A second hook entry would run on macOS and
Linux too (`shell: "powershell"` is ignored there, so it executes under bash), putting a permanent
error line in every Unix session to serve a narrow Windows corner; the hook schema has no per-OS
condition, and Windows PowerShell 5.1 has no `||` to short-circuit a polyglot one-liner. The Node
wrapper that would close the gap outright is the exact dependency zzop is built to not need.

## Tools

| Tool | Purpose |
|---|---|
| `analyze_repo` | Analyze one repository/tree path. |
| `cross_repo` | Analyze 2+ repos/trees and join them across the layer boundary (frontend calls matched against backend routes, shared DB tables, route drift). |
| `check_endpoint` | Definitive answer to "is io key X provided/consumed/joined?" — case-insensitive substring match over every io key (http routes, env keys, DB tables, topics), one sealed verdict (`linked` / `provided-only` / `consumed-unprovided` / `external` / `unresolved-only` / `ambiguous` / `mixed` / `not-found`) instead of bucket counts to eyeball. |
| `analyze_envelope` | Run Mode A: a full Normalized AST envelope (a custom parser's output) REPLACES native parsing for this run — contrast `validate_envelope`, which only validates the envelope's shape, and Mode B overlays, which merge external symbols on top of a natively-parsed tree instead of replacing it. |
| `validate_envelope` | Validate a Normalized AST envelope (a custom parser's output) against the v1 contract, offline. |
| `validate_rule_pack` | Validate a DSL rule pack's structure before loading it — the pack loader's own load-time judgments (shape only, never rule-quality semantics), offline. |

Plus a `resources/*` surface exposing ten embedded authoring-contract documents
(`zzop://contract/<name>`) for writing a custom parser adapter, a DSL rule pack, or a
`zzop.config.jsonc` with nothing but this binary. The same documents print to a terminal via
`zzop contract [<name>]` — no MCP client required.

See [docs/modules/mcp.md](../../docs/modules/mcp.md) for the full tool/resource/config reference,
including exact argument shapes, the output-truncation contract, and the config path-resolution rules
(relative `path`/`configPath` arguments are resolved against the server process's cwd — pass absolute
paths).
