# Auth overlay adapter (Mode B overlay example)

A reference **adapter** that completes `mutating-route-no-auth`'s middleware blind spot for an
Express/Hono-style backend — without teaching the engine any framework's middleware vocabulary
natively.

## The problem it solves

`mutating-route-no-auth` (`zzop_rules_http::mutating_route_no_auth`) flags a mutating (POST/PUT/PATCH/
DELETE) route whose handler, walked via call-graph BFS, never reaches a callee whose name looks like an
auth guard:

```ts
// routes/admin.ts
adminRoutes.delete('/admin/users/:id', deleteUser); // deleteUser's own body never calls anything guard-named
```

But a very common pattern guards routes at the ROUTER level instead, before the handler ever runs:

```ts
// app.ts
app.use('/admin', requireAuth); // every route mounted under /admin is guarded here, not in each handler
app.use(adminRoutes);
```

`requireAuth` is never CALLED FROM `deleteUser` — it's wired in at registration time, one level up — so
it never appears as a call edge the BFS can follow. The route is genuinely guarded, but
`mutating-route-no-auth` has no way to know that from the call graph alone, and false-positives.

## The approach

Per zzop's direction, this environment-specific middleware convention is resolved by an **injected
adapter**, not by growing the engine's native middleware modeling:

1. Walk the tree for `.ts`/`.js` files.
2. Regex-match the common `app.use('<prefix>', <guard>)` / `router.use("<prefix>", <guard>)` shape: a
   literal string prefix followed by a bare identifier guard argument.
3. Keep only registrations whose guard identifier looks auth-shaped (`auth`/`guard`/`requireAuth`/
   `isAuthenticated`, case-insensitive).
4. For every matching file, emit one `FileProjection` whose `attributes` carries one entry per matched
   prefix: `{ target: { pathScope: { prefix } }, key: "auth-guarded", value: true }` — a router-level
   PathScope annotation on the generic entity-attribute channel (`zzop_core::AttributeStore`). A file
   with no matching registration is omitted entirely. Registrations within a file are sorted by prefix
   for deterministic output.
5. Feed the resulting envelope to zzop via the **`adapterOverlays`** config field ([Mode B
   overlay](../../docs/NORMALIZED_AST.md)).

`mutating-route-no-auth` reads this attribute (`AUTH_GUARDED_ATTR`) off the store BEFORE running its
call-graph BFS at all (`route_attr(kind, key, "auth-guarded")`, exact `IoKey` match wins over the
longest-matching `PathScope`) — an exempted route never enters the BFS. The injected `auth-guarded` key
**composes with, and does not replace,** the native BFS: a route with no injected attribute is still
checked the normal way, and only a route actually covered by an injected exemption is cleared. This is
one consumer of the generic attribute channel, not a bespoke auth-only code path — the same channel also
carries `bound-model`/`model-churn` Symbol attributes for `zzop_rules_schema::usage`.

## Usage

```sh
node adapter.mjs --root <root> > auth.json
```

Pass the resulting envelope through your zzop config's `adapterOverlays` array (a field on the
`analyze` / `analyzeTrees` request):

```js
const overlay = JSON.parse(execSync(`node adapter.mjs --root .`));
const out = JSON.parse(native.analyzeTrees(JSON.stringify({
  trees: [
    { root: '.', sourceId: 'backend', adapterOverlays: [overlay] },
  ],
})));
```

Or via `zzop.config` (`overlays: ["./auth.json"]`), if your embedding reads overlays from a config file
rather than passing them inline.

## Limitations (a production adapter can go further)

- Regex-based over raw text, not a real parser — it recognizes exactly one shape: a literal string
  prefix immediately followed by a bare-identifier guard argument (`app.use('/admin', requireAuth)`).
  A computed/templated prefix, an inline arrow-function guard (`app.use('/admin', (c, next) => ...)`),
  or a guard imported and aliased under an unrelated name is not recognized.
- The guard-name vocabulary is a fixed regex (`auth`/`guard`/`requireAuth`/`isAuthenticated`); a
  project's own custom guard name (e.g. `mustBeStaff`) needs the adapter extended or forked.
- Only the router-level `.use(prefix, guard)` two-argument mount shape is matched; a chained
  `.use(guard1, guard2)` (no prefix — applies tree-wide) or a nested sub-router mount isn't followed.
- Emits `PathScope` attributes only; it does not attempt to resolve an exact per-route `IoKey`, so an
  adapter for a project needing route-level (not prefix-level) precision would need its own logic on top
  of this reference shape.
