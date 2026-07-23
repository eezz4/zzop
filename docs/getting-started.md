# Getting started

The fastest path from "nothing installed" to a report on your own repo, plus how to read that report and
quiet a false positive. For the full config schema and CLI flag reference, see
[`crates/host/README.md`](../crates/host/README.md) — this page does not duplicate it.

## Install & first run

zzop's primary distribution is two Node-free binaries — `zzop` (the CLI, for the terminal workflow
below) and `zzop-mcp` (the MCP server, for AI-agent clients) — no Node.js, no npm, no Rust toolchain
needed. Get them one of four ways:

- **Download the binaries.** Grab the `zzop-<platform>[.exe]` (CLI) and/or `zzop-mcp-<platform>[.exe]`
  (MCP server) assets for your platform from [GitHub Releases](https://github.com/eezz4/zzop/releases)
  and put them on `PATH`.
- **Claude Code plugin.** `/plugin marketplace add eezz4/zzop`, then `/plugin install zzop@zzop` (the
  plugin's bundled `.mcp.json` runs `zzop-mcp mcp` from `PATH`).
- **Claude Desktop.** One-click `.mcpb` bundle (drag-and-drop install) — see
  [`packages/mcpb/README.md`](../packages/mcpb/README.md).
- **npm.** `npm i -g @zzop/cli` installs the exact same `zzop` binary above, fetched for your platform
  as an npm dependency — identical subcommands (`analyze`/`cross`/`endpoint`/`contract`/`validate-*`),
  identical output; a tiny launcher script is the only Node involvement, no separate JS
  implementation to drift from the native binary. See [`packages/cli/README.md`](../packages/cli/README.md).

```sh
zzop analyze .          # analyzes the current directory and prints a report
zzop cross ./frontend ./backend   # cross-layer join across 2+ trees
```

There is no scaffolding subcommand — write `zzop.config.jsonc` by hand (or copy one from
[`crates/host/README.md`](../crates/host/README.md)) and pass it explicitly:

```sh
zzop cross --config zzop.config.jsonc
```

See [`crates/host/README.md`](../crates/host/README.md) for the full `zzop.config.jsonc` schema
(`roots`/`trees` — including per-tree `mountedAt`/`mounts`/`hosts` connection topology — `packs`, `rules`,
`git`, `cacheDir`, `sizeCap`, `format`, `report`, `failOn`).

## Reading the output

`zzop analyze` prints a single JSON object to stdout — a shaped summary, not a raw dump. It carries
full finding counts by severity and by rule, engine warnings, and a capped findings list (default 50;
truncation is always disclosed). When git signals ran, it also carries a compact `architecture` object
(pain score, top recommendation, top critical files). This is the exact same summary the `analyze_repo`
MCP tool returns — the CLI subcommand and the tool share one handler, so they never disagree.

**Severity.** Every finding is one of three levels:

| Severity | Roughly means |
| --- | --- |
| `critical` | A confirmed correctness/security issue — the kind of thing that should block a merge. |
| `warning` | A likely issue or architectural smell worth a look, not necessarily urgent. |
| `info` | Lower-confidence or advisory — useful context, high volume. |

**Exit codes:**

| Code | Meaning |
| --- | --- |
| `0` | Ran successfully (regardless of what was found). |
| `1` | Analysis/runtime error. |
| `2` | Usage or config error. |

The binary does **not** gate its exit code on finding severity: it is an analysis + summary surface, not
a CI linter. To gate CI, read the JSON counts yourself (e.g. fail the job when `bySeverity.critical > 0`).
The `format`, `report`, and `failOn` config keys are *recognized* (they do not trigger unknown-key
warnings) but nothing acts on them today — no shipped surface renders a terminal report, writes a
report file, or gates its exit code on them; `@zzop/cli` (see
[`packages/cli/README.md`](../packages/cli/README.md)) is the identical native binary, not a separate
presentation layer.

## Suppressing findings

There are four mechanisms, at three different scopes. This section is the one place they're all listed
together — each links to its authoritative doc.

**(a) Inline suppress marker (in code, per line).** Some DSL rules define a `suppress_marker` — a
`//`-comment on the finding's own line, or the line directly above it, silences that one finding. The
marker name is rule-specific; when a rule has one, its `message` tells you what to write. Example (the
n+1 rule's marker is `n+1-ok`):

```ts
const items = list.map(x => db.find(x.id)); // n+1-ok: batched below, false positive
```

Full semantics (lookback window, regex-escaping, which matchers support it) in
[rules/dsl-reference.md](rules/dsl-reference.md#suppress-marker-semantics).

**(b) Config-level (per project, in `zzop.config.jsonc`).** Turn a rule off, override its severity, or
drop it for matching file paths. Keys are matched by exact rule id: a DSL rule's id is the full
`"{pack}/{rule}"` string (e.g. `sql/nplus1`, `sql/race-condition-toctou`), while a native analysis id
is used as-is (e.g. `dead-candidates` — and note some native ids contain a slash of their own, like
`cross-layer/unconsumed-endpoint`; that slash is part of the native id, not a pack prefix):

```jsonc
"rules": {
  "typescript/no-explicit-any": "off",
  "dead-candidates": { "exclude": ["**/app/**/{page,layout,route}.tsx"] }
}
```

(`failOn` is a recognized severity-threshold key but the Node-free binary does not act on it today — see
"Reading the output" above.) Full schema in [`crates/host/README.md`](../crates/host/README.md).

**(c) SDK/embedding-level (per call, when embedding the engine directly).** Callers embedding
`zzop-facade` directly — or driving the engine JSON contract via `zzop`'s subcommands — pass
`suppressions` (finding-level accept-list by rule + path/glob), `disabledRules`, or
`severityOverrides` on the request:

```json
{ "suppressions": [{ "rule": "sql/nplus1", "path": "legacy/" }] }
```

Full field shapes in [modules/mcp.md](modules/mcp.md#the-zzop-facade-json-contract) (see `AnalyzeRequest`).

**(d) Caveat: native cross-layer analyses are disable-only.** The `cross-layer/*` native rules (run
over the cross-layer join, `zzop cross`) have no source line to anchor an inline marker against — silence
one only via `disabledRules`/config `rules` `"off"`, never a comment. See
[modules/mcp.md](modules/mcp.md) for why (no single tree owns a cross-layer finding).

## Where to next

- [ARCHITECTURE.md](ARCHITECTURE.md) — how a tree gets processed: the IR, route/IO extraction, caching, degraded files.
- [modules/mcp.md](modules/mcp.md#the-zzop-facade-json-contract) — embed the engine directly (the `zzop-facade` JSON contract, request/response shapes).
- [rules/authoring-guide.md](rules/authoring-guide.md) — write and ship a new DSL rule pack.
- [NORMALIZED_AST.md](NORMALIZED_AST.md) and [../examples/](../examples/README.md) — extend zzop to a new language or framework via an external parser/adapter.
