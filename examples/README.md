# examples — extending zzop

Worked, runnable references for the two authoring modes, both speaking the Normalized-AST contract
([`docs/NORMALIZED_AST.md`](../docs/NORMALIZED_AST.md); authoring guide
[`docs/adapters/README.md`](../docs/adapters/README.md)). Each is a reference, not a product
dependency — copy it, point it at your repo. Approach, usage, measured results, and limitations live
in each folder's own README.

## Mode A — bundle (full envelope for a new language, replaces native analysis via `analyzeEnvelope`)

- [`rust-parser-adapter/`](rust-parser-adapter/) — external parser for a language with no engine
  crate: module-tree import resolution to exact repo-relative paths, cargo-convention `is_entry`,
  `type_only` root-fallback edges.
- [`jsp-envelope.example.json`](jsp-envelope.example.json) — hand-written minimal envelope (no body
  spans, one `http` provide + one `db-table` consume); the fixture behind `zzop-core`'s
  `normalized::tests::jsp_contract_example_validates`.

## Mode B — overlay (partial envelope merged onto native analysis via `adapterOverlays`)

- [`java-imports-adapter/`](java-imports-adapter/) — **start here**: the minimal on-ramp, one
  channel (`imports`) in ~90 lines — built when the v0.16-era lexical Java projector left a
  dep-graph gap (`imports: None`), proving a partial envelope is enough; no parser required. The
  native Java parser has since closed that gap, so on Java trees it now merges as a no-op — kept
  as the reference recipe for any extension still missing a channel.
- [`openapi-sdk-adapter/`](openapi-sdk-adapter/) — generated OpenAPI SDK calls keyed from the
  committed spec (`operationId → "METHOD /path"`), including class-method clients.
- [`oazapfts-adapter/`](oazapfts-adapter/) — the oazapfts generator's call-shape recognizer, ported
  out of the engine (generated-SDK knowledge lives in adapters, not engine vocabulary).
- [`svelte-adapter/`](svelte-adapter/) — `.svelte` importer fan-in plus SvelteKit entry exemption
  via `is_entry`.
- [`wrapper-adapter/`](wrapper-adapter/) — hand-rolled HTTP client wrapper calls keyed for the
  cross-layer join (the broadest opaque-client class).
- [`react-query-adapter/`](react-query-adapter/) — react-query v3 positional cache keys
  (`useQuery('/tags')`) keyed as `GET` consumes.
- [`auth-overlay-adapter/`](auth-overlay-adapter/) — a demo of the entity-ATTRIBUTES injection
  channel: router-level `app.use('/prefix', requireAuth)` guards injected as file attributes to
  close `mutating-route-no-auth`'s middleware blind spot for non-Express frameworks/custom
  middleware naming — common Express guard registrations are now recognized natively (see the
  rule's catalog entry).

## Shared

- [`adapter-kit/`](adapter-kit/) — the walk / envelope-builder / key-normalization library the JS
  adapters import (key normalization byte-identical to `zzop_core`).
