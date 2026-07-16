# rust-parser-adapter — Mode A external parser (runnable reference)

A lexical Rust parser that projects a whole Cargo workspace into one complete `NormalizedEnvelope`
and runs it through zzop's Mode A entry point, `analyzeEnvelope` — the engine learns zero Rust
vocabulary; every language-neutral analysis (dep graph, symbol rules, coverage census) runs on the
projected channels. Contract: [`docs/NORMALIZED_AST.md`](../../docs/NORMALIZED_AST.md); authoring
guide: [`docs/adapters/README.md`](../../docs/adapters/README.md).

## Run

```sh
node adapter.mjs --root <workspaceRoot> [--source <id>] > envelope.json
node analyze.mjs envelope.json
```

`analyze.mjs` uses `@zzop/native` when installed and falls back to the in-checkout addon
(`packages/native/index.js`) — inside the zzop repo it runs with no install.

## Contract points

| Channel | Projection |
| --- | --- |
| `symbols` | top-level `fn`/`struct`/`enum`/`trait`/`type`/`const`/`static`; `exported` = any `pub` form |
| `imports` | `mod foo;` + `use` paths + inline qualified paths (`zzop_git::collect(...)`), resolved through the module tree to repo-relative paths — the contract joins specifiers against the envelope's path set EXACTLY, so unresolvable paths (std, registry crates) are omitted, never guessed |
| `is_entry` | crate roots, cargo-convention files (`tests/**`, `src/bin/**`, ...), explicit manifest targets — loaded by cargo with zero in-repo importers |
| `io` | empty on purpose — module graph only; a Rust web service would project axum/actix routes here |

A `use` naming an item at a crate root falls back to the root file as a `type_only` edge: real
fan-in, excluded from cycle detection (Rust has no module load order, so the approximated edge would
manufacture a `root <-> module` 2-cycle per root-item import). Lexical limitations are listed in
`adapter.mjs`'s header comment.

## Measured result

As of 2026-07-16, on zzop's own workspace (`--root ../.. --source zzop`): 218 files, 5188 symbols,
607 import edges, findings = 6 circular (genuine sibling-module coupling; `dead-candidates`/
`unreachable` 0 — every zero-fan-in file is a correctly exempted cargo-convention entry).
