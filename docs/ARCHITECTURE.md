# How the engine processes your tree

A short orientation for making sense of the `analyze`/`analyzeTrees` output — full field-by-field
shapes are in [modules/napi.md](modules/napi.md); this page just explains what's actually happening
underneath so the output makes sense.

## The IR your `ir` field contains

Every analyzed file is parsed and projected into a language-neutral intermediate representation
(`CommonIr`): symbols (functions/classes/consts/types/interfaces), import edges, line counts, and
optional `IoFacts` (HTTP/DB/tRPC provide-consume facts used for cross-layer joins). This — never a raw
language AST — is what the `ir` field in the output actually contains. A custom/external parser can
feed the exact same shape in through the Normalized AST protocol; see
[NORMALIZED_AST.md](NORMALIZED_AST.md).

## Route & IO extraction

HTTP `provides` are composed from two sources, merged together: **code-registered** routes
(decorator-based — NestJS-style controllers; router-mount calls — Hono and Express, including
cross-file mounts composed from router fragments; manual pathname dispatch — framework-less
Workers/Node servers comparing `url.pathname` against literals, evidence-gated on URL provenance
plus a Request-typed or -named parameter, with declared Durable-Object class bodies excluded —
see the adapter's own doc for the exact gates and accepted limits) and **file-convention** routes
inferred from the
tree's own layout (Next.js `pages/api` and the app router, Remix flat routes, Medusa-style `src/api`).
tRPC procedures are similarly composed from cross-file router fragments into `(verb, dotted.path)` keys.

`consumes` resolution goes beyond a literal `fetch(...)` call at the call site: wrapper-consume
resolution re-anchors an HTTP consume recorded against a thin positional wrapper (an n8n-class helper)
back to its real call site, and `hono/client` typed-RPC usage is recognized as an `http` consume
directly.

Both directions can be extended by an **external adapter** without touching this workspace — a
producer of a Normalized AST envelope that either stands in for an entire tree (Mode A,
`analyzeEnvelope`) or overlays extra `io`/router facts onto a natively-parsed tree (Mode B, the Rust
`EngineConfig::adapter_overlays` field, also reachable via napi's `adapterOverlays` config field) — see
[NORMALIZED_AST.md](NORMALIZED_AST.md)'s "Adapter overlays" section and
`packages/engine/examples/fastapi_overlay_adapter.rs` for a runnable FastAPI/Python demo.

## Degraded files

A file that's too large (`sizeCap`, default 1,500,000 bytes / ~1.5MB) or fails to parse is still
analyzed on a best-effort basis: line count and `line-scan` DSL rules still run against the raw text,
but symbol/import/IO extraction is skipped and the file's path is listed in the output's `degraded`
array. `method-scan`/`symbol-scan`/`io-scan` rules silently produce no findings for a degraded file
(they need the symbol/IO data that extraction didn't produce), rather than erroring.

## Minified/generated files (DSL skip)

A file is classified minified/generated when either holds: any single line is 5000+ bytes long (a
blob that big is never hand-written, even embedded in an otherwise-normal file), or it has a 500+ byte
line AND lines that long make up at least half of the file's bytes — the signature of bundler output
(esbuild etc.) and other generated code, where most content collapses onto a few giant physical lines. A
hand-written file containing one long string or comment line among ordinary lines is NOT classified (that
shape is common in normal source, and must keep its rule coverage). The engine skips
the **entire** DSL rule-pack evaluation for a classified file: every matcher type (`line-scan`,
`method-scan`, `symbol-scan`, `io-scan`), not only `line-scan`, since a giant single line offers no reliable
scoped context for any of them (a rule scoped to one symbol's span, or one line, spuriously "sees" every
unrelated pattern elsewhere on that same physical line).

This is a **distinct concept from "degraded" above**: a degraded file still runs `line-scan` DSL rules
against its raw text (only structural extraction is skipped); a minified file runs **no** DSL rule at all.
Native structural extraction — symbols/imports/IO, the dep graph, circular/dead-code analyses — is
unaffected either way: a minified file still fully participates in those, exactly like a normal file.

When 1 or more files are classified this way, the output's `warnings` array gets one aggregate entry (never
one entry per file) naming the count and a sample of the affected paths.

## Language support

| Extension(s) | Structural support |
|---|---|
| `.ts, .tsx, .js, .jsx, .mjs, .cjs, .mts, .cts` | Full: symbols, imports, calls, HTTP routes/egress |
| `.prisma` | Schema models/fields (structural + usage-aware schema rules) |
| `.java` | Method/class body spans only (lexical, not a full grammar) — enough for `method-scan` rules |
| anything else | Lexical fallback: line count + `line-scan` rules only, no symbols/imports/IO |

`.jsp`/Python sources can still participate as first-class analysis input via a hand-written external
parser adapter conforming to [NORMALIZED_AST.md](NORMALIZED_AST.md) — that path doesn't depend on
in-tree structural support for the language.

## Caching

`cacheDir` stores per-file IR and per-file rule findings, keyed by content hash plus parser and
rule-pack fingerprints. It's safe to delete at any time — it's pure derived state. A rule-pack or
config change invalidates only the cache entries it actually affects; whole-tree passes (dependency
graph, scores, cross-layer joins) are always recomputed fresh and are never cached.

## Cross-layer join

When analyzing multiple trees together (`analyzeTrees`), each parser's declared `IoFacts.provides`/
`consumes` entries are joined across trees on an exact `(kind, key)` match — e.g. a frontend's
`fetch("/users/:id")` joins a backend's registered `GET /users/:id` route. The join is a plain string
match on the normalized key, never AST matching, which is why even a crude external parser adapter can
participate as long as its key normalization is correct.

`consumes` resolution also accounts for a literal client-wide base path: when a tree sets
`axios.defaults.baseURL` to a string literal (e.g. `"/api"`, or an `http(s)://` URL's path part), that
path is prepended to every axios-tagged consume's key before joining — `GET /users` becomes
`GET /api/users`. Only the base's path part is used (the host is deploy config, not contract); a
non-literal base is left uninterpreted (adapter-overlay territory). This shifts which joins/near-misses
land: pairs where both sides genuinely agree on a prefix like `/api` go from unjoined to fully joined,
while a pair whose backend does not actually carry that prefix now honestly reports prefix drift instead
of an accidental key match.

The join itself carries three integrity gates on top of the raw `(kind, key)` match:
- **Ambiguity**: a consume key provided by 2+ distinct source trees is not auto-linked — it is reported
  separately with every candidate provider listed, rather than picking a winner. Multiple providers for
  the same key *within one tree* (e.g. a tree legitimately exposing something twice) are unaffected and
  still join normally.
- **External egress**: a consume key carrying a host (containing `://`) is treated as third-party egress
  and never cross-tree joined, so an unmatched call to someone else's API isn't reported as drift.
- **Low confidence**: an edge whose key matches an injected "generic path" pattern (e.g. `/health`, which
  many unrelated services legitimately share) is still emitted, but tagged so a consumer can discount it.

## Sentinel-based tree rewrites

A few cross-cutting facts — a NestJS app's `setGlobalPrefix(...)`, an axios instance's
`defaults.baseURL` — aren't visible to a per-file extractor, since the declaration and the routes/calls
it affects usually live in different files. These are carried as engine-internal sentinel `provides`/
`consumes` entries, collected and applied once at assemble time (prepending the prefix to the affected
route/consume keys) and then stripped before output. Producers of an external adapter envelope or
overlay must never emit these sentinel kinds — the engine drops them at ingestion rather than letting
them leak into `MinimalIr::io` or get double-applied.

Request-body shapes are resolved similarly: a `@Body() dto: SomeDto`-style provide only names its DTO
class by identifier, so the body's field shape is resolved against a tree-wide class-declaration map at
assemble time, after the class itself may live in another file.
