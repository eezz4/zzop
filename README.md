# zzop ( Zero Zone Of Pain )

A multi-language SAST / architecture-analysis engine, written in Rust. It parses a source tree into a
language-neutral IR, runs a layered rule system (native whole-graph analyses + declarative JSON rule
packs) over it, and returns structural findings, dependency/dead-code analysis, and health scores as
one JSON document.

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
- `parser/` — parser frontends: source → Common IR, including HTTP route/consume extraction across
  languages and frameworks ([parser/README.md](parser/README.md))
- `rules/native/` — whole-graph native rules (`rules-graph`, `rules-http`, `rules-cross-layer`, `rules-schema`) plus `rules/dsl/`
  declarative JSON rule packs ([rules/README.md](rules/README.md))

## Build & test

```
cargo test --workspace
cargo clippy --workspace --all-targets   # kept at 0 warnings
cargo fmt --all
```

See [`packages/napi/README.md`](packages/napi/README.md) for the N-API addon build/toolchain details
(`cargo build -p zzop-napi --release --features addon`).

Cold/warm benchmark over a real tree:

```
cargo run --release -p zzop-engine --example bench -- <root> --packs rules/dsl --cache <dir> --git
```

Other `packages/engine/examples/` ad hoc harnesses: `cross_layer_rule_counts` (per-`cross-layer/*`-rule
finding counts across 1+ tree roots; set `ZZOP_DUMP_MESSAGES=<n>` to print sample messages),
`dep_graph_export` (exports the file-level dependency graph as Graphviz DOT or Mermaid), and
`fastapi_overlay_adapter` (reference external adapter — a lexical FastAPI/Python router scanner feeding
`EngineConfig::adapter_overlays`, Mode B, also reachable via napi's `adapterOverlays` config field; see
[`docs/NORMALIZED_AST.md`](docs/NORMALIZED_AST.md)'s "Adapter overlays" section).

Run the English-only source guard (OSS-facing files must be English; Korean is confined to the internal
notes directory, which is gitignored and not part of this repo's published contents):

```
bash scripts/check-english-source.sh
```
