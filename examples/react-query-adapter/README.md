# react-query adapter (Mode B overlay example)

## What it does

A reference adapter that makes react-query v3's positional-key idiom visible to zzop's cross-layer
analysis. Apps wired with one default `queryFn` carry their HTTP routes as cache keys —
`useQuery('/tags')`, `` useQuery(`/articles/${slug}`) `` — with no `fetch`/`axios` call site anywhere,
so zzop's native egress extractor extracts nothing and the app's whole GET surface goes silent. This
adapter lexically matches `useQuery(`/`useInfiniteQuery(` call sites whose first queryKey element is a
string/template literal and projects each as an `IoConsume` fact in a
[Mode B overlay](../../docs/NORMALIZED_AST.md) envelope (`adapterOverlays`), keeping the
queryKey-is-a-route convention out of the engine. Overlay/key-parity background:
[docs/adapters/README.md](../../docs/adapters/README.md).

## Run

```sh
# emit the overlay, then pass it via the analyze/analyzeTrees `adapterOverlays` array
node adapter.mjs --root <frontend-root> [--source web] [--hooks useQuery,useInfiniteQuery] [--method GET] > overlay.json

# tests
node --test test/adapter.test.mjs

# measurement harness (reads `sourceId=path.json` pairs, `;`-separated, from ZZOP_OVERLAYS)
node adapter.mjs --root <fe-root> --source fe-vite > overlay.json
ZZOP_OVERLAYS="fe-vite=overlay.json" \
  cargo run --release -p zzop-engine --example cross_layer_rule_counts -- <fe-root> <be-root>
```

## Contract points

- Envelope: `zzop-normalized-ast` v1, `parser: react-query-adapter/1`; one `FileProjection` per
  matching file; facts as `io.consumes` entries `{ kind: 'http', key, file, line }`
  ([schema](../../docs/adapters/envelope.schema.json)).
- Keying: adapter-kit's `resolveConsumeKey`, byte-identical to
  `zzop_core::http_consume_interface_key` — template `${...}` and `:param` segments collapse to `{}`,
  `?query`/`#fragment` drops, a no-leading-`/` literal resolves as base-relative (axios/ky `baseURL`
  idiom), an `http(s)://` literal keys verbatim as an external consume. Non-literal keys are skipped
  (stderr `skipped` count), never guessed.
- Method: the `--method` flag (default `GET`) applied uniformly — the verb lives in the app's shared
  `queryFn`, not at the call site, so it is supplied, never guessed.
- Detection is lexical and single-line; v3 positional idiom only (object-form
  `useQuery({ queryKey })` is not matched); ternary interpolation collapses to one `{}` key with no
  branch fan-out.

## Measured result

As of 2026-07-16, engine v0.16.0: the overlay recovers all 7 previously-invisible `GET` consumes on
the react-vite RealWorld frontend and surfaces its missing `/api` prefix — downstream finding counts
are engine-version-dependent; re-run the harness above for current numbers.
