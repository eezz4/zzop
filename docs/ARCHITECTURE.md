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
Workers/Node servers comparing `url.pathname` against literals or matching it with anchored
`pathname.match(/…/)` regexes (parameterized routes), evidence-gated on URL provenance
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
`analyzeEnvelope`) or overlays extra `io`/router facts — and generic entity attributes (open-vocab
cross-cutting annotations a rule consumes by key, e.g. an `auth-guarded` marker) — onto a
natively-parsed tree (Mode B, the Rust
`EngineConfig::adapter_overlays` field, also reachable via any host's `adapterOverlays` config field —
a direct `zzop-facade` embedding, or `packages/mcp`'s `zzop-mcp` host through
`zzop.config.jsonc`'s `overlays` key, mapped by the shared `zzop-config` crate) — see
[NORMALIZED_AST.md](NORMALIZED_AST.md)'s "Adapter overlays" section and
`crates/engine/examples/fastapi_overlay_adapter/main.rs` for a runnable FastAPI/Python demo. A native
producer can emit the same generic entity attributes directly, with no overlay involved at all — the
native TypeScript parser's router-mounts recognizer does this for a common Express middleware guard; see
[NORMALIZED_AST.md](NORMALIZED_AST.md)'s `router_mount_fragments` section for the composed shape both
paths share.

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

This is the canonical precision-tier table — support is disclosed per language, not as a flat yes/no,
because each tier stands behind a different, honestly-scoped set of structural facts:

| Language | Tier | Extension(s) | What it extracts |
|---|---|---|---|
| TypeScript / JavaScript | Full AST (native, swc) | `.ts, .tsx, .js, .jsx, .mjs, .cjs, .mts, .cts` | Symbols, imports/dep graph, calls, HTTP provides/consumes across Express/Hono/NestJS/Next.js/tRPC and more, router-mount fragments, middleware guard attributes, ORM `db-table` facts (Prisma client accessors and TypeORM `@Entity` classes / `@InjectRepository`/`getRepository` references) |
| Python | Full AST (native, ruff) | `.py, .pyi` | **Python 3** syntax (ruff's parser linked as a Rust library — no Python runtime required; Python-2-only syntax degrades to the lexical fallback like any parse failure; the crate path (`parser-python-3`) names that supported major version, the same convention as `parser-java-21`). Symbols (`def`/`class`/methods, `__all__`-aware exports), imports/dep graph (incl. relative `from .x import y`), FastAPI route provides (decorators, `APIRouter` literal prefix, cross-file `include_router` composition), `requests`/`httpx` literal egress consumes (module-level calls plus `Session`/`Client`/`AsyncClient` instances bound by assignment or a `with`/`async with` block), and ORM `db-table` facts — SQLModel/SQLAlchemy model classes (`table=True` or a `__tablename__`) and Django models (field-driven, through any abstract base) project `db-table` provides, and their query sites (`select(X)`/`session.get(X)`; `X.objects…`) project `db-table` consumes resolved cross-file against the model class |
| Rust | Full AST (native, syn 2) | `.rs` | Symbols (top-level fn/struct/enum/trait/type-alias/const/static/union, plus `impl` block methods/assoc consts), imports/dep graph (`use`/`mod` items, `crate::`/`super::`/`self::` module-path resolution, plus same-workspace crate resolution via `Cargo.toml` manifest scan), axum router provides (builder chains, `.nest`/`.merge` cross-file composition), `reqwest` literal egress consumes |
| Go | Full CST (native, tree-sitter-go 0.25) | `.go` | Symbols (top-level func/method/type/const/var, grouped declarations expanded one symbol per spec-name), imports/dep graph (`import` declarations, `go.mod` `module` directive resolution — an import path resolves to its whole PACKAGE directory, so every file directly in that package gets a real dep-graph edge, not just one guessed file), gin and `net/http` router provides (route groups, cross-file mount composition — a router received as a function parameter is mounted from a call site in another file, including a multi-argument call resolved when exactly one argument is a mountable receiver — Go 1.22 `"METHOD /path"` mux pattern syntax), `net/http` literal egress consumes (package free functions plus the same convenience methods on a bound `http.Client` value, including `fmt.Sprintf`-reassembled path literals), and GORM ORM `db-table` facts (a `gorm.Model`-embedding or `gorm:`-tagged struct projects a `db-table` provide named by `TableName()` or GORM's default; a model composite-literal in a query method projects a `db-table` consume resolved cross-file against the struct); an ERROR CST region is never guessed past — extraction stops at the boundary of what actually parsed |
| Java | Full CST (native, tree-sitter-java 0.23.5) | `.java` | Symbols (top-level + nested class/interface/enum/record/annotation-type declarations, methods/constructors as dot-qualified `Outer.Inner.method` with body spans, `static final`/interface-constant fields), imports/dep graph (`import` declarations — plain/glob/static — resolved via an in-tree `(package, type)` index; a glob import fans out to every file in the target package, the same package-directory-wide fanout Go's own resolver uses), Spring MVC HTTP route provides (`@RestController`/`@Controller`, class + method-level `@RequestMapping`/`@GetMapping`/etc., cross-file `extends`-chain and constant-prefix resolution via the whole-corpus project pass) — Java 21 grammar coverage (records/sealed classes/pattern-switch parse as ordinary CST, though sealed-permits and pattern-switch carry no dedicated symbol extraction of their own in v1); the crate path (`parser-java-21`) names the pinned grammar version, the representative Java release this frontend targets, not a hard floor on the source dialect it can parse |
| C# | Full CST (native, tree-sitter-c-sharp 0.23.5) | `.cs` | Symbols (top-level + nested class/interface/struct/enum/record/delegate as dot-qualified `Outer.Inner` names, methods/constructors/properties with body spans, `const`/`static readonly` fields; `public`-modifier exports), imports/dep graph (`using` directives incl. `static`/alias/`global`, resolved by a namespace→files index — a `using Foo.Bar;` fans out to every file declaring namespace `Foo.Bar`, the same package-directory-fanout honesty Go/Java use), ASP.NET Core HTTP route provides (`[ApiController]`/`[Controller]` attribute controllers with class `[Route("api/[controller]")]` + method `[HttpGet]`/`[HttpPost("{id}")]`/… composition and the `[controller]` token, plus same-file Minimal-API `app.MapGet`/`MapGroup` literal routes), `HttpClient` literal HTTP egress consumes (`GetAsync`/`PostAsync`/`GetFromJsonAsync`/… with `$"…"` interpolation reassembly) |
| Prisma | Lexical schema (native) | `.prisma` | Schema models/fields — structural, plus usage-aware schema rules; each model also projects a `db-table` io provide (accessor-cased `table:` key, joining the TS client-side `db-table` consumes) |
| SQL (DDL) | Lexical DDL (native) | `.sql` | `CREATE TABLE` statements → `db-table` io provides only (`table:<name>`, quote-stripped, schema qualifier dropped, accessor-cased to match the Prisma/TS db-table key — same lower-first transform; persistent tables only — a session-local `CREATE TEMP`/`TEMPORARY TABLE` mints no provide, since no other layer can join a connection-scoped name, while `UNLOGGED` — crash-unsafe but cross-connection — still provides) — migration files (Flyway/Liquibase-style) light up the db-table channel for MyBatis/JDBC-style stacks; no symbols/imports/consumes |
| Everything else | External adapter | any | First-class via the Normalized AST envelope protocol — Mode A (`analyzeEnvelope`, stands in for a whole tree) or Mode B (overlays facts onto a natively-parsed tree); see [NORMALIZED_AST.md](NORMALIZED_AST.md) |

A file that falls outside what its tier extracts (a `.py`/`.ts`/`.rs` file that fails to parse, or any
extension in the "everything else" row with no adapter attached) still gets the **degraded lexical
fallback**: line count and `line-scan` DSL rules run against the raw text rather than a hard failure —
see "Degraded files" above.

Python's v1 scope is deliberate, not an oversight: Flask/Django routes and FastAPI `Depends` auth
attributes are roadmap (SQLModel/SQLAlchemy and Django ORM *table* facts now ship — see the Python row
above). (`requests`/`httpx` `Session`/`Client`/
`AsyncClient` INSTANCES are now recognized — a name bound to a client constructor via assignment or a
`with`/`async with` binding has its `.get()`/`.post()`/… keyed as egress, so the idiomatic async
`async with httpx.AsyncClient() as c: await c.get(url)` lands natively.) The Mode-B overlay path already
covers the remaining shapes today — see
`crates/engine/examples/fastapi_overlay_adapter/main.rs`, the reference FastAPI/Python overlay adapter,
which remains the escape hatch for exactly what native v1 skips.

Rust's v1 scope is similarly deliberate: Rocket/warp/actix-web decorator- or macro-attribute-style route
registration, axum `Extension`/`State`-based auth guards, and Diesel/SQLx ORM table facts are roadmap —
only axum's builder-chain route registration and `reqwest` literal egress are extracted natively today.
`macro_rules!`-defined items and identifiers used only inside a macro invocation's argument tokens are
also out of scope (syn parses macro arguments as an opaque token stream, not a structured tree) — see
`zzop_parser_rust`'s own crate doc for the exact v1 gaps.

Go's v1 scope is deliberate too: echo/chi/fiber decorator-free route registration idioms,
`client.Do(req)` request dispatch (where the URL rides an `*http.Request` value built elsewhere), and
`embed`/`cgo`-loaded files are all roadmap — only gin route groups, `net/http`'s
`DefaultServeMux`/`NewServeMux` (including Go 1.22's `"METHOD /path"` pattern syntax), and `net/http`'s
egress — both the package-level free functions (`http.Get`/`Post`/`PostForm`/`Head`, with `fmt.Sprintf`
template reassembly) AND the same convenience methods on a bound `*http.Client`/`http.Client` value
(`c := &http.Client{}`/`var c = http.Client{}`/`new(http.Client)`, then `c.Get(url)`/…) — are extracted
natively today. Gin's
cross-file mount idiom — a router received as a function PARAMETER (`func setup(r *gin.RouterGroup) {
... }`, no local `:=`/`=` binding to anchor on, unlike the local-binding case above) — is shipped: the
parameter is tracked as a receiver whose fragment is named after the
enclosing function, and a call-site mount (`pkg.Setup(r)`, or a same-file `Setup(r)`) composes that
fragment onto the caller's own receiver, closing the dominant real-world cross-file registration gap.
The call side also resolves a multi-argument call (`pkg.Register(db, api.Group("/admin"))`) as long as
EXACTLY ONE argument is a mountable receiver (a bare tracked receiver or `<tracked>.Group("literal")`)
— every other argument (a db handle, a config struct, a literal, ...) is ignored outright. Two or more
mountable-receiver arguments in the same call (`pkg.Wire(a.Group("/a"), b.Group("/b"))`) is genuinely
ambiguous — which one does `Wire` actually mount onto? — so the whole call is rejected rather than
guessed. Receiver METHODS (`func (s *Server) Register(r *gin.RouterGroup)`, a struct-field-style
receiver) remain the one documented blind spot in this idiom: `method_declaration` is a distinct
grammar node this recognizer never matches against, so a router mounted from a method body is not
recognized — roadmap, not attempted. `tree-sitter-go` is a full CST (not merely lexical), but this
crate never guesses past an `ERROR`/`MISSING` region: a single malformed statement skips just that
subtree, extracting from every other still-valid region of the same file — see `zzop_parser_go`'s own
crate doc for the exact v1 gaps and the never-guess discipline.

Java's v1 scope is deliberate too, same shape as Python's/Rust's/Go's own: this engine has no Java-side
HTTP-egress extractor yet (`RestTemplate`/`WebClient` consumes are not extracted — see
`framework_silence`'s `org.springframework.web.client` disclosure vocab, the escape hatch for exactly this
gap), functional/lambda `RouterFunction` route registration and non-Spring frameworks (JAX-RS, Micronaut,
Quarkus) are roadmap, and record-component accessors/annotation-type elements are not projected as method
symbols (structurally implicit, never a written declaration — see `zzop_parser_java_21`'s own crate doc
for the exact v1 gaps). `tree-sitter-java` is a full CST (not merely lexical), and — like `zzop_parser_go`
— never guesses past an `ERROR`/`MISSING` region.

C#'s v1 scope is deliberate too, same shape as the others': attribute-controller + same-file Minimal-API
route provides and `HttpClient` literal egress are extracted natively today; cross-file base-controller
`[Route]` inheritance, cross-statement Minimal-API group variables (`var g = app.MapGroup("/api");
g.MapGet(...)`), `HttpClient.SendAsync(HttpRequestMessage)`, conventional routing (`MapControllerRoute`),
and SDK-injected implicit/`global` usings beyond what the source itself declares are roadmap. Namespace
resolution is namespace-level: a `using` that targets a TYPE (via `using static`/alias) resolves to
nothing — an accepted under-approximation with no by-type index, the same honesty argument the other
fanout resolvers make. See `zzop_parser_csharp`'s own crate doc for the exact v1 gaps. `tree-sitter-c-sharp`
is a full CST and, like the other tree-sitter frontends, never guesses past an `ERROR`/`MISSING` region.

Each native parser carries its own internal `PARSER_FINGERPRINT` that keys the per-file cache. Each
begins with a stable technique+grammar-version stem — `zzop-parser-python-3`'s `python3/ruff-0.0.4/…`,
`zzop-parser-prisma`'s `prisma/…`, `zzop-parser-rust`'s `rust/syn-2/…`, `zzop-parser-go`'s
`go/tree-sitter-go-0.25.0/…`, `zzop-parser-java-21`'s `java21/tree-sitter-java-0.23.5/…`,
`zzop-parser-csharp`'s `csharp/tree-sitter-c-sharp-0.23.5/…`, `zzop-parser-sql`'s `sql/…` — followed by a
`vN` and a chain of `+feature-vN` tags that grows with each projection-changing extraction bump (so a
literal copy here would go stale on the next bump — deliberately elided). The TypeScript, Prisma, Python,
Java, Rust, Go, C#, and SQL fingerprints are each surfaced in full in `zzop --version`'s output, so a
given build's actual parser identity is machine-checkable there, not asserted by this table.

A normal-sized file whose extension has no native parser is not counted in `degraded` (that's a
size-cap/parse-failure fact, not a coverage one) — instead it self-reports as a per-extension entry in
`warnings`, naming the extension, a file count, and a path sample, pointing at the `overlays: [...]`
config knob. An oversized file of that same unparsed extension gets both: it still lands in `degraded`
and still names its extension in the per-extension warning — the two facts are orthogonal.

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

A per-tree deployment-topology declaration (`mountedAt`/`mounts`/`hosts` — see
[modules/napi.md](modules/napi.md#functions)'s `AnalyzeRequest` field table) supplies the one class of join
information that lives only in infra, not in either repo's source: a gateway/ingress mount prefix, and
which hosts a tree owns. Mounts apply as the last provide-key transform, stacking on top of any
code-extracted prefix (e.g. NestJS's `setGlobalPrefix`); a declared host re-keys a matching absolute-URL
consume to an internal joinable key before the external-egress gate above ever applies. Both self-disclose
via a `warnings` entry when they turn out to have zero effect on the join.

Routing is resolved from **visible code literals on two axes — path and HTTP method (verb)**. A dynamic
route on either axis is an injection boundary, never guessed. A computed/opaque URL path stays an
unresolved consume (surfaced as a near-miss with a "verify manually" caveat); a route whose handler serves
*any* method (a `pages/api` catch-all, a `pathname`-dispatch or Go `HandleFunc` block naming no method
literal) is emitted as a single verb-unknown route and disclosed via `cross-layer/unknown-verb-route`
rather than inventing a `{GET, POST}` pair. A route zzop cannot resolve from source this way — a
verb-unknown handler, a non-literal path (`@GetMapping(ApiPaths.USERS)`), or a computed client URL — is
completed by **injecting the concrete route fact**: either a full Normalized AST adapter overlay, or, for a
handful of routes, the lightweight per-tree `routes: [{ key, role }]` declaration (see
[modules/napi.md](modules/napi.md#functions)'s `AnalyzeRequest` field table), which expands into a
synthetic overlay and joins through the identical path. **Deployment-config routing is the same boundary**: zzop does **not** read
deployment config files (`next.config` `rewrites`/`redirects`, `vercel.json`, nginx/ingress). A uniform
gateway/ingress prefix or host is injected via the `mountedAt`/`mounts`/`hosts` declaration above; an
arbitrary path-rewrite map (`/legacy/* → /v2/*`) is **not** modeled in v1 — a deployment that rewrites
paths this way can surface a near-miss/unprovided finding that the unseen rewrite would explain, so treat
cross-layer route findings as "verify against your deployment topology," not ground truth.

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
