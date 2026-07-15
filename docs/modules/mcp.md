# `zzop-mcp`

The Node-free host: one self-contained binary that runs the zzop analysis engine with no Node.js
runtime at all. Where [`zzop-napi`](napi.md) is the Node binding (a `.node` addon plus a JS loader
package), `zzop-mcp` is the other side of the same `zzop-facade` contract — it calls
`analyze_json`/`analyze_trees_json`/`validate_envelope_only_json` (`crates/facade/src/lib.rs`) directly,
with no napi and no JS in between. It serves two front ends over that one engine call path:

- an **MCP server** over stdio (`zzop-mcp mcp`) — for MCP clients (`.mcp.json` pointing at this
  executable);
- a **CLI** (`zzop-mcp analyze <path>` / `zzop-mcp cross <path>...`) — for direct terminal/CI use, no
  MCP client required.

Both share the exact same handlers (`packages/mcp/src/tools.rs`), so a CLI run and an MCP `tools/call`
against the same path produce the same analysis through the same code.

## Module map

| Module | Responsibility |
|---|---|
| `main.rs` | Thin argument dispatch: `analyze` / `cross` / `mcp` subcommands over the library below. |
| `server.rs` | The stdio JSON-RPC 2.0 loop (`initialize`, `tools/*`, `resources/*`). |
| `tools.rs` | MCP tool definitions (`tools/list`) + handlers (`tools/call`), reused by the CLI subcommands. |
| `resources.rs` | MCP resource handlers (`resources/list`, `resources/read`) over the embedded authoring contracts. |
| `embedded.rs` | The embedded contract documents themselves — compiled into the binary via `include_str!`. |
| `output.rs` | Tool-output shaping: full counts, capped lists, explicit truncation disclosure (see [Output contract](#output-contract) below). |

The config front-end (`zzop.config.jsonc` discovery, JSONC parsing, config→request mapping, `trees:
"auto"` workspace expansion) is **not** a module in this crate — it lives in the shared `zzop-config`
crate (`crates/config`), so a future full-CLI binary built on this same host would map configs
identically. `zzop-mcp`'s own `Cargo.toml` depends on `zzop-facade` (the engine call path) and
`zzop-config` (the config front-end) and nothing else beyond `serde_json`.

## CLI surface

```
zzop-mcp analyze <path>                  # analyze ONE repo/tree, print a JSON findings summary
zzop-mcp cross <path>...                 # analyze 2+ trees, print the cross-layer join (paths mode)
zzop-mcp cross --config <zzop.config.jsonc>  # same, but the config's `trees` define the join
zzop-mcp mcp                             # the MCP server over stdio (newline-delimited JSON-RPC 2.0)
```

`analyze`/`cross` print pretty-printed JSON to stdout on success (exit `0`); a failure prints
`zzop-mcp: <message>` to stderr and exits `1`. A missing/malformed argument (no `<path>`, `cross
--config` with no path following it) exits `2` with a usage line. Both subcommands run the unfiltered
default view (no `severity`/`rule`/`limit` narrowing — that's an MCP-tool-only argument surface today).

## MCP surface

### Tools (`tools/list` / `tools/call`)

| Tool | Purpose |
|---|---|
| `analyze_repo` | Analyze ONE repo/tree path. |
| `cross_repo` | Analyze 2+ repos/trees and join them across the cross-layer (kind, key) boundary — zzop's headline capability (e.g. a frontend `fetch` call matched against a backend route, a shared DB table, route drift). |
| `validate_envelope` | Validate a Normalized AST envelope against the v1 contract WITHOUT running an analysis — the authoring feedback loop. Returns `{valid, issues[]}`; never fails on bad input (same contract as `zzop-napi`'s `validateEnvelopeOnly` — see [napi.md](napi.md#validation-only-validateenvelopeonly)). |

`analyze_repo` and `cross_repo` share three optional drill-down arguments, described in
[Output contract](#output-contract) below: `severity` (`"critical" | "warning" | "info"`, minimum
severity to include in the findings *list* — counts always cover everything), `rule` (exact rule id),
and `limit` (list cap, default 50, max 1000).

`analyze_repo({ path })` auto-discovers `<path>/zzop.config.jsonc` (see
[Config semantics](#config-semantics) below); the reply's `config` field says whether one was honored
(a path string) or not (`null`, zero-config defaults applied). A config that declares multiple trees is
a guided error telling the caller to use `cross_repo` with `configPath` instead, or to point
`analyze_repo` at one tree root directly.

`cross_repo` takes **either** `configPath` (a `zzop.config.jsonc` — its `trees`, including `"auto"`,
define the join; the config-first way) **or** `paths` (2+ explicit tree roots; config-free — each tree
tagged by its directory name). Passing both, or neither, is a named argument error. See
[Config semantics](#config-semantics) for what paths mode discloses.

Tool-level failures (bad path, malformed envelope config, an unknown severity value) come back as a
normal MCP result with `isError: true` and a `zzop error: <message>` text block — the MCP convention.
Protocol-level errors (malformed JSON-RPC) are the JSON-RPC error responses described below, not this
channel.

### Protocol errors (stdio transport)

The server (`server.rs`) is silent-failure-free by policy: every line it reads either gets a reply or is
a spec-legal notification (a parsed object with no `id`) — a line it cannot even parse must never be
swallowed, or the client is left hanging on a reply that never comes. Two cases are answered at the
transport level, before dispatch reaches `tools/call`/`resources/read`:

- A line that isn't valid JSON at all answers JSON-RPC error `-32700` (Parse error) with `id: null` —
  the spec's reserved shape for "the request id itself is unrecoverable."
- A line that parses but isn't a single JSON object (e.g. a batch array — unsupported by this server, and
  unused by MCP clients) answers `-32600` (Invalid Request).

Both also log one line to stderr. An unknown `method` (anything other than `initialize`/`tools/list`/
`tools/call`/`resources/list`/`resources/read`) answers `-32601` (Method not found) with the request's
own `id`. None of this is the `isError: true` tool-result channel described above — that channel is only
for a named tool call that ran and failed; these are protocol-level responses for input the server
couldn't even dispatch.

### Resources (`resources/list` / `resources/read`)

Seven embedded authoring contracts, addressed as `zzop://contract/<name>` — the documents a custom-parser
or DSL-rule author needs, with no zzop source checkout and no Node required, since they are compiled
into the binary (`embedded.rs`, `include_str!` over the repo's own committed public docs, ~105KB total):

| `<name>` | Content |
|---|---|
| `envelope-schema` | JSON Schema (draft-07) for the Normalized AST envelope v1 — machine-validate a custom parser's output. |
| `envelope-guide` | The Normalized AST envelope contract: Mode A (full envelope) / Mode B (overlay) adapter authoring, field semantics, worked examples (`docs/NORMALIZED_AST.md`). |
| `key-normalization-fixture` | Byte-pinned HTTP key-normalization fixture — the exact `(method, path)` → join-key rows an adapter must reproduce for cross-layer joins. |
| `adapter-guide` | Adapter authoring README: key-normalization parity rules, schema/versioning policy, adapter-kit pointers (`docs/adapters/README.md`). |
| `dsl-reference` | DSL rule-pack reference: pack/rule fields and all four matchers (`docs/rules/dsl-reference.md`). |
| `dsl-authoring-guide` | DSL rule authoring guide: placement, a worked example, testing conventions (`docs/rules/authoring-guide.md`). |
| `example-envelope` | Minimal valid Mode-A envelope example (a crude JSP parser's output). |

`resources/list` returns every entry above (in this order) with its `uri`/`name`/`description`/
`mimeType`; `resources/read` returns the full text verbatim. Deterministic: same binary, same list, same
bytes every time. An unknown `uri` is a named error listing every valid resource — an agent should never
have to guess the name.

## Config semantics

All config handling is delegated to the shared `zzop-config` crate (`crates/config`) — the same Rust
port of the JS CLI's config front-end (`config.js` discovery + `mapper.js` mapping + the napi wrapper's
`withDefaults` layer) that a future full-CLI Rust binary would reuse. Three things to know about how it
behaves here specifically:

- **Auto-discovery.** `analyze_repo`/`zzop-mcp analyze <path>` look for `<path>/zzop.config.jsonc`
  (literal filename, no ancestor walk — same rule as the JS CLI). If present, it is parsed (JSONC:
  comments and trailing commas allowed) and mapped exactly like the JS CLI's `mapper.js` would map it.
- **Zero-config defaults.** Unlike the JS CLI (which errors without a config file), a missing config
  produces the *same request an empty `{}` config would produce* — an MCP tool pointed at any repo must
  still work. This still gets the full default treatment: the bundled DSL rule packs (embedded at
  compile time by `zzop-config`'s `build.rs` and injected as inline `packDefs` — see
  [napi.md](napi.md#defaults-zero-config--full-analysis)'s note on this field) and default `git: {}`
  collection (30-day recency) both apply, exactly as the JS wrapper's `withDefaults` would inject them.
- **`configPath` / paths-mode disclosure (`cross_repo`).** Config-first mode (`configPath`) loads that
  file directly (or a directory containing `zzop.config.jsonc`) and requires it to declare `trees` (2+,
  or `"auto"`) — a single-tree config there is a guided error pointing at `analyze_repo` instead.
  Paths mode (`paths`) builds one zero-config tree request per path, tagged `sourceId` = that directory's
  name; critically, it does **not** load a `zzop.config.jsonc` sitting inside any of those paths — and it
  says so. Each such path adds a `configWarnings` entry (`"<path> contains a zzop.config.jsonc that
  paths mode does NOT load — pass configPath to honor it"`) rather than silently ignoring it, or
  silently loading it and surprising the caller with rules/overlays/mounts they never asked this call to
  apply.
- **Path resolution deviates from the JS CLI in one documented way.** The JS CLI leaves `root`/
  `cacheDir`/`packsDir` as literal strings for the analyzing process's cwd to resolve — which, in normal
  CLI use, *is* the config file's own directory. A server host's cwd is meaningless (an MCP client can
  invoke this binary from anywhere), so `zzop-config` resolves these path-ish config values against the
  **config file's own directory** instead. Overlay paths are the one exception and keep JS parity
  (resolved relative to each tree's own root, matching the JS mapper).

Every reply from `analyze_repo`/`cross_repo` carries `config` (the config file path honored, or `null`)
and `configWarnings` (the config front-end's own non-fatal notes — unknown keys, a skipped/unreadable
overlay, an `"auto"`-expansion report, the paths-mode disclosure above) as a channel **separate** from
the engine's own `warnings` — two different honesty channels, never merged into one.

## Output contract

Every tool reply is summary-first: full counts ride along unconditionally, and any list that gets capped
says so explicitly — this is the token-bomb guard for MCP responses (`packages/mcp/src/output.rs`),
built to never lie by omission.

- **Findings** shape to `{total, bySeverity, byRule, shown, truncated?}`. `total`/`bySeverity`/`byRule`
  are always computed over the FULL set — a `severity`/`rule` filter narrows only `shown`, never the
  counts. `shown` is the filtered list, sorted severity-descending with original engine order as the
  stable tiebreak (deterministic — same analysis, byte-identical tool output), capped at `limit`
  (default 50, max 1000). `truncated` (`{shown, totalMatching, hint}`) appears **only** when `shown` is
  incomplete — its absence is itself the "you have everything" signal, so a cap is never silent.
- **Cross-layer edges** (`cross_repo`) get the same treatment via a plain list cap (`edgesTruncated`,
  default cap 200 — edges are small rows, so most joins fit uncapped).
- **`warnings` (engine) and `configWarnings` (config front-end) are never capped** — the honest
  self-report channels outrank brevity, on the theory that a truncated warning list is worse than a long
  one.
- **`disclosure`** — the engine's run-global, pinned silent-failure-class registry (identical every run;
  see [napi.md](napi.md#disclosure--silent-failure-class-registry-run-global) for the full field
  contract) rides through unfiltered on every `analyze_repo`/`cross_repo` reply, the same meta-honesty
  channel `zzop-napi` exposes.

## Build instructions

`zzop-mcp` has no napi dependency and no MSVC/toolchain requirement — it builds under the workspace's
default toolchain on every platform:

```sh
cargo build -p zzop-mcp --release
```

The binary lands at `target/release/zzop-mcp` (`target/release/zzop-mcp.exe` on Windows); drop `--release`
for a debug build at `target/debug/zzop-mcp` during local iteration. `cargo test -p zzop-mcp` runs the
crate's own unit tests (`output.rs`, `resources.rs`) with no addon feature flags to worry about.

## Distribution status

This release ships `zzop-mcp` in-tree only — build it from a source checkout with the command above and
point `.mcp.json` at the resulting path (see [packages/mcp/README.md](../../packages/mcp/README.md) for
a worked example). Prebuilt, per-platform binary releases (mirroring `@zzop/native`'s
`prebuild.yml`-driven npm publish — see [napi.md](napi.md#packaging-layout)) are planned but not part of
this release; there is currently no download/install path other than `cargo build`.

See also: [napi.md](napi.md) (the `zzop-facade` request/response contract this host drives directly, and
the Node binding this host has no dependency on), [../NORMALIZED_AST.md](../NORMALIZED_AST.md) (the
envelope contract behind `validate_envelope`/`envelope-guide`), [../adapters/README.md](../adapters/README.md)
(adapter authoring, mirrored by the `adapter-guide` resource).
