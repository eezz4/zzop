# How the engine processes your tree

A short orientation for making sense of the `analyze`/`analyzeTrees` output — full field-by-field
shapes are in [modules/facade.md](modules/facade.md#the-zzop-facade-json-contract); this page just explains
what's actually happening underneath so the output makes sense.

**Which output you are reading.** Everything below describes what the ENGINE computes, which is what a
direct `zzop-facade` embedding receives verbatim. What the two shipped binaries print is one layer
above: `zzop-summary` (`crates/summary`) owns every bit of shaping between the facade output and a
product's answer — the caps and their truncation disclosures, the findings filters, the warning merge,
and the `facts`/`graph`/`manifest`/`coverage` projections. Both `zzop` and `zzop-mcp` call it and neither
reimplements any of it, so for the lanes both surfaces expose — `analyze`/`cross`/`endpoint`/`file`/
`analyze-envelope` and the two offline validators — a CLI run and an MCP tool call against the same path
give the identical answer. The findings-list knobs (`severity`, `rule`, `limit`) are declared on the MCP
tools (`packages/mcp/src/tools/definitions.rs`) AND on their CLI twins as `--severity`/`--rule`/`--limit`
(`packages/cli-bin/src/cli/args.rs`), both parsed into the one shared `FindingFilters` — so an MCP call
passing `limit: 5` and `zzop analyze --limit 5` return the same shorter list over the same tree, with the
full counts unaffected either way. The projections named
above are further not paired at all: `facts`, `graph`, `coverage` and `manifest` (with `diff`) are CLI-only lanes
with **no MCP tool twin**, and `docs/contracts/surface-parity.json`'s `_cliOnlyLanes` records the reason
for each lane it declares — that registry's own keys are the list, never this sentence. `explain` has no
MCP twin either, and as of 2026-07-27 it IS declared there: it is a lookup over compiled-in rule data
rather than a projection of an analysis, and MCP already reaches the same data through the
`zzop://contract/rule-catalog` resource. `init` is the one unpaired lane deliberately left OUT of the
registry — a subcommand whose only implementation is argv parsing plus a file write, over bytes MCP
already serves as the `config-template` resource, so there is no projection for the registry to name.
Where either surface differs from the raw facade output, that same registry names the field and the reason.

## The IR your `ir` field contains

Every analyzed file is parsed and projected into a language-neutral intermediate representation
(`CommonIr`): symbols (functions/classes/consts/types/interfaces), RESOLVED IN-TREE import edges
(an import whose target zzop did not resolve inside the tree contributes no edge, so a low edge count
can mean "unresolved", not "uncoupled" — see [the facade module doc](modules/facade.md)), line counts, and
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
a direct `zzop-facade` embedding, or either shipped product (`packages/cli-bin`'s `zzop`,
`packages/mcp`'s `zzop-mcp`) through `zzop.config.jsonc`'s `overlays` key, mapped by the shared
`zzop-config` crate) — see
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

## Minified line shape (DSL skip)

A file has minified line shape when either holds (this measures LINE SHAPE and decides nothing about whether a file is machine-generated — a generated file with ordinary line lengths is not skipped by it): any single line is 5000+ bytes long (a
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
| TypeScript / JavaScript | Full AST (native, swc) | `.ts, .tsx, .js, .jsx, .mjs, .cjs, .mts, .cts` | Symbols, imports/dep graph, calls, HTTP provides/consumes across Express/Hono/NestJS/Next.js/tRPC and more, router-mount fragments, middleware guard attributes, ORM `db-table` facts (Prisma client accessors and TypeORM `@Entity` classes / `@InjectRepository`/`getRepository` references), and ORM-LESS `db-table` consumes — the tables named inside a raw SQL statement string (`env.DB.prepare("SELECT … FROM ledger")`, `` sql`…` ``), read through `parser-sql` so the key matches the migration-side provide. Recognition needs the string to OPEN a statement head with UPPERCASE keywords: that is the only thing separating a query from English prose like "Select a date from the list", which is a structurally valid `SELECT`. Lower-case SQL is therefore not recognized (a silent under-report, never a claim of "no table access"), and an interpolated table name (`` `… FROM ${t}` ``) is dropped rather than guessed |
| Python | Full AST (native, ruff) | `.py, .pyi` | **Python 3** syntax (ruff's parser linked as a Rust library — no Python runtime required; Python-2-only syntax degrades to the lexical fallback like any parse failure; the crate path (`parser-python-3`) names that supported major version, the same convention as `parser-java-21`). Symbols (`def`/`class`/methods, `__all__`-aware exports), imports/dep graph (incl. relative `from .x import y`), FastAPI route provides (decorators, `APIRouter` literal prefix, cross-file `include_router` composition), Django URLconf route provides (a top-level `urlpatterns` list's `url()`/`re_path()`/`path()` entries, with `include('<dotted.module>')` composed cross-file through the same router-mount pass FastAPI uses; the HTTP method lives in the view class, not the URLconf, so every such route is emitted verb-unknown rather than guessed — it drives the `cross-layer/unknown-verb-route` disclosure and never joins by exact method), `requests`/`httpx` literal egress consumes (module-level calls plus `Session`/`Client`/`AsyncClient` instances bound by assignment or a `with`/`async with` block), CALL SITES for the whole-repo call graph (so the handler-reachability rules run on Python routes — see the `mutating-route-no-auth` row in [rules/catalog.md](rules/catalog.md)), AUTH-GUARD evidence (FastAPI `Depends(...)` in a route decorator's `dependencies=`, a parameter default, an `Annotated[..., Depends(...)]` parameter, or an `Annotated` alias that is BOUND in the route's own file and resolved tree-wide; Django REST Framework `permission_classes` on a view class, joined to its URLconf route by view name. In every shape the injected callable's NAME must read as a guard — see the not-recognized list below), and ORM `db-table` facts — SQLModel/SQLAlchemy model classes (`table=True` or a `__tablename__`) and Django models (field-driven, through any abstract base) project `db-table` provides, and their query sites (`select(X)`/`session.get(X)`; `X.objects…`) project `db-table` consumes resolved cross-file against the model class |
| Rust | Full AST (native, syn 2) | `.rs` | Symbols (top-level fn/struct/enum/trait/type-alias/const/static/union, plus `impl` block methods/assoc consts), imports/dep graph (`use`/`mod` items, `crate::`/`super::`/`self::` module-path resolution, a `#[path = "..."]` module declaration resolved to the file it NAMES rather than to the file-name convention — the literal is taken relative to the declaring file's own directory, and one that points outside the analyzed tree is dropped rather than clamped to something in-tree, plus same-workspace crate resolution via `Cargo.toml` manifest scan), axum router provides (builder chains, `.nest`/`.merge` cross-file composition), `reqwest` literal egress consumes, raw-SQL `db-table` consumes (the tables named inside a SQL statement string, including inside a `sqlx::query!`-style macro's token stream, read through `parser-sql` so the key matches the migration-side provide; recognition needs the string to OPEN a statement head with UPPERCASE keywords, the same discriminator against English prose the TypeScript row describes, so lower-case SQL — `sqlx::query("select id from ledger")` — is not recognized, a silent under-report that can leave a migration-side `table:` provide reading as "declared, consumed by nobody", never a claim of "no table access"; a table name built by interpolation, `format!("… FROM {t}")`, is dropped rather than guessed), CALL SITES for the whole-repo call graph (resolved crate-locally AND across same-workspace crates, so the handler-reachability rules run on Rust routes), and AUTH-GUARD evidence in the shape Rust actually uses — a request EXTRACTOR in the handler signature (`async fn create(user: AuthUser, ..)`) is projected as a call-graph edge out of the handler, so the ordinary guard-name vocabulary matches it; an OPTIONAL extractor (`vocabulary.rustOptionalExtractorPrefixes`, default `maybe`/`optional`) never counts. Auth applied at the ROUTER level (a tower `.route_layer`/`.wrap`) is NOT visible — every run whose Rust routes are in range discloses that gap in its own `warnings` |
| Go | Full CST (native, tree-sitter-go 0.25) | `.go` | Symbols (top-level func/method/type/const/var, grouped declarations expanded one symbol per spec-name), imports/dep graph (`import` declarations, `go.mod` `module` directive resolution — an import path resolves to its whole PACKAGE directory, so every file directly in that package gets a real dep-graph edge, not just one guessed file), gin and `net/http` router provides (route groups, cross-file mount composition — a router received as a function parameter is mounted from a call site in another file, including a multi-argument call resolved when exactly one argument is a mountable receiver — Go 1.22 `"METHOD /path"` mux pattern syntax), `net/http` literal egress consumes (package free functions plus the same convenience methods on a bound `http.Client` value, including `fmt.Sprintf`-reassembled path literals), and GORM ORM `db-table` facts (a `gorm.Model`-embedding or `gorm:`-tagged struct projects a `db-table` provide named by `TableName()` or GORM's default; a model composite-literal in a query method projects a `db-table` consume resolved cross-file against the struct); an ERROR CST region is never guessed past — extraction stops at the boundary of what actually parsed |
| Java | Full CST (native, tree-sitter-java 0.23.5) | `.java` | Symbols (top-level + nested class/interface/enum/record/annotation-type declarations, methods/constructors as dot-qualified `Outer.Inner.method` with body spans, `static final`/interface-constant fields), imports/dep graph (`import` declarations — plain/glob/static — resolved via an in-tree `(package, type)` index; a glob import fans out to every file in the target package, the same package-directory-wide fanout Go's own resolver uses), Spring MVC HTTP route provides (`@RestController`/`@Controller`, class + method-level `@RequestMapping`/`@GetMapping`/etc., cross-file `extends`-chain and constant-prefix resolution via the whole-corpus project pass), CALL SITES for the whole-repo call graph (attributed to the enclosing method/constructor body, riding the same `SymbolGraph`/BFS the TypeScript and Python rows do — so the handler-reachability rules run on Spring MVC routes), AUTH-GUARD evidence (Spring Security method-security annotations — `@PreAuthorize`/`@PostAuthorize`/`@Secured`/`@RolesAllowed`, class-level guarding every route the controller declares — feeding the same framework-neutral `(file, line)` exemption set NestJS `@UseGuards` does, since an annotation application is not a call edge the BFS could see), `RestTemplate`/`WebClient` literal HTTP egress consumes (client-specific method names + `.get().uri("…")` chains; `exchange`/`.method(...)` only with a literal `HttpMethod.X`; a variable/concatenated URL stays honestly unresolved; Feign and `java.net.http.HttpClient` are not recognized — disclosed), and JPA ORM `db-table` provides (`@Entity` classes: `@Table(name = "…")` literal verbatim, else Spring Boot's snake-case class-name default; a non-literal `@Table` name skips the class rather than guessing) — Java 21 grammar coverage (records/sealed classes/pattern-switch parse as ordinary CST, though sealed-permits and pattern-switch carry no dedicated symbol extraction of their own in v1); the crate path (`parser-java-21`) names the pinned grammar version, the representative Java release this frontend targets, not a hard floor on the source dialect it can parse |
| C# | Full CST (native, tree-sitter-c-sharp 0.23.5) | `.cs` | Symbols (top-level + nested class/interface/struct/enum/record/delegate as dot-qualified `Outer.Inner` names, methods/constructors/properties with body spans, `const`/`static readonly` fields; `public`-modifier exports), imports/dep graph (`using` directives incl. `static`/alias/`global`, resolved by a namespace→files index — a `using Foo.Bar;` fans out to every file declaring namespace `Foo.Bar`, the same package-directory-fanout honesty Go/Java use), ASP.NET Core HTTP route provides (`[ApiController]`/`[Controller]` attribute controllers with class `[Route("api/[controller]")]` + method `[HttpGet]`/`[HttpPost("{id}")]`/… composition and the `[controller]` token, plus same-file Minimal-API `app.MapGet`/`MapGroup` literal routes), `HttpClient` literal HTTP egress consumes (`GetAsync`/`PostAsync`/`GetFromJsonAsync`/… with `$"…"` interpolation reassembly), and EF Core ORM `db-table` provides (`DbSet<T>` properties named by EF's property-name convention + `[Table("…")]` attribute renames, which override the convention; a non-literal `[Table]` argument skips the class rather than guessing) |
| Prisma | Lexical schema (native) | `.prisma` | Schema models/fields — structural, plus usage-aware schema rules; each model also projects a `db-table` io provide (accessor-cased `table:` key, joining the TS client-side `db-table` consumes) |
| SQL (DDL) | Lexical DDL (native) | `.sql` | `CREATE TABLE` statements → `db-table` io provides only (`table:<name>`, quote-stripped, schema qualifier dropped, accessor-cased to match the Prisma/TS db-table key — same lower-first transform; persistent tables only — a session-local `CREATE TEMP`/`TEMPORARY TABLE` mints no provide, since no other layer can join a connection-scoped name, while `UNLOGGED` — crash-unsafe but cross-connection — still provides) — migration files (Flyway/Liquibase-style) light up the db-table channel for MyBatis/JDBC-style stacks; no symbols/imports, and a `.sql` FILE never projects a consume. The crate does own the channel's consume-side reader (the tables one SQL statement string names), but it is called by another parser holding such a string — see the TypeScript row — so both keys come out of one transform and cannot drift |
| Everything else | External adapter | any | First-class via the Normalized AST envelope protocol — Mode A (`analyzeEnvelope`, stands in for a whole tree) or Mode B (overlays facts onto a natively-parsed tree); see [NORMALIZED_AST.md](NORMALIZED_AST.md) |

A file that falls outside what its tier extracts (a `.py`/`.ts`/`.rs` file that fails to parse, or any
extension in the "everything else" row with no adapter attached) still gets the **degraded lexical
fallback**: line count and `line-scan` DSL rules run against the raw text rather than a hard failure —
see "Degraded files" above.

Python's v1 scope: Flask routes are not recognized (Django URLconf routes,
SQLModel/SQLAlchemy and Django ORM *table* facts, the call graph, and FastAPI `Depends` / DRF
`permission_classes` auth evidence now ship — see the Python row above). Within the auth evidence that
ships, these shapes are deliberately NOT recognized and leave the finding firing rather than clearing it
silently: a router-level `APIRouter(dependencies=[...])`, `fastapi.Security(...)`, a guard applied by a
custom decorator, DRF's project-wide `DEFAULT_PERMISSION_CLASSES` setting, `permission_classes` inherited
from a base view, a runtime `get_permissions()`, and the `@login_required`/`@permission_classes([...])`
function-view decorators. Three more shapes are recognized-but-REFUSED, which is a different statement
and the one most likely to surprise: a `Depends(...)` whose injected callable's NAME does not read as an
authorization check (`Depends(get_db)` is dependency injection, not a guard — and so, deliberately, is
`Depends(ensure_member)`, because the vocabulary is precision-first and would rather miss a guard than
erase a finding); a dependency FACTORY called with its own anonymous switch off
(`Depends(get_current_user_authorizer(required=False))` returns `None` for a tokenless caller, so it
gates nothing, and a non-literal switch is refused too); and an `Annotated` alias whose name is not bound
in the route's own file, or that two modules declare with disagreeing verdicts. All of these leave the
finding firing. (`requests`/`httpx` `Session`/`Client`/
`AsyncClient` INSTANCES are now recognized — a name bound to a client constructor via assignment or a
`with`/`async with` binding has its `.get()`/`.post()`/… keyed as egress, so the idiomatic async
`async with httpx.AsyncClient() as c: await c.get(url)` lands natively.) The Mode-B overlay path already
covers the remaining shapes today — see
`crates/engine/examples/fastapi_overlay_adapter/main.rs`, the reference FastAPI/Python overlay adapter,
which remains the escape hatch for exactly what native v1 skips.

**What "roadmap" means in this document** (stated once, applies to every use below): a recognized,
named gap — NOT a commitment to build it. Native coverage here grows from ONE driver: what this
project's maintainer actually uses. Framework idioms are an endless long tail and the project is
maintained by one person, so coverage deepens where that person works and stays put elsewhere; if
their stack changes, the native side grows with it. Popularity, issue requests, and corpus arrivals
are not, by themselves, what moves this. Everything outside that path is reached through injection
(Mode A/B envelopes) — a supported, documented, first-class route, not a consolation prize. So read
"roadmap" as "here is the boundary, and here is the door", never as "coming soon".

Rust's v1 scope is similarly deliberate: Rocket/warp/actix-web decorator- or macro-attribute-style route
registration, axum `Extension`/`State`-based auth guards, and the ORM SCHEMA DSLs (Diesel's `table!`,
SeaORM's `DeriveEntityModel`) are roadmap — axum's builder-chain route registration, `reqwest` literal
egress, and raw-SQL `db-table` touches are what is extracted natively today. That last one is
shape-keyed rather than crate-keyed: an UPPERCASE-headed SQL statement STRING in a `.rs` file names its
tables wherever it sits, so sqlx (`query!`/`query_as!` included), tokio-postgres, rusqlite,
`diesel::sql_query` and `sea_orm::Statement::from_string` all land through one recognizer — while
lower-case SQL (`query("select id from ledger")`) falls outside the shared statement gate and is not
recognized at all, and a table named only by interpolation (`format!("SELECT * FROM {t}")`) is dropped
rather than guessed.
`macro_rules!`-defined items and identifiers used only inside a macro invocation's argument tokens are
also out of scope for the SYMBOL and call-graph layers (syn parses macro arguments as an opaque token
stream, not a structured tree; the raw-SQL adapter reads string literals out of that stream, which is
all it needs) — see `zzop_parser_rust`'s own crate doc for the exact v1 gaps.

Rust also carries one exclusion no other language has yet, and it changes what a run REPORTS rather than
what it extracts: a finding whose line sits inside a `#[cfg(test)]`/`#[test]`-gated item — a `mod`, `fn`,
`impl` or trait member, or a whole file under an inner `#![cfg(test)]` — is dropped, and the raw-SQL and
egress adapters skip those regions rather than extract from them. Every other language this workspace
parses names its tests in the PATH (`foo.test.ts`, `tests/test_foo.py`), which the shared path-shaped
exclusion already sees; Rust's dominant convention puts unit tests INSIDE the shipping file, where no
path regex can reach them. The exception is the credential-at-rest rule family, which opts out and keeps
judging test regions — a committed key is leaked whether or not the compiler keeps it. Both halves are
disclosed per rule in [rules/catalog.md](rules/catalog.md). The axis is per-file and attribute-driven,
so a file whose test-ness is declared only by its parent (`#[cfg(test)] mod helpers;`) carries no
attribute of its own and is not covered by it; the path axis still owns "the whole file is a test file".
External adapters can project the same spans (`testSpans` in [NORMALIZED_AST.md](NORMALIZED_AST.md)), so
the exclusion is Rust-only by who ships it today, not by construction.

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

Java's v1 scope is deliberate too, same shape as Python's/Rust's/Go's own: `RestTemplate`/`WebClient`
literal egress IS extracted (client-specific method names like `getForObject`, `exchange` with a literal
`HttpMethod.X`, and `.get().uri("…")` chains — `RestTemplate.put`/`.delete` are deliberately not
recognized, since those generic names would false-key `Map.put` and friends; a variable/concatenated URL
stays an unresolved consume rather than a guess), but Feign `@FeignClient` interfaces and
`java.net.http.HttpClient` (URL not visible at the `send` call site) remain unrecognized — the
`framework_silence` `org.springframework.web.client` tripwire now signals an unrecognized idiom rather
than guaranteed blindness. JPA `@Entity`/`@Table(name = "…")` classes provide `db-table` facts (a
non-literal `@Table` name skips the class; a missing one derives Spring Boot's snake-case default), with
no query-site consume arm yet. Functional/lambda `RouterFunction` route registration and non-Spring
frameworks (JAX-RS, Micronaut, Quarkus) are roadmap, and record-component accessors/annotation-type
elements are not projected as method symbols (structurally implicit, never a written declaration — see
`zzop_parser_java_21`'s own crate doc for the exact gaps). `tree-sitter-java` is a full CST (not merely
lexical), and — like `zzop_parser_go` — never guesses past an `ERROR`/`MISSING` region.

C#'s v1 scope is deliberate too, same shape as the others': attribute-controller + same-file Minimal-API
route provides, `HttpClient` literal egress, and EF Core `db-table` provides (`DbSet<T>` properties —
table named after the property, EF's convention — and `[Table("…")]` attributes, which override the
convention; a non-literal `[Table]` argument skips the class, and a same-file `[Table]` suppresses the
DbSet convention name, while a CROSS-file rename is a documented per-file limit — the stale
convention-named provide simply never joins) are extracted natively today; fluent
`modelBuilder.Entity<T>().ToTable(...)` mapping, query-site `db-table` consumes, cross-file
base-controller `[Route]` inheritance, cross-statement Minimal-API group variables (`var g =
app.MapGroup("/api"); g.MapGet(...)`), `HttpClient.SendAsync(HttpRequestMessage)`, conventional routing
(`MapControllerRoute`), and SDK-injected implicit/`global` usings beyond what the source itself declares
are roadmap. Namespace
resolution is namespace-level: a `using` that targets a TYPE (via `using static`/alias) resolves to
nothing — an accepted under-approximation with no by-type index, the same honesty argument the other
fanout resolvers make. See `zzop_parser_csharp`'s own crate doc for the exact v1 gaps. `tree-sitter-c-sharp`
is a full CST and, like the other tree-sitter frontends, never guesses past an `ERROR`/`MISSING` region.

Each native parser carries its own internal `PARSER_FINGERPRINT` that keys the per-file cache. Each
begins with a stable technique+grammar-version stem — `zzop-parser-python-3`'s `python3/ruff-0.0.4/…`,
`zzop-parser-prisma`'s `prisma/…`, `zzop-parser-rust`'s `rust/syn-2/…`, `zzop-parser-go`'s
`go/tree-sitter-go-0.25.0/…`, `zzop-parser-java-21`'s `java21/tree-sitter-java-0.23.5/…`,
`zzop-parser-csharp`'s `csharp/tree-sitter-c-sharp-0.23.5/…`, `zzop-parser-sql`'s `sql/…` — followed by a
`vN` and a chain of `+feature-vN` tags recording projection-shape generations (so a literal copy here
would go stale — deliberately elided). Each fingerprint is an **ID, not a version**: it names the pinned
frontend and the projection generation, and it does not have to move when extraction code changes,
because `crates/engine/build.rs` hashes each parser crate's whole dependency closure into the cache key
beside it. Correctness no longer depends on remembering to bump the string.

The consequence for reading a fingerprint off a surface: `zzop_facade::version_string()` carries each
of these as `zzop-parser-<x>=<id>/<hash>` — the frontend ID joined to that same derived closure hash
(`zzop_engine::parser_fingerprints()`) — plus a `zzop-engine=<hash>` token for the engine's own source
(the one producer of cached bytes that sits in no parser's closure), and reaches a user as the `tool`
field of `zzop manifest`/`zzop facts`, `zzop graph`'s `%% tool:` line, and what `zzop version --verbose`
(and `zzop-mcp version --verbose`, byte-identical) prints. So the string answers **which build analyzed
your files**, not merely which frontend read them: two builds whose extraction differs print two
different strings, because the stamp moves with the exact hashes that would invalidate a warm cache.
Plain `zzop version`/`--version`/`-V` prints the bare release number only, and carries no fingerprints.

A normal-sized file whose extension has no native parser is not counted in `degraded` (that's a
size-cap/parse-failure fact, not a coverage one) — instead it self-reports as a per-extension entry in
`warnings`, naming the extension, a file count, and a path sample, pointing at the `overlays: [...]`
config knob. An oversized file of that same unparsed extension gets both: it still lands in `degraded`
and still names its extension in the per-extension warning — the two facts are orthogonal.

## On-disk layout

zzop touches exactly two directory names inside a tree it analyzes, and they differ by one leading dot:

| Path | Owner | Contents today | Version control |
|---|---|---|---|
| `.zzop/` | zzop | `.zzop/cache/` (the default `cacheDir`, below) | **Ignored.** Pure derived state; deleting it costs a warm cache and nothing else. |
| `zzop/` | you | `zzop/rules/` (custom DSL rule packs), `zzop/adapters/` (Normalized-AST adapter overlays) | **Committed.** These are source, and losing them loses work. |

Nothing zzop ships ever writes into `zzop/`; it only ever reads from it. Everything zzop writes goes
under `.zzop/`. That is the whole rule, and the two names are deliberately near-identical so the
directory listing reads as one tool's footprint.

The near-identity has one sharp edge worth stating explicitly: **ignore rules must be anchored, never
globbed.** `**/.zzop/` (what this repo's own `.gitignore` uses) matches only the derived directory;
`zzop*` matches both and would silently drop every rule pack you wrote out of version control — no error
from git, no warning from zzop, and nothing in a diff that looks wrong. The anchoring is not stylistic.

`zzop/rules/` is additionally a **default discovery location**: a config that does not declare
`packs.extraDirs` picks it up automatically when it exists. Declaring `packs.extraDirs` replaces the
default outright rather than merging with it, so a run's pack directories always have exactly one
origin; declaring it as `[]` is the explicit opt-out. A tree with no `zzop/rules/` produces no warning.
`zzop/adapters/` is a naming convention only — overlays are loaded from the paths your `overlays` key
names, wherever they are.

## Caching

`cacheDir` stores per-file IR and per-file rule findings, keyed by content hash plus the file's own
path, the parser fingerprint, the declared **convention vocabulary**, and the rule-pack fingerprint.
It's safe to delete at any time — it's pure derived state. A rule-pack or config change invalidates
only the cache entries it actually affects; whole-tree passes (dependency graph, scores, cross-layer
joins) are always recomputed fresh and are never cached.

**Editing `vocabulary` re-analyzes.** The names you declare there are not report-time filters — they
decide what gets extracted (which calls are write sites, which routes count as guarded, which schema
fields are money). So the whole `vocabulary` object is part of both cache keys: change any key in it and
the affected files are re-parsed rather than answered from entries written under the previous
vocabulary. The trade is deliberate: a broader-than-necessary invalidation costs one slower run, while a
narrower one would hand you a stale answer.

**An entry is never stale, only orphaned.** Entries are immutable and addressed by a digest of their own
key, so one that stops being asked for is simply never read again — editing a file, upgrading zzop, or
changing `vocabulary` all leave the previous entries on disk unread rather than wrong. Deleting the
directory yourself is always safe, at any time, for the same reason.

**Two things reclaim disk, and they answer different questions.**

- *The schema version changed* — every entry was written under a contract this build does not speak, so
  all of them go at once. The version is a hash of the code that decides what gets persisted, so it moves
  when that code moves and not otherwise. Upgrading zzop only wipes if the upgrade actually changed
  something about the stored shape or meaning; a release that fixes packaging or CI leaves your warm
  cache warm. The comparison is equality, not ordering: downgrading, or skipping several versions,
  invalidates exactly the same way as stepping forward one.
- *The directory is over its size budget* — the contract is fine, there is just more on disk than the
  cap allows, so the oldest-written entries go until it is back under. This is what bounds growth, since
  ordinary file edits orphan entries forever otherwise. Being evicted while still useful costs one
  recompute and never a wrong answer, which is why no reachability analysis is needed or attempted.

Up to and including v0.29.1 — every version installable today — the schema version carried the release
number, so *every* upgrade wiped the cache whether or not anything about the analysis had changed, and
there was no size cap at all. That single value was doing both jobs above, which meant the only way to
reclaim disk was to charge every user a full cold run on every release; the split described here holds
from the next release onward.

**Where it lands, and who decides.** The value is a directory, and two dialects answer "what if I don't
set one?" differently — the split is deliberate, so read the one you are in:

- **`zzop` / `zzop-mcp` / any `zzop.config.jsonc` run** (the config front end, `crates/config`): an
  absent `cacheDir` defaults to **`.zzop/cache`**, resolved against the config file's own directory
  (there is always one — an analysis lane refuses a tree with no config). **This means a first run
  creates a directory inside the tree you point zzop at.** How many directories that is follows from that base: one config
  file means ONE cache directory however many trees it declares (`trees: "auto"` included — the default
  is resolved once, against the config's own directory, then shared by every tree). A multi-PATH run (`zzop cross a b c`, or the MCP `paths`
  argument) has no single base — each path carries its own config and is its own analyzed root, so each
  gets its own `.zzop/`. Ignore it with `**/.zzop/` in that repo's
  `.gitignore`: the leading `**/` is what covers those per-path directories, which a root-anchored
  `/.zzop/` leaves unignored — and never with a `zzop*` glob, for the reason
  [On-disk layout](#on-disk-layout) gives above. The whole `.zzop/` tree is derived state — deleting it
  costs you a warm cache and nothing else.
- **An embedder calling `zzop-facade`/`zzop-engine` directly**: no default is injected at all. Omitting
  `cacheDir` runs uncached, and nothing is ever written into the caller's tree unasked. Default
  injection is the product front end's job, never the library's — see
  [modules/facade.md](modules/facade.md#defaults-a-config-is-required-what-it-does-not-have-to-say).

**Turning it off** is the same key: a JSON-falsy `cacheDir` disables caching entirely and writes
nothing. `null` is the canonical spelling. `""` is accepted as the same intent rather than taken
literally — read literally it would resolve to the base directory *itself* and scatter cache entries
across the repo root, which is not what "off" meant.

Cache hit/miss counters are not part of what a `zzop`/`zzop-mcp` reply carries: the `cache` field is a
raw-facade-only diagnostic (one of several fields the shaped summary drops — see
[`docs/contracts/surface-parity.json`](contracts/surface-parity.json)). A repeated run being faster, or
producing an identical answer, is not by itself evidence the cache was read.

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

A URL resolved from a constant declared **in the same file** covers two positions. The **leading slot** —
`` fetch(`${BASE}/joke/${category}`) `` or `axios.get(BASE + "/users")` — reads a plain string-literal
constant; the **whole argument** — `fetch(URL)`, `fetch(url, { method })` — reads whatever that name's
initializer itself resolves to, exactly **one hop** (`` const url = `${BASE}/x` `` resolves; `const url =
other`, `const base = process.env.X`, `const base = apiBase()` do not). Both stand on the same never-guess
gates: the name is declared by a `const` or `let` (a `var` is not accepted) **at any nesting depth**, it is
bound **exactly once in the whole file** (any redeclaration, parameter, destructuring element, import, or
function/class/enum/namespace of that name disqualifies it), and it is **never reassigned**. Nesting is
deliberately not a gate — "bound exactly once in the file" is what carries the scope argument, so a
function-local `const url = …` qualifies exactly like a top-level one. A parameter or prop
(`fetchJson(url)`) is interprocedural value resolution and stays unresolved. Any failure leaves the call
exactly as it was: an opaque slot, or an unresolved consume. This matters most when the constant is an absolute URL: without it, the opaque head is dropped and
a third-party call is filed as an internal route (or, when nothing literal survives, as an all-placeholder
`GET /{}` key), so reading a visible same-file constant is what lets such a call land in
`externalConsumes` where it belongs. Only the leading slot is read — a mid-path interpolation is a route
parameter and `{}` is its correct normalization — and it is read only when the text immediately after it
is itself visible literal text, so a base glued straight onto a dynamic value (`` `${BASE}${path}` ``)
stays unresolved rather than inventing a half-literal segment. Cross-file constants, environment
variables, and deployment-supplied bases are deliberately out of scope and stay unresolved; they enter by
injection (`hosts`/`mounts`, adapter overlays), never by inference.

The join itself carries four integrity gates on top of the raw `(kind, key)` match:
- **Route identity**: when a consume key matches no provide, WHERE it lands depends on whether the key
  names a route at all. A key whose every path segment is the opaque `{}` placeholder (`GET /{}`, the
  head-drop artifact of an unresolved `${BASE}` interpolation) carries no route identity, so its failure
  to match is an extraction gap, not a missing contract: it goes to `unresolvedConsumes` (counted by
  `cross-layer/unresolved-consume-ratio`) rather than `unprovidedConsumes`, which would assert an
  internal contract nobody wrote. A root `GET /` is unaffected — zero segments, but a fully known path.
  The gate sits in the MISS branch only: if some tree really does provide a catch-all for that key, the
  join emits a genuine edge. The single predicate is `zzop_core::key_carries_route_identity`, shared with
  the single-tree `unprovided-consume` rule so both surfaces decide it identically.
- **Ambiguity**: a consume key provided by 2+ distinct source trees is not auto-linked — it is reported
  separately with every candidate provider listed, rather than picking a winner. Multiple providers for
  the same key *within one tree* (e.g. a tree legitimately exposing something twice) are unaffected and
  still join normally.
- **External egress**: a consume key carrying a host (containing `://`) is treated as third-party egress
  and never cross-tree joined, so an unmatched call to someone else's API isn't reported as drift.
- **Low confidence**: an edge whose key matches an injected "generic path" pattern (e.g. `/health`, which
  many unrelated services legitimately share) is still emitted, but tagged so a consumer can discount it.

A per-tree deployment-topology declaration (config: `trees[].topology`; embedder request fields stay flat — see
[modules/facade.md](modules/facade.md#functions)'s `AnalyzeRequest` field table) supplies the one class of join
information that lives only in infra, not in either repo's source: a gateway/ingress mount prefix, and
which hosts a tree owns. Mounts apply as the last provide-key transform, stacking on top of any
code-extracted prefix (e.g. NestJS's `setGlobalPrefix`); a declared host re-keys a matching absolute-URL
consume to an internal joinable key before the external-egress gate above ever applies. Both self-disclose
via a `warnings` entry when they turn out to have zero effect on the join.

The same declaration answers the CALLING side. `trees[].topology.clientBase` states the prefix a tree's own
outbound calls carry — the case where the base is real but unreadable, assigned from a cross-file constant
(`axios.defaults.baseURL = settings.baseApiUrl`) that the never-guess extractor leaves alone, so the calls
key `GET /articles` while the provider serves `GET /api/articles` and *nothing* joins. It is prepended to
every keyed relative http consume of the tree and, unlike a mount, **warns when it stacks** on a base zzop
already read from a literal: on the serving side a second prefix is a real second layer (a gateway sits
outside the app), while on the calling side it is usually the same base declared twice.

Routing is resolved from **visible code literals on two axes — path and HTTP method (verb)**. A dynamic
route on either axis is an injection boundary, never guessed. A computed/opaque URL path stays an
unresolved consume (surfaced as a near-miss with a "verify manually" caveat); a route whose handler serves
*any* method (a `pages/api` catch-all, a `pathname`-dispatch or Go `HandleFunc` block naming no method
literal) is emitted as a single verb-unknown route and disclosed via `cross-layer/unknown-verb-route`
rather than inventing a `{GET, POST}` pair. A route zzop cannot resolve from source this way — a
verb-unknown handler, a non-literal path (`@GetMapping(ApiPaths.USERS)`), or a computed client URL — is
completed by **injecting the concrete route fact**: either a full Normalized AST adapter overlay, or, for a
handful of routes, the lightweight per-tree `routes: [{ key, role }]` declaration (see
[modules/facade.md](modules/facade.md#functions)'s `AnalyzeRequest` field table), which expands into a
synthetic overlay and joins through the identical path. **Deployment-config routing is the same boundary**: zzop does **not** read
deployment config files (`next.config` `rewrites`/`redirects`, `vercel.json`, nginx/ingress). A uniform
gateway/ingress prefix or host is injected via the `trees[].topology` declaration above; an
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
