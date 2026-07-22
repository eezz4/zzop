# zzop ( Zero Zone Of Pain )

[![CI](https://github.com/eezz4/zzop/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/eezz4/zzop/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](./LICENSE)

zzop is built for an AI agent working in one repo — say the frontend — that needs to verify or
understand the other side of a contract (the backend) without reading it whole; a human reviewing the
same cross-repo change is the identical use case. Its core move is a cross-repo join: it parses each
repo into a language-neutral IR, exact-matches frontend `fetch` calls against backend routes across the
repo boundary, and names near-misses (a typo'd path segment, a version drift, a method mismatch) instead
of leaving you to diff two codebases by hand — cutting the read/context cost of confirming the other
side actually agrees. Alongside that cross-layer join it also runs a SAST-style layered rule system
(native whole-graph analyses + declarative JSON rule packs) over each repo individually, returning
structural findings, dependency/dead-code analysis, and health scores as one JSON document.

Every run is deterministic: same code in, same findings out — byte-stable output you can diff between
runs. That determinism is what makes zzop usable as a CI gate — fail a PR on contract drift by reading
the JSON severity counts — and as a substrate an agent can re-run and diff without chasing flaky
rechecks.

- Documentation site: <https://eezz4.github.io/zzop/> (source in [`site/`](site/))
- Documentation (in-repo): [`docs/README.md`](docs/README.md)
- External parser protocol: [`docs/NORMALIZED_AST.md`](docs/NORMALIZED_AST.md)

## Quick start

zzop ships as two Node-free binaries — `zzop` (CLI) and `zzop-mcp` (MCP server) — no Node.js, no npm,
nothing to compile. Get them one of three ways:

- **Download the binaries.** Grab the `zzop-<platform>[.exe]` (CLI) and/or `zzop-mcp-<platform>[.exe]`
  (MCP server) assets for your platform from [GitHub Releases](https://github.com/eezz4/zzop/releases)
  and run them directly.
- **Claude Code plugin.** `/plugin marketplace add eezz4/zzop`, then `/plugin install zzop@zzop` —
  see [Use in Claude Code](#use-in-claude-code-mcp-plugin) below.
- **Claude Desktop.** One-click `.mcpb` bundle (drag-and-drop install) — see
  [packaging/README.md](packaging/README.md).

Write a `zzop.config.jsonc` and run it, ESLint-style:

```sh
zzop analyze .                          # analyze one repo/tree -> JSON findings summary
zzop cross --config zzop.config.jsonc   # cross-layer join, driven by that config
```

See [packages/mcp/README.md](packages/mcp/README.md) for the full CLI and config reference.

To embed the engine instead of running the binary, depend on the `zzop-facade`/`zzop-summary` Rust
crates and call the JSON-in/JSON-out contract directly — or shell out to `zzop-mcp`'s JSON subcommands,
no linkage required:

```rust
let report: serde_json::Value =
    serde_json::from_str(&zzop_facade::analyze_json(r#"{"root":"."}"#)?)?;
```

### Use in Claude Code (MCP plugin)

`zzop-mcp` is a self-contained binary with an MCP server built in:

1. Download the `zzop-mcp-<platform>[.exe]` asset for your platform from [GitHub
   Releases](https://github.com/eezz4/zzop/releases) and put it on `PATH` under the exact name
   `zzop-mcp` (`zzop-mcp.exe` on Windows).
2. In Claude Code: `/plugin marketplace add eezz4/zzop`, then `/plugin install zzop@zzop`.

See [packages/mcp/README.md](packages/mcp/README.md) for the full install/build reference.

### Result (abridged)

Every finding carries a rule id, severity, and a `file:line` location, e.g.:

```json
{
  "findings": [
    {
      "ruleId": "sql/nplus1",
      "severity": "warning",
      "file": "src/routes/orders.ts",
      "line": 42,
      "message": "await on a store/ORM call (`Repository`/`Store`/`prisma`/`db`/`orm`/`tx`/`trx`) verified structurally inside a for/for-of/for-in/while/do-while statement or an array-iteration callback — checked against the parser's projected loop spans, not merely co-occurring with loop syntax somewhere in the same function — N+1 query pattern. Batch the fetch (e.g. `findMany` with an `in` filter) instead of one call per item. Suppress a vetted case with `// n+1-ok`."
    }
  ],
  "scores":             { /* structural subscores, 0-100 */ },
  "health":             { "pain": 12.4, "contributors": [ /* metrics driving the pain score, highest first */ ] },
  "recommendations":    [ /* refactor-first candidates, ROI-ordered */ ],
  "warnings":           [ /* anything this run could not provide */ ]
}
```

`analyzeTrees` (multi-tree) additionally returns `crossLayerFindings` — frontend fetch <-> backend
route joins — which has no single-tree equivalent.

## Supported languages

| Language | Support |
|---|---|
| TypeScript / JavaScript (`.ts, .tsx, .js, .jsx, .mjs, .cjs, .mts, .cts`) | Native, full AST (swc): symbols, imports, calls, HTTP routes/egress |
| Python (`.py, .pyi`) | Native, full AST (ruff, Python 3 — Python-2-only syntax falls back to lexical): symbols, imports, FastAPI route provides, `requests`/`httpx` consumes (module-level calls plus `Session`/`Client`/`AsyncClient` instances) — v1 scope |
| Rust (`.rs`) | Native, full AST (syn 2): symbols, imports/`mod` tree (incl. same-workspace crate resolution), axum route provides, `reqwest` consumes — v1 scope |
| Go (`.go`) | Native, full CST (tree-sitter-go 0.25): symbols, imports/dep graph (`go.mod` module resolution, package-directory-wide edges), gin + `net/http` route provides (cross-file mount composition — a function-parameter router mounted from another file's call site — incl. Go 1.22 `"METHOD /path"` mux syntax), `net/http` literal egress consumes (package free functions plus bound `http.Client` values) — v1 scope |
| Java (`.java`) | Native, full CST (tree-sitter-java 0.23.5, Java 21 grammar): symbols (incl. nested types, dot-qualified method names, real visibility), imports/dep graph (`(package, type)`-indexed resolution, glob package-directory-wide edges), Spring MVC route provides (cross-file `extends`-chain + constant-prefix resolution) — v1 scope |
| C# (`.cs`) | Native, full CST (tree-sitter-c-sharp 0.23.5): symbols (incl. nested types, dot-qualified method names, `public` visibility), imports/dep graph (namespace→files index, `using` package-directory-wide edges), ASP.NET Core route provides (attribute controllers with `[Route("api/[controller]")]` + `[HttpGet]`/… composition, plus same-file Minimal-API `app.MapGet`/`MapGroup`), `HttpClient` literal egress consumes — v1 scope |
| Prisma schema (`.prisma`) | Native, lexical schema: models/fields (structural + usage-aware schema rules) + `db-table` provides joining the client-side consumes |
| SQL DDL (`.sql`) | Native, lexical DDL: `CREATE TABLE` → `db-table` provides (migration files light up the db-table channel for MyBatis/JDBC-style stacks) |
| Anything else (Ruby, JSP, ...) | Lexical fallback in-tree (line count + `line-scan` rules only), or first-class support via an external parser adapter conforming to the [Normalized AST protocol](docs/NORMALIZED_AST.md) |

Full precision-tier breakdown — exactly what each native parser extracts, Python's v1 scope note, and
each parser's `zzop version` fingerprint — in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md#language-support).

A normal-sized file whose extension has no native parser also self-reports in the output's `warnings`
— naming the extension, a file count, and a path sample — instead of vanishing silently; point it at an
adapter (`overlays: [...]` in `zzop.config.jsonc`) if that language matters for the analysis.

## Versioning & stability

zzop is **pre-1.0 (`0.x`) and unstable** — any release may change behavior, output, rules, or
defaults without notice, so pin an exact version (not a `^`/`~` range) and re-test before upgrading.
Semantic Versioning and a maintained changelog begin at `1.0.0`. Full policy:
[VERSIONING.md](VERSIONING.md).

## Layout

- `crates/core` — engine library: Common IR, cross-layer linker, graph analyses, call graph, rule
  DSL interpreter (line/method/symbol/io matchers), unified rule registry + gating
- `crates/metrics` — score channels consumed by `engine`: roi/health/criticality/coupling/
  seams/recommendations/diagnostics
- `crates/engine` — fused execution pipeline: language dispatch (TS/Prisma/Python/Rust/Go/Java/C#/SQL) → rayon
  per-file parse + per-file rules → AST drop → whole-graph passes; graceful degrade, cache
  consumption, git/scores integration, multi-tree cross-layer join, rule profiling
- `crates/git` — git history collection (single `git log --numstat` pass → per-file stats +
  per-commit sets)
- `crates/cache` — per-file IR/findings cache (content hash + parser fingerprint + ruleset
  fingerprint)
- `crates/facade` — pure-JSON `analyze`/`analyzeTrees`/`analyzeEnvelope`/`validateEnvelopeOnly`/`validateRulePackOnly`/`queryIo`/`version` contract
  that `packages/mcp` calls directly — no Node, no native addon in between
- `crates/config` — shared Rust config front end (`zzop.config.jsonc` discovery → JSONC strip →
  config→facade-request mapper → `trees: "auto"` workspace expansion), used by `packages/mcp`
- `packages/mcp` — two Node-free host binaries over one shared lib: `zzop` (CLI subcommands) and
  `zzop-mcp` (MCP stdio server), built on `zzop-config` + `zzop-facade`
- `parser/` — parser frontends: source → Common IR, including HTTP route/consume extraction across
  languages and frameworks ([parser/README.md](parser/README.md))
- `rules/native/` — whole-graph native rules (`rules-graph`, `rules-http`, `rules-cross-layer`, `rules-schema`) plus `rules/dsl/`
  declarative JSON rule packs ([rules/README.md](rules/README.md))

## Development

Contributing? Start with [`CONTRIBUTING.md`](./CONTRIBUTING.md).

### Build & test

```
cargo test --workspace
cargo clippy --workspace --all-targets   # kept at 0 warnings
cargo fmt --all
```

See [`packages/mcp/README.md`](packages/mcp/README.md) for building/running the `zzop-mcp` binary
(`cargo build -p zzop-mcp --release`).

Cold/warm benchmark over a real tree:

```
cargo run --release -p zzop-engine --example bench -- <root> --packs rules/dsl --cache <dir> --git
```

Other `crates/engine/examples/` ad hoc harnesses: `cross_layer_rule_counts` (per-`cross-layer/*`-rule
finding counts across 1+ tree roots; set `ZZOP_DUMP_MESSAGES=<n>` to print sample messages),
`dep_graph_export` (exports the file-level dependency graph as Graphviz DOT or Mermaid), and
`fastapi_overlay_adapter` (reference external adapter — a lexical FastAPI/Python router scanner feeding
`EngineConfig::adapter_overlays`, Mode B; now the reference for what native Python v1 deliberately skips
— non-literal prefixes, Flask/Django, custom conventions — since native FastAPI extraction covers the
common literal shapes directly; also reachable via the `adapterOverlays` config field; see
[`docs/NORMALIZED_AST.md`](docs/NORMALIZED_AST.md)'s "Adapter overlays" section).

Run the English-only source guard (OSS-facing files must be English; Korean is confined to the internal
notes directory, which is gitignored and not part of this repo's published contents):

```
bash scripts/check-english-source.sh
```

## License

MIT — see [`LICENSE`](./LICENSE).
