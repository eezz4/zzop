# zzop ( Zero Zone Of Pain )

[![CI](https://github.com/eezz4/zzop/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/eezz4/zzop/actions/workflows/ci.yml)
[![npm version](https://img.shields.io/npm/v/%40zzop%2Fcli)](https://www.npmjs.com/package/@zzop/cli)
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
runs. That determinism is what makes zzop usable as a CI gate — fail a PR on contract drift with
`failOn` — and as a substrate an agent can re-run and diff without chasing flaky rechecks.

- Documentation site: <https://eezz4.github.io/zzop/> (source in [`site/`](site/))
- Documentation (in-repo): [`docs/README.md`](docs/README.md)
- External parser protocol: [`docs/NORMALIZED_AST.md`](docs/NORMALIZED_AST.md)

## Quick start

Run zzop as a CLI — write a `zzop.config.jsonc`, run `npx zzop`, ESLint-style. The `@zzop/cli` package
depends on `@zzop/native`, which auto-installs the right prebuilt platform binary (nothing to
compile). Requires Node.js >= 18.

```sh
npm i -D @zzop/cli     # add to your project (or one-off: npx @zzop/cli)
npx zzop init     # writes an annotated zzop.config.jsonc
npx zzop          # analyzes using that config and prints a report
```

`@zzop/cli` and `@zzop/native` are published on npm; each tagged release drives the published version. See
[`packages/cli/README.md`](packages/cli/README.md) for the full CLI and config reference.

To embed the engine instead of running the CLI, depend on `@zzop/native` directly and call it
JSON-in / JSON-out:

```js
import zzop from '@zzop/native';

const report = JSON.parse(zzop.analyze(JSON.stringify({ root: '.' })));
```

### Use in Claude Code (MCP plugin)

Prefer no Node install at all? `zzop-mcp` is a self-contained binary with an MCP server built in:

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
| Python (`.py, .pyi`) | Native, full AST (ruff, Python 3 — Python-2-only syntax falls back to lexical): symbols, imports, FastAPI route provides, `requests`/`httpx` consumes — v1 scope |
| Rust (`.rs`) | Native, full AST (syn 2): symbols, imports/`mod` tree (incl. same-workspace crate resolution), axum route provides, `reqwest` consumes — v1 scope |
| Go (`.go`) | Native, full CST (tree-sitter-go 0.25): symbols, imports/dep graph (`go.mod` module resolution, package-directory-wide edges), gin + `net/http` route provides (cross-file mount composition — a function-parameter router mounted from another file's call site — incl. Go 1.22 `"METHOD /path"` mux syntax), `net/http` literal egress consumes — v1 scope |
| Java (`.java`) | Native, full CST (tree-sitter-java 0.23.5, Java 21 grammar): symbols (incl. nested types, dot-qualified method names, real visibility), imports/dep graph (`(package, type)`-indexed resolution, glob package-directory-wide edges), Spring MVC route provides (cross-file `extends`-chain + constant-prefix resolution) — v1 scope |
| Prisma schema (`.prisma`) | Native, lexical schema: models/fields (structural + usage-aware schema rules) + `db-table` provides joining the client-side consumes |
| SQL DDL (`.sql`) | Native, lexical DDL: `CREATE TABLE` → `db-table` provides (migration files light up the db-table channel for MyBatis/JDBC-style stacks) |
| Anything else (Ruby, JSP, ...) | Lexical fallback in-tree (line count + `line-scan` rules only), or first-class support via an external parser adapter conforming to the [Normalized AST protocol](docs/NORMALIZED_AST.md) |

Full precision-tier breakdown — exactly what each native parser extracts, Python's v1 scope note, and
each parser's `zzop --version` fingerprint — in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md#language-support).

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
- `crates/engine` — fused execution pipeline: language dispatch (TS/Prisma/Python/Rust/Go/Java/SQL) → rayon
  per-file parse + per-file rules → AST drop → whole-graph passes; graceful degrade, cache
  consumption, git/scores integration, multi-tree cross-layer join, rule profiling
- `crates/git` — git history collection (single `git log --numstat` pass → per-file stats +
  per-commit sets)
- `crates/cache` — per-file IR/findings cache (content hash + parser fingerprint + ruleset
  fingerprint)
- `crates/facade` — pure-JSON `analyze`/`analyzeTrees`/`analyzeEnvelope`/`validateEnvelopeOnly`/`validateRulePackOnly`/`queryIo`/`version` facade, extracted
  from the N-API crate so every native host (`packages/native`, `packages/mcp`) shares one napi-free
  implementation
- `crates/config` — shared Rust config front end (`zzop.config.jsonc` discovery → JSONC strip →
  config→facade-request mapper → `trees: "auto"` workspace expansion), a Rust port of `packages/cli`'s
  JS config layer, used by `packages/mcp`
- `packages/native` — the single Node↔Rust boundary (`analyze`/`analyzeTrees`/`analyzeEnvelope`/`validateEnvelopeOnly`/`validateRulePackOnly`/`queryIo`/`version`) + npm
  distribution skeleton ([packages/native/README.md](packages/native/README.md))
- `packages/mcp` — `zzop-mcp`, a Node-free host binary: an MCP stdio server plus direct CLI
  subcommands, built on `zzop-config` + `zzop-facade`
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

See [`packages/native/README.md`](packages/native/README.md) for the N-API addon build/toolchain details
(`cargo build -p zzop-napi --release --features addon`).

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
