# Wrapper adapter (Mode B overlay example)

A reference **adapter** that makes a hand-rolled HTTP client wrapper's calls visible to zzop's
cross-layer analysis — without adding any wrapper-specific vocabulary to the engine.

## The problem it solves

Many frontends never call `fetch`/`axios` directly. They funnel every request through one central
hand-written helper (a superagent/got/node-fetch-style wrapper), so the route lives at the *wrapper* call
site, not at a recognizable egress call:

```js
// src/agent.js — the wrapper
const requests = {
  get:  (url)       => superagent.get(`${API_ROOT}${url}`).then(...),
  post: (url, body) => superagent.post(`${API_ROOT}${url}`, body).then(...),
  del:  (url)       => superagent.del(`${API_ROOT}${url}`).then(...),
};
// ...callers everywhere:
requests.get('/articles');
requests.post('/users/login', { user });
requests.get(`/articles/${slug}`);
```

zzop's native egress extractor recognizes `fetch(...)` / `axios.*` / `ky.*` and a few generated-client
runtimes, but not `requests.get('/articles')`. So an SDK-less, wrapper-only frontend extracts **zero**
consumes — and the failure is worse than a wrong edge: the tree goes *silent*. Every backend endpoint the
frontend actually calls reads as **unconsumed** (looks dead), and a reviewing agent cannot tell a genuinely
clean tree from a blind one. This is the broadest opaque-client class — hand-rolled wrappers are not a
generated artifact, so [the OpenAPI-SDK adapter](../openapi-sdk-adapter/) doesn't cover them.

## The approach

The wrapper is app-specific, so — per zzop's direction — it's resolved by an **injected adapter**, not by
teaching the engine the wrapper's shape:

1. Walk the frontend tree for `.ts`/`.tsx`/`.js`/`.jsx`/`.mjs` files.
2. Lexically match `<wrapper>.<verb>(<pathLiteral>)` call sites, where `<wrapper>` is a configurable
   binding name (default `requests`/`agent`/`api`/`http`/`client`) and `<verb>` is an HTTP verb method
   (`get`/`post`/`put`/`patch`/`delete`/`del`/`head`; `del` → `DELETE`).
3. Key each call site as `"METHOD /path"`, normalizing the path exactly the way zzop keys routes so the
   consume can join a native provide: template `${...}` and `:param` segments → `{}`, query strings
   dropped. Only a first-argument **string literal rooted at `/`** is keyed — a path built from a
   variable, or a wrapper-internal `` `${API_ROOT}${url}` `` host expression, is left alone (never guessed).
4. Emit each call site as an `IoConsume` fact, grouped per file into a Normalized-AST envelope.
5. Feed that envelope to zzop via the **`adapterOverlays`** config field
   ([Mode B overlay](../../docs/NORMALIZED_AST.md)), which the engine merges on top of native analysis.

This is "Mode B": the overlay *augments* native analysis (unlike `analyzeEnvelope`, which *replaces* it).
The engine stays framework/vendor-neutral; the wrapper knowledge lives only in this adapter.

## Usage

```sh
node adapter.mjs --root <frontend-root> [--wrapper requests,agent] [--source web] > overlay.json
```

Pass the resulting envelope through your zzop config's `adapterOverlays` array (a field on the
`analyze` / `analyzeTrees` request):

```js
const overlay = JSON.parse(execSync(`node adapter.mjs --root fe --source web`));
const out = JSON.parse(native.analyzeTrees(JSON.stringify({
  trees: [
    { root: 'fe', sourceId: 'web', adapterOverlays: [overlay] },
    { root: 'be', sourceId: 'api' },   // native backend provides, no overlay needed
  ],
})));
```

## Worked result

The motivating case is [RealWorld](https://github.com/gothinkster/react-redux-realworld-example-app)'s
`agent.js` superagent wrapper — where zzop, natively, extracts none of the frontend's calls and reports
every backend route as unconsumed. On a connected frontend/backend fixture in that shape (a `requests.*`
wrapper calling four routes, a native backend serving them):

| | cross-layer http edges | backend routes reported unconsumed |
|---|---|---|
| native only (no overlay) | 0 | 4 |
| with the consume overlay | 4 | 0 |

Every wrapper call site — literal (`GET /articles`, `POST /users/login`) and templated
(`` `/articles/${slug}` `` → `GET /articles/{}`, joining the backend's `:slug` route) — becomes a
first-class, route-keyed cross-layer edge, and the four "dead" backend endpoints are correctly recognized
as consumed. The silence is gone, with zero wrapper-specific code in the engine.

## Limitations (a production adapter can go further)

- **Only the call-site path is keyed — a base URL/prefix the wrapper prepends is not.** If the wrapper
  builds its target as `` `${API_ROOT}${url}` `` and `API_ROOT` carries a path (e.g.
  `https://host/api`), the served route is `/api/articles` but this adapter keys the call site's
  `/articles`, so it will not join a backend that serves the prefixed route. (This is exactly why the
  RealWorld pair is a poor live demo: its `API_ROOT` both points at an external host *and* ends in
  `/api`.) A richer adapter can resolve the wrapper's `API_ROOT` constant one hop — like the
  [FastAPI example](../../packages/engine/examples/fastapi_overlay_adapter.rs) folds a config constant —
  and prepend it.
- The wrapper binding must be a named identifier from the default/`--wrapper` list; a wrapper reached
  through a differently-named local or a member chain (`this.api.get(...)`) is not matched.
- Only a first-argument string-literal path rooted at `/` is keyed. A path stored in a variable, built by
  concatenation, or whose literal starts with a `${...}` host expression is skipped (reported by nothing —
  a richer adapter could resolve one-hop constants, like the FastAPI example does).
- Call detection is lexical and single-line; a call split across lines, or a verb method that is not an
  HTTP verb name, is not seen.
- Path normalization mirrors zzop's route key (`${...}`/`:param` → `{}`, query dropped) but does not model
  matrix params or optional segments.
