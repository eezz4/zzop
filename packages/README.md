# zzop CLI + zzop-mcp reference

The Node-free way to run zzop: two self-contained binaries with no Node.js runtime, no npm install —
`zzop` (a plain CLI for direct/CI use) and `zzop-mcp` (an MCP server over stdio for MCP clients). They
are two separate PACKAGES in this directory, each building exactly one bin — `packages/cli-bin` →
`zzop`, `packages/mcp` → `zzop-mcp` — and each a thin entrance: both call the same shared
`zzop-summary` library (`crates/summary`) for every answer, which is what keeps a CLI query and an MCP
tool call on the same analysis path. Full reference: [docs/modules/mcp.md](../docs/modules/mcp.md).

The two remaining entries here ship no Rust: `packages/cli` is the `@zzop/cli` npm shim (a zero-logic
wrapper that spawns the native `zzop` binary) and `packages/mcpb` is the Claude Desktop `.mcpb`
manifest.

How to GET the binaries is not restated here: the repo README's
[Quick start](../README.md#quick-start) is the canonical install-lane list for repo readers. Building
from a source checkout (below) remains an option.

## Build

```sh
cargo build -p zzop-cli-bin -p zzop-mcp --release   # builds BOTH bins: `zzop` and `zzop-mcp`
```

The binaries land at `target/release/zzop` and `target/release/zzop-mcp` (`.exe` on Windows).

## Use as a CLI

```sh
zzop analyze ./my-repo
zzop analyze --config ./ci/zzop.config.jsonc  # same analysis, the ONE tree a config elsewhere names
zzop analyze ./my-repo --severity warning --rule sql/nplus1 --limit 20  # narrow the findings LIST only
zzop analyze-envelope ./envelope.json  # Mode A: envelope REPLACES native parsing
zzop validate-envelope ./envelope.json # offline: well-formed? {valid,issues,hints}, exit 0/1 by issues alone
zzop validate-rule-pack ./pack.json    # offline: pack loads + regexes compile? exit 0/1
zzop cross ./frontend ./backend
zzop cross --config ./zzop.config.jsonc
zzop file ./frontend/src/api.ts ./frontend ./backend   # everything zzop knows about ONE file: its tree,
                              # symbols, io, BOTH edge directions, every finding anchored there (uncapped).
                              # Its verdict says whether the file was ANALYZED, not whether it is healthy.
zzop file ./frontend/src/api.ts --config ./zzop.config.jsonc
zzop endpoint users ./frontend ./backend
zzop manifest ./frontend ./backend > contracts.json   # structural contract manifest: identity only, no file/line
zzop diff ./contracts.json ./contracts.new.json       # what MOVED between two runs (bucket transitions first)
zzop coverage ./frontend ./backend                    # how much of each tree zzop can actually SEE: the
                              # per-extension dispatch table, blind spots crossed against each rule's
                              # declared sightline, and the axes this run did NOT measure. The surface
                              # that answers "is 0 findings clean, or blind?". CLI-only.
zzop facts ./frontend ./backend > facts.json          # post-assembly facts (per-tree CommonIr + the whole
                              # join, uncapped) for your own out-of-process rule program. CLI-only: no MCP
                              # twin, because the output is uncapped and grows with the tree.
zzop graph ./frontend ./backend > join.mmd            # the cross-layer join as MERMAID text for an external
zzop graph ./frontend ./backend --scope src/api --top 10   # renderer — scoped, and every cap disclosed
zzop graph . --domain dep --top 40                   # or the FILE import graph: nodes are files, ranked by
                                                     # degree, cycles drawn distinctly. --domain join is default
zzop graph . --domain risk                           # or blast-radius hubs + extraction seams; arrows mean
                                                     # CONTAINMENT there, not imports
zzop graph . --domain posture                        # or the mutating attack surface by guard status
                              # inside the document. The one subcommand that prints text, not JSON.
zzop init                     # write the annotated starter zzop.config.jsonc into the current directory
zzop init --force             # …overwriting an existing one (without --force it refuses, exit 1)
zzop contract                 # list the embedded authoring contracts
zzop contract config-surface  # print one to stdout (raw bytes, pipe-safe)
zzop explain sql/nplus1       # print one bundled DSL rule's compiled-in data (id/pack/severity/message/…)
                              # Also answers for the bare form of a namespaced native id (e.g. god-model)
                              # by naming the full schema/cross-layer id config matches; an io-scan rule's
                              # marker is printed with its native-parse-only condition.
zzop version | --version      # this binary's version (equals the MCP serverInfo.version)
zzop version --verbose        # …plus every parser's fingerprint (which parser build produced an analysis)
zzop help | --help | -h       # the usage line plus one elaboration per subcommand (exit 0)
zzop <subcommand> --help | -h # just that subcommand's own line (exit 0, stdout — never an error)
```

That list mirrors the `USAGE` constant in `packages/cli-bin/src/main.rs`, which `zzop --help` prints and
which is the canonical surface — there is no `mcp` subcommand here. Serving MCP is the sibling binary's
job (below), and a bare `zzop` says so on the error path. The three findings-view knobs
(`--severity`/`--rule`/`--limit`) apply to exactly `analyze`, `analyze-envelope` and `cross` — the same
three tools that declare them over MCP, parsed into the same shared filter.

Prints pretty-printed JSON to stdout — with one deliberate exception, `graph`, whose product is mermaid
text because its consumer is a renderer. A failure prints to stderr and exits non-zero.

`graph` is the join's picture, and it is honest about being one. zzop renders **no pixels**: it emits a
standard `flowchart LR` document and an external renderer (mermaid.js, a chat client, `mmdc`) draws it.
Because a large join makes an unreadable diagram, the surface is scoped by construction — `--top` caps
**drawn relations per bucket** (default 25; call sites for one key/source collapse into one node, so
the cap is never spent on duplicates) and `--scope <prefix>` keeps only rows whose source id or site
path starts with the prefix — and both announce themselves twice: as a `%%` per-bucket
`drawn/inScope/total` census (with the call-site count they aggregate beside it) and as a **visible
note node**, since a mermaid comment does not survive rendering. What the format cannot carry is printed in the document's own header rather than left to
inference: drift **verdicts** (`crossLayerFindings`), `hostRekeyCounts`, the warning/disclosure prose
channels, and per-site file/line (a node aggregates call sites — use `facts` or `cross` for those). Full
contract: [docs/modules/facade.md](../docs/modules/facade.md#the-joins-picture-zzop-graph).

`manifest`/`diff` are the structural-drift pair: commit a manifest next to the code, and a later
`diff` reports what MOVED (a route leaving `edges` for `unprovidedConsumes` is a broken contract, not
just a `-`). They are CLI-only — no MCP tool twin — and `diff` refuses two manifests produced by
different zzop builds unless you pass `--allow-tool-drift`. Full contract:
[docs/modules/facade.md](../docs/modules/facade.md#structural-drift-zzop-manifest--zzop-diff).

## Register as an MCP server

`zzop-mcp` has five argument forms and no analysis subcommands of its own — every analysis reaches it as
an MCP `tools/call`:

```sh
zzop-mcp                      # serve MCP over stdio (the bare form; a plain zzop-mcp on PATH IS the server)
zzop-mcp mcp                  # same, explicitly — the form every registered config below passes in `args`
zzop-mcp version | --version  # this binary's version (equals the MCP serverInfo.version)
zzop-mcp version --verbose    # …plus every parser's fingerprint — the SAME string `zzop version --verbose` prints
zzop-mcp help | --help | -h   # the usage line (exit 0)
```

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

Pass an absolute repo path in each tool call; the target repo's own `zzop.config.jsonc` is
auto-discovered and honored. **That file is required** — an analysis of a tree without one is refused,
with a message naming the `config-template` resource this server serves (the same bytes `zzop init`
writes). Everything the config does not say still defaults: bundled rule packs and git-derived signals
are included either way. The one thing it holds that has no default is the `vocabulary` block — the
names zzop would otherwise guess about that project — where an undeclared key is a judgment zzop does
not make.

`zzop-mcp` is also listed on the official MCP registry (`registry.modelcontextprotocol.io`) as
`io.github.eezz4/zzop` (see [`server.json`](../server.json), published by CI on every release) —
discoverable there by MCP clients/subregistries with no registration step of your own; its entries point
at the same released `.mcpb` bundles the Quick start lane already names, no separate install path. In the committed `server.json` the versions are
release-gated to match the workspace version, but **no entry carries a `fileSha256` at all**: no hash of
a not-yet-built asset can be committed honestly, and a placeholder in that field would be worse than its
absence — the field is schema-shaped, so a client reads any value there as an integrity guarantee. CI
computes the real hash (and re-stamps the version and download URL) from the actual uploaded assets at
release time, and refuses to publish the entry if any package came out of that step without one.

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
| `check_file` | Definitive answer to "what does zzop know about THIS FILE?" — the targeting twin of `check_endpoint` with a file path as the target. Tree, symbols, io facts, dependency edges in BOTH directions, and every finding anchored there (uncapped: a file is bounded, so nothing is dropped). Its sealed verdict (`analyzed` / `lexical-only` / `degraded` / `not-found`) answers whether the file was ANALYZED, not whether it is healthy — an empty findings list means "clean" for the first and "nothing structural ever ran" for the next two. |
| `check_endpoint` | Definitive answer to "is io key X provided/consumed/joined?" — case-insensitive substring match over every io key (http routes, env keys, DB tables, topics), one sealed verdict (`linked` / `provided-only` / `consumed-unprovided` / `external` / `unresolved-only` / `ambiguous` / `mixed` / `not-found`) instead of bucket counts to eyeball. |
| `analyze_envelope` | Run Mode A: a full Normalized AST envelope (a custom parser's output) REPLACES native parsing for this run — contrast `validate_envelope`, which only validates the envelope's shape, and Mode B overlays, which merge external symbols on top of a natively-parsed tree instead of replacing it. |
| `validate_envelope` | Validate a Normalized AST envelope (a custom parser's output) against its contract, offline. |
| `validate_rule_pack` | Validate a DSL rule pack's structure before loading it — the pack loader's own load-time judgments (shape only, never rule-quality semantics), offline. |

Plus a `resources/*` surface exposing the embedded authoring-contract documents
(`zzop://contract/<name>`) for writing a custom parser adapter, a DSL rule pack, or a
`zzop.config.jsonc` with nothing but this binary. The same documents print to a terminal via
`zzop contract [<name>]` — no MCP client required.

See [docs/modules/mcp.md](../docs/modules/mcp.md) for the full tool/resource/config reference,
including exact argument shapes, the output-truncation contract, and the config path-resolution rules
(relative `path`/`configPath` arguments are resolved against the server process's cwd — pass absolute
paths).
