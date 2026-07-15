# zzop-mcp

The Node-free way to run zzop: one self-contained binary with no Node.js runtime, no npm install. It
serves an MCP server over stdio for MCP clients, and doubles as a plain CLI for direct/CI use — both
share the same analysis path. Full reference: [docs/modules/mcp.md](../../docs/modules/mcp.md).

This release ships in-tree only — build it from a source checkout (below). Prebuilt per-platform
binaries are planned but not part of this release.

## Build

```sh
cargo build -p zzop-mcp --release
```

The binary lands at `target/release/zzop-mcp` (`target/release/zzop-mcp.exe` on Windows).

## Use as a CLI

```sh
zzop-mcp analyze ./my-repo
zzop-mcp cross ./frontend ./backend
zzop-mcp cross --config ./zzop.config.jsonc
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

## Tools

| Tool | Purpose |
|---|---|
| `analyze_repo` | Analyze one repository/tree path. |
| `cross_repo` | Analyze 2+ repos/trees and join them across the layer boundary (frontend calls matched against backend routes, shared DB tables, route drift). |
| `validate_envelope` | Validate a Normalized AST envelope (a custom parser's output) against the v1 contract, offline. |

Plus a `resources/*` surface exposing seven embedded authoring-contract documents
(`zzop://contract/<name>`) for writing a custom parser adapter or DSL rule pack with nothing but this
binary.

See [docs/modules/mcp.md](../../docs/modules/mcp.md) for the full tool/resource/config reference,
including exact argument shapes, the output-truncation contract, and the config-resolution deviation
from the JS CLI.
