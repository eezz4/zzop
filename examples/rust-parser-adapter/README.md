# rust-parser-adapter — a Mode A external parser (worked reference)

A lexical **Rust** parser that projects a whole Cargo workspace into one `NormalizedEnvelope` and runs
it through zzop's Mode A entry point, `analyzeEnvelope`. This is the first *runnable* Mode A reference
(the JSP example is a hand-written fixture) — and the demo below is zzop analyzing **its own Rust
workspace** with it.

Mode A is how a language the engine has no crate for enters zzop: an out-of-process producer emits the
full envelope, and every language-neutral analysis runs on whatever channels it filled —
dep graph → `circular` / `dead-candidates` / `unreachable` / fan-in-out / folder rollups,
symbols → symbol-scan DSL rules, plus the coverage census and config diagnostics. The engine learns
zero Rust vocabulary.

## What it projects (lexical, line-based — no real Rust parse)

| Channel | From |
| --- | --- |
| `symbols` | top-level `fn`/`struct`/`enum`/`trait`/`type`/`const`/`static`; `exported` = any `pub` form |
| `imports` | `mod foo;` → child file edge; `use` paths and **inline qualified paths** (`zzop_git::collect(...)` — Rust 2018 needs no `use`) for `crate::`/`super::`/`self::` and cross-crate workspace names, resolved through the module tree (`foo.rs` / `foo/mod.rs`) to repo-relative paths |
| `is_entry` | crate roots and cargo-convention files (`lib.rs`, `main.rs`, `build.rs`, `tests/**`, `benches/**`, `examples/**`, `src/bin/**`) **plus explicit manifest targets** (`[[test]] path = "..."`) — cargo loads all of these with zero in-repo importers |
| `io` | left empty on purpose — this adapter models the module graph; a Rust web service would project its axum/actix routes here the same way |

Two projection choices matter for signal quality:

- A `use` that names an **item at a crate root** (`use crate::RuleRegistry`) falls back to the root
  *file* as a `type_only` edge: real fan-in, excluded from cycle detection. Rust has no module load
  order, so counting the approximated edge as a cycle edge would manufacture a `root <-> module`
  2-cycle out of every root-item import.
- Inline qualified paths are scanned in addition to `use` lines — without that, a crate referenced
  only as `zzop_git::collect(...)` reads as an unreachable island.

## Usage

```sh
node adapter.mjs --root <workspaceRoot> [--source <id>]   > envelope.json   # envelope to stdout
node analyze.mjs envelope.json                                              # analyzeEnvelope round trip
```

`analyze.mjs` uses `@zzop/native` when installed and falls back to the in-checkout addon
(`packages/napi/index.js`), so inside the zzop repo it runs with no install.

## Worked result — zzop analyzing itself

```
$ node adapter.mjs --root ../.. --source zzop > envelope.json
[rust-parser-adapter] 191 file(s), 3878 symbol(s), 834 import binding(s), 14 crate(s)
$ node analyze.mjs envelope.json
files:        191
symbols:      3878
import edges: 541
findings:     6
  - circular: 6
```

- `dead-candidates: 0` and `unreachable: 0` — every zero-fan-in file in this workspace is a
  cargo-convention entry, correctly exempted via `is_entry`.
- `circular: 6` — genuine sibling-module mutual-reference components (e.g.
  `analyze/{compose,diagnostics,mod}.rs`, `file_routes/*`, `metrics/scores/*`). Rust tolerates module
  cycles, so on a Rust tree read `circular` as a **coupling signal**, not an error.

**This example caught a real engine gap on first contact**: Mode A used to drop `FileProjection.
is_entry` (the Mode B overlay path honored it; `analyze_envelope` passed an empty entry set), so all
51 convention-loaded entry files above read as dead. Fixed in the same change — the self-analysis
numbers before/after: `dead-candidates` 51 → 0.

## Limitations (intentional — a real adapter can go further)

Detection is lexical and line-based: items inside `macro_rules!` or `cfg`-gated blocks are
approximated; multi-line `use` groups (a `{` spanning newlines) are missed (single-line groups are
handled); `#[path = ...]` module overrides and `include!` are not modeled; `pub use` re-export chains
are not followed; string/comment contents can produce spurious inline-path matches. Unresolvable paths
(std, external registry crates) are dropped — external, never guessed.
