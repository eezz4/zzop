# Auth overlay adapter (Mode B overlay example)

A reference adapter — a demo of zzop's generic entity-attribute injection channel
(`zzop_core::AttributeStore`, fed via a [Mode B overlay](../../docs/NORMALIZED_AST.md)). It completes
`mutating-route-no-auth`'s middleware blind spot: that rule's call-graph BFS walks FROM a mutating
route's handler looking for a guard-named callee, but a router-level `app.use('/admin', requireAuth)`
guards every route mounted under it at registration time — a wiring fact, not a call edge — so a route
guarded only this way false-positives. Per zzop's direction, such per-project middleware conventions
are completed by injection rather than native framework modeling: this adapter regex-matches that one
concrete shape and injects `auth-guarded` PathScope attributes that compose with (never replace) the
native BFS. Common Express guard registrations are now recognized NATIVELY too (see "Native coverage"
under Contract points below) — this adapter remains the reference for a Hono-style backend and for any
Express project whose guard shapes/naming fall outside the native vocabulary.

## Run

```sh
node adapter.mjs --root <root> > auth.json   # emit the overlay envelope
node --test test/adapter.test.mjs            # snapshot test
```

Feed the envelope in via the `overlays` key in `zzop.config.jsonc` (per-tree or top-level), then run
the binary:

```sh
node adapter.mjs --root . > auth.json     # emit the overlay envelope
zzop-mcp cross --config zzop.config.jsonc  # tree carries overlays: ["./auth.json"]
```

Embedders calling `zzop-facade` directly instead pass the parsed envelope on the request's
`adapterOverlays` field (of `analyze_json` / `analyze_trees_json`).

## Contract points

**Native coverage:** the native TypeScript parser
(`parser/parser-typescript/src/adapters/router_mounts.rs`) now recognizes the common Express shapes this
adapter was written for directly — `app`/`router.use(guard)`, `.use('<prefix>', guard)`, and a
route-level guard argument — against its own guard-name vocabulary (`auth`/`guard`/`verify`/`jwt`/
`token`/`permission`/`loggedin`/`api-key`/`(has|can|check|require)access`, plus well-known callees like
`passport.authenticate`), emitting the same `auth-guarded` attribute this adapter injects, without
requiring any overlay at all. For a plain Express project using that vocabulary, this adapter is no
longer necessary. It remains the reference for: Hono and any other non-Express framework (the native
recognizer's `.use` scope is Express-only), a project's own custom guard naming the native vocabulary
doesn't cover (e.g. `mustBeStaff`), and shapes the native producer deliberately skips for precision (see
its own module doc) — plus a template for anyone writing a similar overlay for a framework zzop doesn't
cover at all yet. One approximation both native and this adapter share: registration ORDER isn't
modeled — a `.use` guard is treated as covering its scope regardless of where it sits relative to the
route, so a mutating route registered before its guarding `.use` call still reads as guarded.

- Recognized shape (only this one): `app.use('<prefix>', <guard>)` / `router.use("<prefix>", <guard>)`
  in `.ts`/`.js` files — a literal string prefix plus a bare-identifier guard whose name matches
  `auth|guard|requireAuth|isAuthenticated` (case-insensitive). Computed/templated prefixes, inline
  arrow-function guards, prefix-less `.use(guard)`, nested sub-router mounts, and custom guard names
  outside the vocabulary are not recognized.
- Injected attribute: `{ target: { pathScope: { prefix } }, key: "auth-guarded", value: true }` — one
  per matched prefix, sorted by prefix; a file with no matching registration is omitted entirely.
  PathScope only; it does not resolve exact per-route `IoKey`s.
- Envelope fields: `parser: "auth-overlay-adapter/1"`, `source: "backend"` (aligned with the tree's
  `sourceId` above), per-file `loc` + `attributes` via adapter-kit's `EnvelopeBuilder`.
- Consumption keying: `mutating-route-no-auth` checks `route_attr(kind, key, "auth-guarded")` BEFORE
  its BFS — an exact `IoKey` match wins over the longest-matching `PathScope`, an exempted route never
  enters the BFS, and every uncovered route is still checked natively. The same channel also carries
  `bound-model`/`model-churn` Symbol attributes for `zzop_rules_schema::usage`.

## Results

`node --test test/adapter.test.mjs` passes (1/1 snapshot test), and the engine e2e
(`crates/engine/tests/analyze_attribute_injection.rs`) shows an injected `auth-guarded` `/admin`
PathScope suppressing `mutating-route-no-auth` from 1 finding to 0 on the same route shape. The native
middleware-recognition e2e (`crates/engine/tests/analyze_native_middleware.rs`) covers the same
suppression WITHOUT an overlay, for the Express shapes native recognition now covers directly.
