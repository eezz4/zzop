# zpz ( Zero Pain Zone )

A multi-language SAST / architecture-analysis engine, written in Rust. It parses a source tree into a
language-neutral IR, runs a layered rule system (native whole-graph analyses + declarative JSON rule
packs) over it, and returns structural findings, dependency/dead-code analysis, and health scores as
one JSON document.

- Documentation: [`docs/README.md`](docs/README.md)
- External parser protocol: [`docs/NORMALIZED_AST.md`](docs/NORMALIZED_AST.md)

## Layout

- `packages/core` — engine library: Common IR, cross-layer linker, graph analyses, call graph, rule
  DSL interpreter (line/method/symbol/io matchers), unified rule registry + gating
- `packages/metrics` — score channels consumed by `engine`: roi/health/criticality/coupling/
  seams/recommendations/diagnostics
- `packages/engine` — fused execution pipeline: language dispatch (TS/Prisma/Java-lexical) → rayon
  per-file parse + per-file rules → AST drop → whole-graph passes; graceful degrade, cache
  consumption, git/scores integration, multi-tree cross-layer join, rule profiling
- `packages/git` — git history collection (single `git log --numstat` pass → per-file stats +
  per-commit sets)
- `packages/cache` — per-file IR/findings cache (content hash + parser fingerprint + ruleset
  fingerprint)
- `packages/napi` — the single Node↔Rust boundary (`analyze`/`analyzeTrees`/`analyzeEnvelope`/`version`) + npm
  distribution skeleton ([packages/napi/README.md](packages/napi/README.md))
- `parser/` — parser frontends: source → Common IR, including HTTP route extraction (code-registered —
  NestJS-style decorators, Hono/Express router-mount composition across files; file-convention — Next.js
  `pages/api` + app router, Remix flat routes, Medusa-style `src/api`; tRPC procedures) and consume
  resolution (wrapper re-anchoring, `hono/client` typed-RPC) ([parser/README.md](parser/README.md))
- `rules/native/` — whole-graph native rules (`rules-graph`: circular/unreachable/dead-exports/
  duplicate-route, plus the 20 `cross-layer/*` multi-tree rules joining HTTP/DB/tRPC IO facts across
  trees (unconsumed endpoints, method/version drift, external egress, tRPC procedure coverage, ...);
  `rules-schema`: Prisma structural + usage rules) / `rules/dsl/` — declarative DSL
  rule packs (JSON) ([rules/README.md](rules/README.md))

## Build & test

```
cargo test --workspace
cargo clippy --workspace --all-targets   # kept at 0 warnings
cargo fmt --all
```

The N-API addon needs the MSVC toolchain on Windows:

```
cargo +stable-x86_64-pc-windows-msvc build -p zpz-napi --release --features addon
node packages/napi/smoke.mjs
```

Cold/warm benchmark over a real tree:

```
cargo run --release -p zpz-engine --example bench -- <root> --packs rules/dsl --cache <dir> --git
```

Other `packages/engine/examples/` ad hoc harnesses: `cross_layer_rule_counts` (per-`cross-layer/*`-rule
finding counts across 1+ tree roots; set `ZPZ_DUMP_MESSAGES=<n>` to print sample messages),
`dep_graph_export` (exports the file-level dependency graph as Graphviz DOT or Mermaid), and
`fastapi_overlay_adapter` (reference external adapter — a lexical FastAPI/Python router scanner feeding
`EngineConfig::adapter_overlays`, Mode B; see [`docs/NORMALIZED_AST.md`](docs/NORMALIZED_AST.md)'s
"Adapter overlays" section).

Run the English-only source guard (OSS-facing files must be English; Korean is confined to the internal
notes directory, which is gitignored and not part of this repo's published contents):

```
bash scripts/check-english-source.sh
```
