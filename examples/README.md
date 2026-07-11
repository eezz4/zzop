# examples — extending zzop

zzop's engine stays language- and framework-neutral. Anything specific — a new language, a framework's
routing convention, a generated SDK — enters through one of two authoring modes, both speaking the same
Normalized-AST contract ([`docs/NORMALIZED_AST.md`](../docs/NORMALIZED_AST.md)). These are worked,
runnable references for each.

## Mode A — bundle (a full envelope for a new language)

An out-of-process parser for a language the engine has no in-workspace crate for (Rust, JSP, Python, ...)
emits a complete `NormalizedEnvelope` that *replaces* native analysis for that source (`analyzeEnvelope`).

- [`rust-parser-adapter/`](rust-parser-adapter/) — a runnable lexical Rust parser: projects a whole
  Cargo workspace (module-tree `use`/`mod` resolution → dep edges, `pub` items → symbols,
  cargo-convention entries → `is_entry`) and round-trips it through `analyzeEnvelope`. Worked result:
  zzop analyzing its own workspace — 191 files, 3878 symbols, 541 import edges,
  `dead-candidates`/`unreachable` 0 (entries correctly exempt), 6 genuine module-coupling cycles.
- [`jsp-envelope.example.json`](jsp-envelope.example.json) — a hand-written, crude-parser-shaped JSP
  envelope: symbols with no body spans, one `http` provide + one `db-table` consume, no imports. It
  validates cleanly against the contract and is the fixture behind `zzop-core`'s
  `normalized::tests::jsp_contract_example_validates`.

## Mode B — overlay (an adapter on top of a language the engine already parses)

An adapter emits a *partial* envelope (usually just `io` + a few `FileProjection`s) that the engine
merges *on top of* native analysis via the `adapterOverlays` config field. The engine learns nothing new;
the framework/SDK knowledge lives only in the adapter.

- [`openapi-sdk-adapter/`](openapi-sdk-adapter/) — makes a generated OpenAPI SDK client's frontend calls
  visible to the cross-layer join, resolved from the committed spec (`operationId → "METHOD /path"`);
  covers both named-import (function) and, via opt-in `--member-calls`, generated CLASS-METHOD clients
  (`api.articles.getArticles(...)`). Worked result on immich: `web/` SDK consumes 0 → 349, cross-layer
  edges 0 → 349. Worked result on fe-vue's class-method client: consumes 0 → 18, edges 1 → 19.
- [`svelte-adapter/`](svelte-adapter/) — completes the dependency graph for a Svelte/SvelteKit frontend:
  fills `.svelte` importer fan-in and exempts SvelteKit entry files via `is_entry`. Worked result on
  immich: `dead-candidates` 204 → 48.
- [`wrapper-adapter/`](wrapper-adapter/) — makes a hand-rolled HTTP client wrapper's calls
  (`requests.get('/articles')`, a superagent/got-style helper) visible to the cross-layer join, the
  broadest opaque-client class (not a generated artifact). Worked result on a RealWorld-shaped fixture:
  cross-layer edges 0 → 4, backend routes reported unconsumed 4 → 0.
- [`react-query-adapter/`](react-query-adapter/) — makes react-query v3's positional-key idiom
  (`useQuery('/tags')`, the route hidden inside a cache key rather than any recognizable HTTP call)
  visible to the cross-layer join. Worked result on a react-vite RealWorld frontend: keyed `GET`
  consumes 0 → 7, `unprovidedConsumes` 12 → 19 with 5 of those correctly explained by
  `cross-layer/route-near-miss` as a missing `/api` prefix.

Each adapter is a reference, not a product dependency — copy it, point it at your repo, and pipe its
stdout envelope into your zzop config's `adapterOverlays` array. See each folder's `README.md` for the
approach, usage, measured result, and known limitations.
