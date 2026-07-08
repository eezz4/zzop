# OpenAPI SDK adapter (Mode B overlay example)

A reference **adapter** that makes a generated OpenAPI SDK client's frontend calls visible to zzop's
cross-layer analysis — without adding any SDK-specific vocabulary to the engine.

## The problem it solves

When a frontend talks to its backend only through a generated SDK client, it never writes a literal
route:

```ts
import { getActivities, deleteActivity } from '@immich/sdk';
await getActivities({ albumId });   // which HTTP route is this? invisible to a static egress scan
```

zzop's native egress extractor recognizes `fetch(...)` / `axios.*` / `ky.*` and a few generated-client
runtimes, but it cannot see a route buried inside a generated function. So for an SDK-only frontend the
FE→BE cross-layer join is blind.

## The approach

Every mainstream OpenAPI generator names each exported function after the spec's `operationId`, and the
OpenAPI spec is almost always committed. So an **injected adapter** can resolve the calls entirely from
data that is already on disk — the engine never needs to learn the SDK's shape:

1. Read the OpenAPI spec → build `operationId → "METHOD /path"` (normalized the same way zzop keys
   routes: `{id}` path params → `{}`).
2. Scan the frontend for value imports from the SDK package and their call sites.
3. Emit each call site as an `IoConsume` fact, grouped per file into a Normalized-AST envelope.
4. Feed that envelope to zzop via the **`adapterOverlays`** config field ([Mode B overlay](../../docs/NORMALIZED_AST.md)),
   which the engine merges on top of native TypeScript analysis.

This is "Mode B": the overlay *augments* native analysis (unlike `analyzeEnvelope`, which *replaces* it).
The engine stays framework/vendor-neutral; the SDK knowledge lives only in this adapter. See
[../../docs/NORMALIZED_AST.md](../../docs/NORMALIZED_AST.md) for the overlay contract.

## Usage

```sh
# FE call sites -> IoConsume overlay
node adapter.mjs --mode consume --root <frontend-root> --spec <openapi.json> --sdk '@your/sdk'

# (optional) spec operations -> IoProvide overlay, for when you have the spec but not the backend tree
node adapter.mjs --mode provide --spec <openapi.json> --source api
```

Each writes a `NormalizedEnvelope` JSON to stdout. Pass it through your zzop config's `adapterOverlays`
array (a field on the `analyze` / `analyzeTrees` request):

```js
const consume = JSON.parse(execSync(`node adapter.mjs --mode consume --root web --spec spec.json`));
const out = JSON.parse(native.analyzeTrees(JSON.stringify({
  trees: [
    { root: 'web', sourceId: 'web', adapterOverlays: [consume] },
    { root: 'server', sourceId: 'api' },   // native NestJS/Hono/etc. provides, no overlay needed
  ],
})));
```

When you have the real backend tree, zzop extracts its route *provides* natively — you only need the
*consume* overlay. The `provide` mode is a convenience for demoing the join from the spec alone.

## Worked result (immich)

Against immich's `web/` frontend (which calls the backend exclusively through `@immich/sdk`) and its
committed `immich-openapi-specs.json`:

| | keyed HTTP consumes in `web/` | cross-layer edges |
|---|---|---|
| native only (no overlay) | 0 | 0 |
| with the consume overlay | 349 (179 files, 184 operations) | 349 |

Every one of the 349 SDK call sites that was invisible to the native scan becomes a first-class,
route-keyed cross-layer edge — resolved purely from the committed spec + call sites, with zero
SDK-specific code in the engine.

## Limitations (a production adapter can go further)

- Named-import call sites only (`import { getUser } from '<sdk>'`); namespace imports
  (`import * as sdk`) and re-exports are not followed.
- `type`-only imports are excluded (they are not calls).
- Call detection is lexical (`name(`), which is safe here because the name must also be a spec
  `operationId`.
