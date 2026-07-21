# zzop-mcp

The Node-free way to run zzop: one self-contained binary with no Node.js runtime, no npm install. It
serves an MCP server over stdio for MCP clients, and doubles as a plain CLI for direct/CI use — both
share the same analysis path. Full reference: [docs/modules/mcp.md](../../docs/modules/mcp.md).

Prebuilt per-platform binaries (`zzop-mcp-<platform>[.exe]`, 5 platforms) are attached to
[GitHub Releases](https://github.com/eezz4/zzop/releases); building from a source checkout (below) remains an option.

## Build

```sh
cargo build -p zzop-mcp --release
```

The binary lands at `target/release/zzop-mcp` (`target/release/zzop-mcp.exe` on Windows).

## Use as a CLI

```sh
zzop-mcp analyze ./my-repo
zzop-mcp analyze-envelope ./envelope.json  # Mode A: envelope REPLACES native parsing
zzop-mcp validate-envelope ./envelope.json # offline: well-formed? {valid,issues}, exit 0/1
zzop-mcp validate-rule-pack ./pack.json    # offline: pack loads + regexes compile? exit 0/1
zzop-mcp cross ./frontend ./backend
zzop-mcp cross --config ./zzop.config.jsonc
zzop-mcp endpoint users ./frontend ./backend
zzop-mcp contract                 # list the embedded authoring contracts
zzop-mcp contract config-surface  # print one to stdout (raw bytes, pipe-safe)
```

Prints pretty-printed JSON to stdout; a failure prints to stderr and exits non-zero.

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

## Install as a Claude Code plugin

The repo doubles as a self-hosted plugin marketplace (`.claude-plugin/marketplace.json` +
`.mcp.json` bundle the MCP server):

1. Put the binary on `PATH` **under the name `zzop-mcp`** — the bundled `.mcp.json` invokes exactly
   that command. Prebuilt assets on [GitHub Releases](https://github.com/eezz4/zzop/releases) are
   named `zzop-mcp-<platform>[.exe]` (5 platforms, single static binary, no Node needed): download
   yours and rename/link it to `zzop-mcp` (`zzop-mcp.exe` on Windows). Building from source (see
   Build above) needs the same rename of the `target/release` artifact.
2. In Claude Code: `/plugin marketplace add eezz4/zzop`, then `/plugin install zzop@zzop`.

The bundled `.mcp.json` invokes `zzop-mcp mcp` from `PATH` — deliberately pre-installed rather than
fetched-on-first-run, so the plugin stays runtime-free and you always know exactly which binary runs.

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
`zzop-mcp contract [<name>]` — no MCP client required.

See [docs/modules/mcp.md](../../docs/modules/mcp.md) for the full tool/resource/config reference,
including exact argument shapes, the output-truncation contract, and the config path-resolution rules
(relative `path`/`configPath` arguments are resolved against the server process's cwd — pass absolute
paths).
