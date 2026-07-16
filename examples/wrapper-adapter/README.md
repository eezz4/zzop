# Wrapper adapter (Mode B overlay example)

## What it does

A reference adapter that makes a hand-rolled HTTP client wrapper's calls visible to zzop's
cross-layer analysis. Many frontends funnel every request through one hand-written helper
(`requests.get('/articles')`, a superagent/got-style wrapper), so the route lives at the wrapper call
site — which zzop's native egress extractor (`fetch`/`axios`/`ky` and a few generated-client
runtimes) does not recognize. The tree goes silent: every backend route reads as unconsumed, and a
reviewer cannot tell a clean tree from a blind one. Unlike a generated SDK (covered by the
[OpenAPI-SDK adapter](../openapi-sdk-adapter/)), a hand-rolled wrapper has no artifact to key from —
this adapter keys the call sites themselves, projecting each `<wrapper>.<verb>(<pathLiteral>)` match
as an `IoConsume` fact in a [Mode B overlay](../../docs/NORMALIZED_AST.md) envelope
(`adapterOverlays`). Overlay/key-parity background:
[docs/adapters/README.md](../../docs/adapters/README.md).

## Run

```sh
# emit the overlay, then pass it via the analyze/analyzeTrees `adapterOverlays` array
node adapter.mjs --root <frontend-root> [--wrapper requests,agent] [--source web] > overlay.json

# tests
node --test test/adapter.test.mjs
```

## Contract points

- Envelope: `zzop-normalized-ast` v1, `parser: wrapper-adapter/1`; one `FileProjection` per matching
  file; facts as `io.consumes` entries `{ kind: 'http', key, file, line }`
  ([schema](../../docs/adapters/envelope.schema.json)).
- Hand-rolled-keying contrast: there is no generated client to read routes from, so only a
  first-argument **string literal** at the call site is keyed — a variable, concatenated, or
  leading-`${...}` path is skipped (stderr `skipped` count), never guessed, and a base URL/prefix the
  wrapper itself prepends (`` `${API_ROOT}${url}` ``) is not folded in.
- Keying: adapter-kit's `resolveConsumeKey`, byte-identical to
  `zzop_core::http_consume_interface_key` — `${...}`/`:param` segments collapse to `{}`,
  `?query`/`#fragment` drops, a no-leading-`/` literal resolves as base-relative (axios/ky `baseURL`
  idiom), an `http(s)://` literal keys verbatim as an external consume.
- Matching: wrapper binding must be a named identifier from the `--wrapper` list (default
  `requests`/`agent`/`api`/`http`/`client`); verbs `get`/`post`/`put`/`patch`/`delete`/`del`/`head`
  (`del` maps to `DELETE`); detection is lexical and single-line.

## Measured result

On a connected RealWorld-shaped fixture (a `requests.*` wrapper calling four routes, a native
backend serving them): cross-layer HTTP edges 0 -> 4 and backend routes reported unconsumed 4 -> 0
with the overlay.
