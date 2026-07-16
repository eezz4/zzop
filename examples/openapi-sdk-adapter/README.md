# OpenAPI SDK adapter (Mode B overlay example)

Makes a generated OpenAPI SDK client's frontend calls (`getUser(id)` instead of a literal
`fetch('/users/1')`) visible to zzop's cross-layer join, with zero SDK-specific engine vocabulary.
Every mainstream generator names its exported function (or class method) after the spec's
`operationId`, and the spec is almost always committed — so the adapter builds
`operationId -> "METHOD /path"` from the spec, scans the frontend for call sites, and emits each as
an `IoConsume` in a NormalizedEnvelope fed to the engine via `adapterOverlays`. See
[examples/README.md](../README.md) for the Mode A/B overview,
[docs/adapters/README.md](../../docs/adapters/README.md) for key-normalization parity, and
[docs/modules/napi.md](../../docs/modules/napi.md) for the host API that accepts the overlay.

## Run

```sh
# named-import clients (import { getUser } from '@your/sdk')
node adapter.mjs --mode consume --root <fe-root> --spec <openapi.json> --sdk '@your/sdk' --source <treeSourceId>

# generated CLASS-METHOD clients (api.articles.getArticles(...), e.g. swagger-typescript-api)
node adapter.mjs --mode consume --root <fe-root> --spec <openapi.json> --member-calls --source <treeSourceId>

# spec operations as IoProvide (when you have the spec but not the backend tree)
node adapter.mjs --mode provide --spec <openapi.json> --source api

# tests
node --test test/*.test.mjs
```

Attach the stdout envelope to a tree's `adapterOverlays` array on an `analyze`/`analyzeTrees`
request; with a real backend tree present, zzop extracts the provide side natively and only the
consume overlay is needed.

## Contract points

- Keying: consume `key` = `METHOD` + `servers[0].url`'s static path part + the `paths` key
  (OpenAPI's effective-URL rule), `{param}` collapsed to `{}` — byte-identical to the engine's keys
  ([docs/adapters/README.md](../../docs/adapters/README.md)). A templated server path part
  contributes no prefix rather than a guess.
- Named-import shape (default): a call counts only if the name is a value import from the `--sdk`
  specifier AND a spec `operationId`. `type`-only imports excluded; namespace imports and re-exports
  not followed.
- CLASS-METHOD shape (`--member-calls`, default OFF): `.name(` matches when `name` is an
  `operationId` or its lowerFirst transform (`GetArticles` -> `getArticles`); colliding candidates
  are marked ambiguous and skipped (reported on stderr), never guessed. A literal `.` must
  immediately precede the identifier and `(` follow it.
- `--sdk` gate: plain substring pre-filter (npm or local specifier alike), default `@immich/sdk`;
  skipped entirely when `--member-calls` is on and `--sdk` is not passed.
- Envelope: `parser: 'openapi-sdk-adapter/1'`; pass `--source` equal to the attached tree's
  `sourceId` (the engine warns on mismatch). Spec must be JSON — convert YAML once, e.g.
  `npx --yes yaml --json --single < openapi.yml > openapi.json` (`--single` avoids the CLI's
  document-stream array wrapper).

## Measured results

- immich (`web/` calls the backend exclusively via `@immich/sdk`, named-import mode): keyed HTTP
  consumes 0 -> 349 (179 files, 184 operations); cross-layer edges 0 -> 349 vs. the spec-provide
  overlay, 361 vs. the real NestJS `server/` tree with zero unexplained unprovided consumes.
- fe-vue (`mutoe/vue3-realworld-example-app`, class-method client, `--member-calls`): keyed consumes
  0 -> 18 (11 files, 14/19 operations, 0 ambiguous); edges vs. its Express backend 1 -> 19, BE
  `cross-layer/unconsumed-endpoint` findings 19 -> 5. (Run predates the engine's overlay
  source-mismatch warning — pass `--source fe-vue` today to silence it; counts unchanged.)
