# react-query adapter (Mode B overlay example)

A reference **adapter** that makes react-query v3's positional-key idiom visible to zzop's cross-layer
analysis — without adding any react-query-specific vocabulary to the engine.

## The problem it solves

A common react-query v3 wiring registers ONE default `queryFn` on the client, and every read call is
just a cache key:

```js
// main.jsx — the default queryFn, wired once
const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      queryFn: ({ queryKey }) => axios.get(queryKey[0], { params: queryKey[1] }).then((res) => res.data),
    },
  },
})

// ...callers everywhere:
useQuery('/tags')
useQuery(`/articles/${slug}`)
useQuery([`/articles${filters.feed ? '/feed' : ''}`, { limit: 10, ...omit(filters, ['feed']) }])
```

There is no `fetch(...)`/`axios.*` call site at any of these call sites — the route is data passed to a
cache-key hook, resolved into a real HTTP request only inside the one shared `queryFn`. zzop's native
egress extractor recognizes `fetch`/`axios`/`ky` call sites, but a `useQuery(...)` call is not one, so
the whole GET surface of a react-query app wired this way is invisible to the cross-layer join — not
"0 findings" but "extracted nothing" for every read.

## The approach

The queryKey-is-a-route convention is app-specific (react-query itself has no opinion on what a key
means), so — per zzop's direction — it's resolved by an **injected adapter**, not by teaching the engine
react-query's shape:

1. Walk the frontend tree for `.ts`/`.tsx`/`.js`/`.jsx`/`.mjs` files.
2. Lexically match `useQuery(`/`useInfiniteQuery(` call sites whose first queryKey argument (optionally
   wrapped in an array literal, `useQuery([key, vars])`) is a string or template literal.
3. Normalize the extracted literal to the SAME key shape `zzop_core::http_consume_interface_key`
   produces on the native consume side: drop everything from a `?`/`#`, root-normalize a
   leading-slash-less literal, collapse duplicate slashes, drop a trailing slash. Template
   `${...}` interpolations collapse to `{}` — zzop's own route-param placeholder — so
   `` `/articles/${slug}` `` joins a backend's `/articles/:slug` the same way a direct `axios.get`
   call would.
4. Emit each call site as an `IoConsume` fact, grouped per file into a Normalized-AST envelope.
5. Feed that envelope to zzop via the **`adapterOverlays`** config field
   ([Mode B overlay](../../docs/NORMALIZED_AST.md)), which the engine merges on top of native analysis.

This is "Mode B": the overlay *augments* native analysis (unlike `analyzeEnvelope`, which *replaces*
it). The engine stays framework-neutral; the queryKey-is-a-route convention lives only in this adapter.
The emitted HTTP method is a flag, not something the call site carries — the default `queryFn`'s verb
(almost always `GET`, since it's the *read* hook) is app-specific, so the adapter takes it as `--method`
rather than guessing.

## Usage

```sh
node adapter.mjs --root <frontend-root> [--source web] [--hooks useQuery,useInfiniteQuery] [--method GET] > overlay.json
```

Pass the resulting envelope through your zzop config's `adapterOverlays` array (a field on the
`analyze` / `analyzeTrees` request):

```js
const overlay = JSON.parse(execSync(`node adapter.mjs --root fe --source web`));
const out = JSON.parse(native.analyzeTrees(JSON.stringify({
  trees: [
    { root: 'fe', sourceId: 'web', adapterOverlays: [overlay] },
    { root: 'be', sourceId: 'api' },   // native NestJS/Express/etc. provides, no overlay needed
  ],
})));
```

The `packages/engine/examples/cross_layer_rule_counts.rs` measurement harness used for the worked
result below reads the same overlay shape from `ZZOP_OVERLAYS` (`sourceId=path.json` entries, `;`
separated) rather than requiring a bespoke Rust harness per adapter:

```sh
node examples/react-query-adapter/adapter.mjs --root <fe-root> --source fe-vite > overlay.json
ZZOP_OVERLAYS="fe-vite=overlay.json" \
  cargo run --release -p zzop-engine --example cross_layer_rule_counts -- <fe-root> <be-root>
```

## Worked result (react-vite RealWorld frontend)

Against a react-query-v3-wired RealWorld frontend (7 `useQuery` read call sites keyed entirely through
`queryKey`) paired with its NestJS backend (routes served under `/api`):

| | keyed HTTP `GET` consumes in the frontend | `unprovidedConsumes` | `route-near-miss` findings | `crossLayerFindings` total |
|---|---|---|---|---|
| native only (no overlay) | 0 | 12 | 10 | 56 |
| with the consume overlay | 7 | 19 (+7) | 15 (+5) | 61 (+5) |

The adapter recovers all 7 previously-invisible reads:

```
GET /tags                          src/components/PopularTags.jsx:5
GET /user                          src/hooks/useUserQuery.js:4
GET /articles/{}                   src/hooks/useArticleQuery.js:7
GET /profiles/{}                   src/hooks/useProfileQuery.js:7
GET /articles/{}/comments          src/hooks/useArticleCommentsQuery.js:7
GET /articles/{}/comments/{}       src/hooks/useArticleCommentQuery.js:7
GET /articles{}                    src/hooks/useArticlesQuery.js:5
```

None of the 7 land as an exact cross-layer edge (`edges` stays 0 before and after) — the backend
serves everything under a `/api` prefix the frontend's default `queryFn` does not know about, so every
overlay-sourced consume is structurally one segment short, mirroring the direct-`axios`-call sites the
same pair already misses. 5 of the 7 are then explained by `cross-layer/route-near-miss` as exactly
that: "missing path prefix (`/api`)" against a real backend route. The remaining 2 land as plain
`unprovidedConsumes` with no near-miss explanation, both for reasons this adapter documents as
limitations rather than bugs:

- `GET /articles/{}/comments/{}` (a "get one comment" read) has no matching backend route at all,
  prefix or not — the near-miss rule correctly finds nothing to explain it with.
- `GET /articles{}` is the feed-ternary call site (`` `/articles${filters.feed ? '/feed' : ''}` ``)
  collapsed to one key with no branch fan-out (see Limitations below). The real backend serves this as
  TWO separate routes (`GET /api/articles` and `GET /api/articles/feed`); the single collapsed key is
  a segment-distance of more than one from either, so it neither exact-joins nor near-miss-explains.

## Limitations (a production adapter can go further)

- Call detection is lexical and single-line (one call per matched line), the same class of limitation
  as the [wrapper adapter](../wrapper-adapter/).
- Template `${...}` interpolation collapses to a single `{}` with no nesting support and — critically —
  **no ternary/conditional fan-out**: `` `/articles${filters.feed ? '/feed' : ''}` `` becomes the ONE
  key `/articles{}`, not the two real keys `/articles` and `/articles/feed`. A richer adapter could
  enumerate literal ternary branches the way zzop's own native `cond-literal-fanout-v1` extraction does
  for direct egress calls.
- Only the react-query v3 **positional**-key idiom is covered (`useQuery('/x')` /
  `useQuery(['/x', vars])`). The object-form idiom (`useQuery({ queryKey: ['/x'], queryFn })`, common in
  v4/v5's `useQuery(options)` signature) is not matched.
- The emitted HTTP method is a flag applied uniformly to every matched call (default `GET`) — react-query
  carries no verb at the call site; the verb is whatever the app's shared `queryFn` actually does, which
  is app-specific and outside what a lexical scan of the call site can determine.
- A leading-interpolation literal (`` `${base}/foo` ``) or a literal containing `://` or whitespace is
  skipped, never guessed, per zzop's "only emit from visible literals" convention.
