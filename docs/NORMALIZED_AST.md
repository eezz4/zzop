# Normalized AST contract (external parser protocol)

External/custom parsers (Ruby, JSP, anything the engine does not parse natively — see
`docs/ARCHITECTURE.md`'s "Language support" section for which languages already have a native parser
and what each one extracts; that table is the only copy of that list) join the
analysis by producing this serialized projection per source tree. The engine never sees their real
AST — it consumes exactly the structures below, projects them into the Common IR, and runs every
language-neutral analysis (dep graph, dead code, scores, cross-layer join, DSL rules) unchanged.

This is deliberately the SAME shape the native parsers project internally: a parser is "first class"
regardless of how crude it is, as long as its projection is accurate (see the cross-layer design note:
the linker is an exact join on normalized keys, never AST matching).

## Envelope

```json
{
  "format": "zzop-normalized-ast",
  "version": "0.29.0",
  "parser": "<parser id>/<impl version>",
  "source": "<tree/source id>",
  "files": [ <FileProjection> ]
}
```

- `version` is the zzop RELEASE whose envelope shape these bytes conform to, as `MAJOR.MINOR.PATCH` —
  the same units `zzop version` prints, NOT an independent counter. **Emit `"0.29.0"`**, the current
  contract version. A consumer accepts anything at or below its own package version and rejects
  anything newer — same "reject newer, never guess" policy as the DSL pack `schema_version`.

  It moves only when the SHAPE moves, so an adapter that emits `"0.29.0"` keeps being accepted through
  every later release that did not change the shape; you do not re-emit it every release. It moves for
  an ADDITIVE field too whenever an engine that silently drops that field would produce a different
  analysis than one that honours it — `overrides` is exactly that case (see "The `overrides` version
  floor" below). Additive fields an older engine can safely ignore do not move it. A new VARIANT on one
  of the externally-tagged fragment enums moves it as well, and less arguably than any field: nothing
  drops an unknown variant, an older engine rejects the whole envelope at deserialization. `0.29.0` is
  such a move — `RouterMountEntry` gained `MountRef` (below).

  A bare integer (`"version": 1`) is what adapters written before 0.27.0 emitted; it is now rejected as
  malformed rather than coerced. This is a one-time break, taken deliberately under the `0.x` no-
  backward-compatibility promise in [`VERSIONING.md`](../VERSIONING.md) — the fix is a one-line edit to
  the adapter, and it buys a reader one version system instead of two.
- `parser` is the producer's self-identification. Every warning that attributes an overlay action to
  its adapter quotes it (the displacement and overruled-binding disclosures, the source-mismatch and
  zero-fact self-reports), and it is the deterministic ordering key when several overlays merge — so
  keep it stable and distinguishable, and move the `/<impl version>` half when your extraction logic
  changes so a reader can tell which adapter build produced an envelope. It is NOT a cache
  ingredient, and bumping it invalidates nothing: envelopes are never served from cache (measured
  2026-07-31 — Mode A analysis is uncached end to end, and a Mode B overlay applies after the cached
  per-file pass), so a changed projection is always honored with or without a bump. This bullet used
  to instruct the opposite; the instruction had no reader.

## FileProjection

```json
{
  "path": "relative/slash/path.ext",
  "loc": 123,
  "symbols": [ <SourceSymbol> ],
  "imports": { "<localName>": { "specifier": "...", "original": "...", "deferred": false, "type_only": false } },
  "re_exports": [ { "specifier": "...", "original": "...", "local_alias": "..." } ],
  "dynamic_imports": ["./lazy-module"],
  "used_names": ["identifiersReferencedLocally", "..."],
  "io": { "provides": [ <IoProvide> ], "consumes": [ <IoConsume> ] },
  "const_map_fragment": { "<dotted.const.KEY>": "<literal-string-value>" },
  "procedure_router_fragments": [ <ProcedureRouterFragment> ],
  "router_mount_fragments": [ <RouterMountFragment> ],
  "class_shape_fragments": [ <ClassShapeFragment> ],
  "degraded": false,
  "is_entry": false,
  "overrides": { "imports": ["localNameWhoseNativeBindingThisReplaces"] },
  "loop_spans": [[10, 14]],
  "function_spans": [[4, 20], [9, 12]],
  "test_spans": [[30, 48]],
  "calls": [ { "from_symbol": "relative/slash/path.ext#handler", "callee_name": "verifyToken", "line": 7 } ],
  "attributes": [ <Attribute> ]
}
```

Field semantics (all mirror the Rust `zzop-core` serde types — those are the normative schema):

- `loc` — raw physical line count (`text.split('\n').length` semantics, trailing newline adds one).
- `symbols` — declarations. `body_start`/`body_end` (1-based, inclusive) power method-scan DSL rules;
  a parser that cannot produce spans omits them and those rules silently skip the file (graceful
  degrade, never an error). A lexical brace-matcher is an acceptable span source (zzop's own Java
  projector worked exactly this way before its tree-sitter CST upgrade).
- `imports` — internal dependency edges are derived from these by the engine's resolver; a parser may
  instead pre-resolve and emit repo-relative specifiers.
- `dynamic_imports` — OPTIONAL (`#[serde(default)]`; absent = empty), this file's dynamic-`import()`
  specifiers. Mirrors the native `FileArtifact::dynamic_imports` — folded into the envelope dep graph
  as real (circular-excluded) edges, so a code-split-only module still gets fan-in credit on the
  envelope path.
- `used_names` — identifiers referenced in the file body (feeds symbol-level dead-export analysis).
- `io` — the cross-layer contract: each provide/consume carries `(kind, key)` where `key` is fully
  normalized by the parser (`"GET /users/{}"`, `"table:users"`). Unresolvable dynamic keys are `null`
  (unresolved), never guessed. Key normalization quality is the parser's whole responsibility and the
  only thing cross-layer accuracy depends on. The normative HTTP key-normalization rules live in
  `crates/core/src/io.rs`'s `http_interface_key`/`http_consume_interface_key` (provide-side vs.
  consume-side asymmetry, path-param collapse, slash/method normalization); the byte-exact,
  language-agnostic parity contract for reproducing them is
  [`adapters/key-normalization.fixture.json`](adapters/key-normalization.fixture.json) (see
  [`adapters/README.md`](adapters/README.md) for how to replay it), the machine-checkable shape is
  [`adapters/envelope.schema.json`](adapters/envelope.schema.json), and a ready-made JS implementation
  of both is [`examples/adapters/adapter-kit/`](../examples/adapters/adapter-kit/)'s `lib/keys.js`. **Absolute-URL keys
  (`"://"` present) are the one case that must NOT go through that normalization at all** — see
  `adapters/README.md`'s "Absolute URLs bypass normalization entirely" section.
  - **Self-calls / loopback.** A service calling ITSELF over HTTP (a smoke-test script, a health-check
    hitting its own base URL) is still an absolute URL if written as `http://localhost:8080/api/ping` —
    the bypass rule above applies by string shape (`"://"` present), not by "is this actually a
    third-party host." Left as-is, that consume keys as the raw URL, lands in `externalConsumes`, and
    never joins the tree's own `GET /api/ping` provide. Two fixes: (1) at the ADAPTER, strip the origin
    and key it as the plain relative path (`"GET /api/ping"`) like any other in-tree call — the right
    call when you control the extraction; or (2) at the CONFIG, declare the tree's own host in its
    `hosts` list (a bare hostname, no scheme/path) — cross-layer linking then re-keys any absolute-URL
    consume targeting a declared host to its internal joinable key at link time instead of routing it to
    `externalConsumes` (see `docs/modules/facade.md`'s `hosts` field and `hostRekeyCounts`).
  - OPTIONAL client provenance (additive since `axios-defaults-base-v1`; omit and nothing changes):
    an `IoConsume` may carry `client: "axios"` naming the HTTP client that produced the call site.
    `client` is a free-form string (`Option<String>` in `crates/core/src/io.rs`), not a closed enum —
    the native TS parser's own recognizers currently tag `"axios"`, `"ky"`, `"fetch"`, `"$fetch"`,
    and `"angular"`, but those are examples of the vocabulary in active use, not an exhaustive or
    enforced list — a generated-SDK injection adapter (e.g. for oazapfts) is free to set its own
    `client` tag. Client-SCOPED normalization seams (e.g. the engine's
    `axios.defaults.baseURL` path-prefix application) act only on consumes tagged with their client
    and leave untagged consumes untouched — an external producer that doesn't tag simply opts out.
  - OPTIONAL request-body evidence (additive since `body-shape-v1`; omit both and nothing changes): an
    `IoConsume` may carry `body: { "keys": ["user", "user.email"], "completeAt": ["", "user"] }` (the
    statically witnessed body-literal key paths, depth ≤ 2, plus which levels are exhaustively
    witnessed) and an `IoProvide` may carry `body: { "subKey": "user", "dtoRef": "CreateUserDto",
    "fields": [{"name": "email", "optional": false}], "complete": true }` — either with `dtoRef` set
    (resolved at assemble time against `class_shape_fragments`, below) or with `fields`/`complete`
    supplied directly and `dtoRef` omitted. Feeds `cross-layer/body-field-drift`; see
    `crates/core/src/io.rs`'s `ConsumeBodyShape`/`ProvideBodyShape` for the normative semantics
    (evidence-only: anything not statically witnessed is omitted, never approximated).
  - OPTIONAL declared-response evidence (additive since `response-shape-v1`; omit and nothing
    changes): an `IoProvide` may carry `response: { "dtoRef": "UserDto", "fields": [{"name": "id",
    "optional": false}], "complete": true }` — the response contract the handler DECLARES via its
    return-type annotation (the native TS parser unwraps `Promise<X>` syntactically; declarations
    only, never inferred from return statements or flow). Same two supply modes as `body`: `dtoRef`
    set (resolved at assemble time against `class_shape_fragments`) or `fields`/`complete` supplied
    directly with `dtoRef` omitted. Feeds `cross-layer/sensitive-response-field`; normative semantics
    in `crates/core/src/io/facts/shapes.rs`'s `ProvideResponseShape` — including the one shape an adapter
    should NOT emit (`dtoRef` omitted AND `fields` empty, the native parser's internal
    "handler declared no return type" sentinel, which the engine strips and discloses).
  - OPTIONAL handler provenance: an `IoProvide` may carry `symbol`, the name of the function that handles
    **this** route. Three native rules start a call-graph BFS from it (`mutating-route-no-auth`,
    `unsafe-read-endpoint`, `non-idempotent-write`) and `duplicate-route` compares it for equality, so a
    dispatch-style router that emits ONE shared symbol for N routes makes all N share their reachability:
    one route's auth guard silences the others, one route's write makes every sibling look mutating.
    **Emit the per-route handler when you can name it unambiguously; omit it (or `null`) rather than emit a
    shared enclosing-function name** — an absent symbol makes the BFS rules skip the route, which is honest,
    while a shared one is silently wrong in both directions. (The in-repo TypeScript adapter learned this the
    hard way: see `parser/parser-typescript/src/adapters/pathname_dispatch.rs` for the exact predicate it
    uses to decide when a dispatch branch is attributable.)
    **Which of those four your `symbol` actually reaches depends on the language, and for most external
    producers it is only `duplicate-route`.** That one is an equality compare and is language-neutral. The
    three BFS rules are not, and the limit is structural rather than a gap someone forgot to close: only
    some of this workspace's parsers produce `RawCall`s at all — today the TypeScript, Java, Python and
    Rust ones (`crates/engine/src/analyze/native_rules/callgraph/mod.rs` and the per-language loops it
    delegates to, `python_guard.rs`/`rust_guard.rs`) — whose calls the engine gathers by re-reading and
    re-parsing those files off disk. So `mutating-route-no-auth` exempts a
    route outright — before the BFS, never "no guard found" — unless its file extension is in
    `CALL_GRAPH_COVERED_EXTENSIONS` (in `rules/native/rules-http/src/mutating_route_no_auth.rs` — that
    constant is the authoritative covered set, not the language list above; a run
    that saw routes OUTSIDE it says so in its own `warnings`, naming the language and the silenced rule),
    and `unsafe-read-endpoint` /
    `non-idempotent-write` additionally need `writeSites`, which you may emit yourself (it is in the
    symbol contract below) but which no non-TypeScript parser here fills.
    **In whole-envelope analysis (Mode A) you can supply the missing edges yourself**: the `calls`
    channel (below) is exactly the per-file `RawCall` fact those re-parse loops produce natively, and an
    envelope that carries it gets the same BFS over its own resolved graph — `unsafe-read-endpoint`/
    `non-idempotent-write` for ANY language (they gate on `writeSites` evidence, not extension), and
    `mutating-route-no-auth` for routes whose extension is in `CALL_GRAPH_COVERED_EXTENSIONS` (its
    candidate gate still applies; a Mode A run with supplied calls and routes outside that set names the
    residual in `warnings` rather than letting the exemption read as a verdict). Without the channel, a
    Ruby or JSP route stays invisible to all three no matter how carefully you name its handler — emit
    `symbol` for `duplicate-route`'s sake and for the run where `calls` (or a native `RawCall` extractor)
    exists, not on the expectation of an auth-reachability verdict without edges. Per-rule language
    sightlines: [rules/catalog.md](rules/catalog.md).
- `const_map_fragment`, `procedure_router_fragments`, `router_mount_fragments`, `class_shape_fragments` —
  all four are OPTIONAL
  (`#[serde(default)]`; absent = empty; a projection with none of them is still fully valid and
  non-degraded). They are the envelope equivalent of the fragment channels native in-process adapters
  already project per file, and feed the SAME whole-tree fragment composition
  (`compose_trpc_provides`/`compose_router_mount_provides` in the engine) — an adapter that only knows
  plain `io` facts may omit all three; one that understands a router framework emits them and
  participates in the identical composition as a native parser would.
  - `const_map_fragment` is `identifier -> literal string value` for this file's top-level `const`
    string bindings — the same shape the native adapters' own const-map fragment uses. It feeds late
    cross-file consume resolution: an `IoConsume` with `key: null` but a `raw`/`method` set gets
    re-resolved once some file's `const_map_fragment` supplies a matching key.
  - `procedure_router_fragments` is `[ <ProcedureRouterFragment> ]`, same shape as the native tRPC-router-fragment
    projection: a named router binding plus entries, each either a `Ref` to another router by
    identifier/import-specifier, a `Nested` inline sub-router, or a `Leaf` procedure. See
    `crates/core/src/fragments.rs`'s `ProcedureRouterFragment`/`ProcedureRouterEntry` for the normative field
    names.
  - `router_mount_fragments` is `[ <RouterMountFragment> ]`, same shape as the native Hono-style
    router-mount projection: a named router identifier plus entries, each either a `Verb` registration
    (`{method, path, handler, line}`) or a `Mount` sub-router mount (`{prefix, ident, specifier}`). See
    `crates/core/src/fragments.rs`'s `RouterMountFragment`/`RouterMountEntry` for the normative field
    names.
    - **`MountRef: {prefix_ref, ident, specifier}` — since `"0.29.0"`, and NOT emittable below it** (an
      engine that predates the variant rejects the whole envelope at deserialization; there is no
      silent-drop mode to fall back to, which is why this variant moved the contract version). It is a
      `Mount` whose prefix the producer could SEE but not READ — FastAPI's
      `include_router(api_router, prefix=settings.API_V1_STR)`, where the value lives in another file.
      `prefix_ref` is the argument's verbatim text (`"settings.API_V1_STR"`), resolved at assemble time
      against the same project-wide merged const map `controller_prefix_route_fragments`' `prefix_ref`
      uses. Emit `Mount` whenever the prefix IS a literal: a producer must not be able to express
      "literal AND reference", because the composer would then have to pick, and picking is guessing.
      On an unresolvable reference, from `crates/core/src/fragments.rs`:

      > **Never-guess on failure**: a `prefix_ref` absent from the merged map does NOT fall back to `/`.
      > The mount is dropped and disclosed, exactly as the controller-prefix composer already does — a
      > route emitted at the wrong key is worse than a route not emitted, because only the second one
      > looks like the absence it is.

      `attr_keys` behaves exactly as `Mount`'s (below). The floor is its own constant
      (`MIN_VERSION_FOR_ROUTER_MOUNT_REF`), pinned to `0.29.0` and not moved by later contract bumps —
      see "The `overrides` version floor" below for why a floor is per-feature.
    - **Additive, backwards-compatible (`#[serde(default)]`; absent = empty/no-op for both shapes):**
      `Verb` and `Mount` each also accept an `attr_keys: [String]` field, and `RouterMountEntry` gains a
      `ScopedAttr: {prefix, key, line}` variant. All three ride the generic entity-attribute
      channel (`attributes`, above) rather than introducing a new one: each key is open vocabulary (the
      kernel never interprets it) and its composed attribute value is implicitly `true` — there is no
      value field to set. Semantics:
      - `Verb.attr_keys` — one attribute per key, attached to that verb's own composed `IoKey` once
        assembled (an `EntityRef::IoKey`). E.g. a route-level guard argument
        (`router.post('/x', requireAuth, handler)`) yields `attr_keys: ["auth-guarded"]` on `POST /x`.
      - `Mount.attr_keys` — UNRESOLVED-FALLBACK semantics: at compose time, if `ident`/`specifier`
        resolves to another router fragment, this is an ordinary mount and `attr_keys` is ignored (the
        ident named a sub-router, not a guard). If it does NOT resolve, each key instead becomes an
        `EntityRef::PathScope` attribute at the composed prefix — the mount that turned out not to be a
        mount is reinterpreted as a scoped fact instead of silently dropped. This is how a producer emits
        one entry for an ambiguous shape (`.use(prefix, ident)`, where `ident` could be a sub-router or a
        middleware guard) without pre-deciding which it is.
      - `ScopedAttr` — a standalone entry (no `Verb`/`Mount` attached) for a middleware registration that
        never resolves to a mount at all, e.g. `app.use('/admin', requireAuth())` (a call, not an ident).
        Composes straight to an `EntityRef::PathScope` attribute at the composed `prefix` (joined through
        the same router-mount chain as `Mount.prefix`).
      - A producer that only knows plain `Verb`/`Mount` registrations (no attribute judgment) can omit
        `attr_keys` and the `ScopedAttr` variant entirely — existing envelopes and adapters keep working
        unchanged. See `parser/parser-typescript/src/adapters/router_mounts.rs` for the native producer
        that populates these for Express middleware guards, and `crates/engine/src/analyze/compose.rs`'s
        `compose_router_mount_provides` for the composition semantics above.
  - `class_shape_fragments` is `[ {"name": "CreateUserDto", "fields": [{"name": "email",
    "optional": false}], "complete": true} ]` — one entry per class declaration the adapter's language
    can see, the resolution substrate for `IoProvide.body.dtoRef` (above): at assemble time the
    tree-wide merge resolves each `dtoRef` by class name, dropping unresolvable or cross-file-conflicting
    names with a warning rather than guessing. `complete: false` means the field list may be partial
    (inheritance, mixins, index signatures — whatever the source language's equivalent is). See
    `crates/core/src/fragments.rs`'s `ClassShapeFragment`.

  **Contract note — specifier resolution is exact-match/relative only, never alias-aware.** A tRPC
  `Ref`'s `specifier` and a router-mount `Mount`'s `specifier` must resolve to either (a) another file's
  `path` exactly as that file emits it in THIS SAME envelope's `files[]` (an exact repo-relative string
  match), or (b) a `./`/`../`-relative path resolved from the EMITTING file's own directory (with
  `.ts`/`.tsx`/`.js` extension-guessing as a fallback when the raw join misses). An external adapter
  controls both sides of this reference — it emits both the fragment and every file's `path` — so a
  full-envelope analysis (`analyze_envelope`, Mode A) never applies tsconfig/workspace-alias
  resolution to fragments. (Adapter OVERLAYS, Mode B, compose alongside the native tree and inherit
  its alias-aware resolver — a superset; rely on the exact/relative contract above for portability
  across both modes.) `validate_envelope` does not check fragment
  specifier resolvability at all: empty or unresolvable fragments are always valid JSON-shape-wise, and
  an unresolvable `Ref`/`Mount` is silently skipped at COMPOSITION time, never a validation error — the
  same "never guess" convention this doc already documents above for `io` consume keys.
- `degraded` — the parser could not fully process the file (size cap, syntax failure); `loc` must
  still be present.
- `attributes` — OPTIONAL (`#[serde(default)]`; absent = empty). The generic entity-attribute channel:
  open-vocab cross-cutting facts a producer attaches to entities that per-file extraction can't see. Each
  is `{ "target": <EntityRef>, "key": "<producer/rule vocab>", "value": <any JSON> }`. `EntityRef` is an
  externally-tagged, camelCase enum: `{ "file": { "path" } }`, `{ "symbol": { "name", "file"? } }`,
  `{ "ioKey": { "kind", "key" } }`, or `{ "pathScope": { "prefix" } }` (a route-prefix scope; longest match
  wins). Rules consume by key — the contract is agnostic to what `key` means. First consumer:
  `mutating-route-no-auth` reads `key: "auth-guarded"` on a route's `ioKey`/`pathScope` to clear a route
  guarded by middleware the call-graph can't see. Second consumer: `cross-layer/retrying-write-no-idempotency`
  reads `key: "idempotency-guarded"` on a route's `ioKey`/`pathScope` — set natively by parser-typescript's
  inline-handler recognizer when a handler reads the `Idempotency-Key` header (Express/Hono, TS only), or
  injected otherwise — to veto a retry-triggered finding once a guard is witnessed. See
  `docs/adapters/envelope.schema.json` `attribute`/`entityRef`.
- `is_entry` — OPTIONAL (`#[serde(default)]`, default `false`). Marks this file a framework/runtime
  ENTRY loaded by convention rather than imported (a SvelteKit `hooks.*`/`+page`, a `.vue` route, ...),
  so zero in-repo importers is expected, not dead-code signal — the overlay counterpart of a
  package.json manifest entry. Read in BOTH modes, for the `dead-candidates` analysis only:
  - `analyzeEnvelope` (Mode A) builds that analysis's ENTIRE entry set from the envelope's own
    `is_entry: true` projections — there is no filesystem root, so there are no package.json entries to
    union with (`crates/engine/src/envelope/ingest.rs`).
  - Mode B (adapter overlays, below) unions the `is_entry: true` paths of every APPLIED overlay into the
    native `dead-candidates` entry set. APPLIED, not declared: the paths are recorded inside the apply
    loop, past the `validate_envelope` gate, so an overlay that was REJECTED exempts nothing
    (`crates/engine/src/envelope/overlay.rs` → `OverlayApplication::entry_paths` →
    `crates/engine/src/analyze/assemble/rules.rs`'s `overlay_entry_paths`).

  It does NOT exempt a file from `unreachable`, in either mode: that analysis's entry set is seeded from
  declared cargo targets plus SFC-only and runtime-asset import targets, and `is_entry` is not among
  them.

- `overrides` — OPTIONAL (`#[serde(default)]`, default `{}`), and the one channel that DISPLACES a
  native fact rather than adding to it. Requires the envelope to declare `version` >= `"0.27.0"`; see
  "The `overrides` version floor" below for why that floor is per-feature. One sub-field today:

  - `overrides.imports` — an array of LOCAL NAMES (keys of this projection's `imports` map) whose
    native binding this overlay replaces. Each listed name MUST also appear in `imports`: the
    replacement is mandatory. A name listed without one is a deletion request, and deletion is not
    offered — it has no honest output form (there is no replacement fact to disclose) and an adapter
    that can delete can blind the engine without leaving a trace. Listing a name twice is rejected.

  **Why a declaration, rather than "the overlay's value wins on a key collision".** Measured on
  `examples/adapters/override-required/`: an adapter correcting a wrong native import did not collide
  with it at all. Python binds `import util.config` under the local name `util`, so an adapter that
  keys its binding `util.config` — the specifier, which reads like the obvious key — lands as a
  SIBLING entry and both the right and the wrong edge survive. Priority-on-collision cannot express
  overriding, because the displacing side often does not know it is colliding. The displacing fact has
  to NAME what it displaces.

  **What the run says.** Every displacement is reported in `warnings`, naming the file, the local
  name, the native specifier and the replacing one — an override is the only overlay operation that
  removes something the engine extracted itself, so that line is the only record the native fact was
  ever there. Nothing verifies that the adapter is the correct side; the disclosure exists so a reader
  can disagree.

  The mirror is reported too: an overlay binding DROPPED because the native pass held the same local
  name with a different specifier and no override was declared. Parsed facts win by default, but
  losing the adapter's side in silence is the same defect pointing the other way — without that line,
  an author who misspells or forgets a declaration sees a run indistinguishable from success. An
  overlay restating a binding the native pass already holds is agreement, not a loss, and is silent.

### The `overrides` version floor

The floor (`"0.27.0"`) exists for exactly one reason: `FileProjection` has no
`additionalProperties: false`, so an `overrides` key handed to an engine built before this field
existed would deserialize, be ignored, and produce a run where the adapter believes it displaced a
native fact and the engine quietly did not — the same silent-loss shape the displacement disclosure
abolishes, reintroduced one level up in the contract itself.

Two DIFFERENT checks close it, and neither substitutes for the other:

- The **ceiling** ("reject anything newer than me") catches an honest envelope: it declares `"0.27.0"`,
  an older engine sees a version above its own and refuses the whole thing.
- The **floor** catches a MISLABELLED one: it declares `"0.20.0"` while carrying `overrides`. The
  ceiling accepts that happily — it is older, not newer — so only the floor rejects it. The engine that
  understands the field is the only one positioned to notice, which is why the current engine is the
  one that says so, at authoring time, instead of letting you ship bytes that mean different things to
  different engines.

The floor is PER-FEATURE, not "the contract version". It is a separate constant
(`MIN_VERSION_FOR_OVERRIDES` in both `crates/core/src/normalized.rs` and the adapter kit), and since
`0.29.0` moved the contract version for `MountRef` the two no longer coincide — `overrides` still
requires only `"0.27.0"`, because a floor pins to the release that introduced its feature and would
stop being a floor if a later bump dragged it along. `MountRef` has its own
(`MIN_VERSION_FOR_ROUTER_MOUNT_REF`, `0.29.0`), and so does `calls` (`MIN_VERSION_FOR_CALLS`,
`0.29.0` — a silently-droppable field whose silent drop is a RECALL loss, the same class as
`overrides`). A field that an older engine can safely ignore gets no floor at all.
- `loop_spans` — OPTIONAL (`#[serde(default)]`; camelCase `loopSpans` also accepted on input). `[[startLine,
  endLine], ...]`, 1-based and inclusive. Each pair is either a loop statement's whole span (`for`/
  `for-in`/`for-of`/`while`/`do-while`, header line included) or an EAGERLY-evaluated
  callback/comprehension body's span (e.g. a `.map`/`.forEach` callback argument — the callback only,
  never the whole call). An adapter must NOT emit a span for a lazy form (generator expression, lazy
  iterator/stream/LINQ chain): the contract is "a call inside this span provably runs once per
  iteration", and a lazy body runs zero times unless consumed. An adapter must also NOT emit a
  callback/comprehension span whose start and end line are EQUAL — line-granular containment cannot
  separate a one-line callback body from the one-shot calls sharing that line, so single-line
  eager-callback spans are dropped (intended under-reporting; statement-loop one-liners are kept,
  a published residual ambiguity). Feeds
  `MethodScan::trigger_in_loop`; absent means no structural loop facts for this file, and that matcher
  silently skips it (graceful degrade, same convention as `symbols`' `body_start`/`body_end`).
- `function_spans` — OPTIONAL (`#[serde(default)]`; camelCase `functionSpans` also accepted on input).
  `[[startLine, endLine], ...]`, 1-based and inclusive: one pair per function-like node — `function`
  declarations and expressions, arrows, class and object-literal methods, constructors, accessors.
  Nested functions overlap freely (an outer function's span contains its closures'); consumers resolve
  the INNERMOST containing span. Order is node source order, NOT sorted by start line.

  **The merge rule.** A function-shaped ARGUMENT of a `.then(...)`/`.catch(...)`/`.finally(...)` member
  call has its span START pulled up to the line of that call's property token, so a promise
  continuation and the boundary that schedules it share one span. This changes the span ONLY when the
  callback opens on a LATER line than the `.then` token — the shape a printer produces once the
  argument list breaks:

  ```ts
  loadRates().then(    // line 1  <- the property token, and the merged span's START
    (d) => {           // line 2  <- the callback's own start line
      setRates(d);     // line 3
    },
  );                   // line 5
  ```
  A producer must emit `[1, 5]` for that callback, not `[2, 5]`. In the far commoner one-line spelling
  (`loadRates().then((d) => {`) the token and the callback already share a line, so the merge is a
  no-op and a naive emitter happens to be correct — which is exactly why the broken-argument-list case
  is easy to miss. Only the callback's start moves; the RECEIVER is never swept in, so a multi-line
  receiver (`loadRates()` on its own line above `.then(`) stays outside. Only those three method names,
  only as a member-call property — `.map`, `setTimeout`, `useEffect`, an aliased `const t = p.then`, or
  a bare `then(cb)` are all left unmerged. That narrowness is deliberate: a wider merge would re-join
  the sibling closures this fact exists to separate.

  **Why it exists / what a producer that omits it loses.** It is the substrate of
  `MethodScan::after_in_same_function`, which requires a rule's ordering match (`after`) and its trigger
  match to sit in the SAME innermost function rather than merely in the same declared symbol's body —
  the difference between "this component `await`s somewhere and sets state somewhere" and "this
  continuation sets state after ITS OWN boundary".

  **Absent-field behavior differs from `loop_spans`, deliberately.** With no spans every line resolves
  to "no enclosing function", so all lines count as the same function and the gate becomes a NO-OP: a
  rule using it keeps its coarser whole-symbol behavior and stays as loud as it was. It does NOT go
  silent the way `trigger_in_loop` does on a file without `loop_spans`. The reason is directional: this
  gate only ever REMOVES pairings, so degrading to "remove nothing" preserves coverage, whereas
  degrading to "skip the file" would delete it. An adapter author who omits this field therefore loses
  precision (the false positives the gate removes come back), never recall.

  **A PARTIAL list degrades identically, per LINE.** The resolution is per trigger line, so a line your
  spans happen not to cover reads as "no gate on this line" — pre-gate scope — and never as "no pair".
  Emitting a subset is therefore safe in the same direction as emitting none: you can leave the coarser
  over-report standing, you can never silence a finding. The native TypeScript parser hits this too (a
  class-body top-level line, e.g. a property initializer, is inside no function span), so it is a real
  line-level state and not an adapter-only edge case.

  **Language coverage today: TypeScript only** among zzop's native parsers — every structural
  statement-loop parser (TypeScript, Go, Python, Java, C#, Rust) produces `loop_spans`, but only
  TypeScript produces `function_spans`. That asymmetry is published
  here rather than left implicit; see `crates/cache/src/ir_slice.rs`'s module doc for the full per-fact
  coverage note.

- `test_spans` — OPTIONAL (`#[serde(default)]`; camelCase `testSpans` also accepted on input).
  `[[startLine, endLine], ...]`, 1-based and inclusive: regions your parser **proved** are compiled out
  of the shipping build. This is the only SUBTRACTIVE field on `FileProjection` — every other one lets a
  rule say more; this one stops DSL rules from judging the lines it names. Declare it only for regions you
  can prove: a span over shipped code silently deletes real findings, and there is no second gate behind it.

  **Exactly what it reaches**, because two of the edges matter to a producer:
  - The PER-FILE matchers — `line-scan`, `method-scan`, `symbol-scan` — whose findings are dropped after
    the matcher runs.
  - **Except** rules that declare `scan_test_regions` (see `docs/rules/dsl-reference.md`). That opt-out
    exists for credential-at-rest rules, where the COMMIT is the leak: a PEM private key or a
    `scheme://user:pass@host` URL inside a test region is still in git history and still has to be rotated,
    so those rules keep judging your spans on purpose.
  - **NOT `io-scan`**, which queries the assembled whole-tree IO facts — the same facts the cross-layer
    join, the coverage census and the endpoint verdict read. So a producer declaring a span must also
    WITHHOLD the `io` provides/consumes it extracted from inside that span. Emitting a test-only route and
    relying on this field would hide only the rule findings while leaving the route in the join.

  Absent (or empty) means nothing is subtracted, so an envelope that omits it keeps its full judgment.
  That is the safe direction for a subtractive fact and the reason this field needs no version floor.

  **Path-named test files are not this field's job.** A `foo.test.ts` or a `tests/` directory is already
  excluded by the rule packs' own `${test-paths-stories}` `file_exclude_pattern`, on the path, before any
  projection is consulted. This field exists for the case a path cannot express: a test region **inside**
  a shipping file.

  **Language coverage today: Rust only** among zzop's native parsers (`#[cfg(test)]` and the
  `#[test]`/`#[tokio::test]`/`#[sqlx::test]` attribute family), because Rust is the only language here
  whose dominant test convention lives inside the shipping file. A blank for another language is a
  statement that its path axis suffices, not a gap.

- `calls` — OPTIONAL (`#[serde(default)]`; snake_case only, the field is new so no frozen camelCase
  alias exists). `[ { "from_symbol", "callee_name", "line", "receiver_type"?, "is_heritage"? } ]` —
  per-file CALL-GRAPH EDGES, the external-parser counterpart of the `RawCall` sites native parsers
  project (`crates/core/src/callgraph.rs`, the same serde type verbatim). This is the channel that lets
  a producer whose language has no native call-graph parser turn on the call-graph-BFS rule family in
  whole-envelope analysis (Mode A): `mutating-route-no-auth`, `unsafe-read-endpoint`,
  `non-idempotent-write`. **Requires `version` >= `"0.29.0"`** (`MIN_VERSION_FOR_CALLS` — same
  per-feature-floor logic as `overrides`; an older engine drops the field silently and its call-graph
  rules stay quiet, so the engine that understands it rejects the mislabel).

  - `from_symbol` — `"<this file's path>#<symbol>"`, the call site's enclosing top-level symbol.
    `validate_envelope` REJECTS any other file's prefix (or a missing `#<symbol>` tail): the engine
    buckets calls by this prefix and resolves them against that file's own `imports`, so a foreign
    prefix would mint an edge under an attribution the producer never controlled.
  - `callee_name` — the target identifier exactly as written (empty is rejected). Resolution is the
    ENGINE's job, per-file extraction is yours: an imported name resolves through this projection's
    `imports` map (specifiers under the same exact-match/`./`-relative contract as the fragment
    channels above), a same-file name through the file's own `symbols`, and anything resolving through
    neither is DROPPED — an external/global call, normal and never an error, never a guessed edge.
  - `receiver_type` (optional) — for `recv.method()`, the class name of `recv` when it is a
    typed/imported class receiver; lets the engine emit a `<file>#<Class>.<method>` edge.
  - `is_heritage` (optional, default `false`) — a class `extends`/`implements` edge; `callee_name` is
    then the super class/interface name.

  **Degrade direction is RECALL, and absence is disclosed.** An empty/absent channel means those three
  rules are structurally silent for this envelope — they did not look, they did not report clean. A
  Mode A run whose envelope carries `http` routes but no `calls` says so in `warnings` (naming the
  silent rules and this channel); one whose routes sit outside `mutating-route-no-auth`'s
  `CALL_GRAPH_COVERED_EXTENSIONS` names that residual gate even when calls were supplied. What each
  rule additionally needs: `unsafe-read-endpoint`/`non-idempotent-write` only fire when the BFS reaches
  a symbol carrying `writeSites` (emit those on your `symbols`); `mutating-route-no-auth` needs the
  route's `symbol` handler reference (above) to resolve. Guard knowledge the graph cannot express
  (route-level middleware) is injected as an `auth-guarded` attribute, exactly as on a native tree.
  The `// idempotent-ok:` suppress-marker window is inert in Mode A (no source text), failing toward
  firing — the same posture as the anchor-line channels in the Validation section below.

  **Mode B (adapter overlays) does not consume this channel today.** The native pass re-parses the
  tree's own dispatched sources for call edges; an overlay carrying `calls` gets one aggregate warning
  naming the ignored channel rather than a silent no-op.

### Channels deliberately NOT on this wire

Two per-file channels native parsers project have **no envelope counterpart**, and each absence is a
decision with its own reason, stated here so a producer does not read the gap as an unfinished field:

- **Call SITES** (`zzop_core::CallSite`, `Matcher::CallScan`'s substrate — "this file used API family X
  at line N"). The `calls` channel above is NOT that: those are call-graph EDGES, a different fact
  category with a different consumer. Opening a per-file call-site channel on this contract is its own
  additive, version-gated change. Consequence: a `call-scan` rule is silent on every envelope-projected
  file — the same recall-side degrade a language with no native producer gets.
- **Bound string literals** (`zzop_core::BoundStringLiteral`, `Matcher::LiteralScan`'s substrate —
  binding name + value hash + value entropy, never the value). Here the boundary is PRIVACY, not only
  additive-change discipline: an envelope is an external submission, and the channel carries an
  unsalted 64-bit FNV hash of every candidate SECRET in the tree — offline dictionary-crackable for
  exactly the low-entropy credentials the consuming rule (`security/high-entropy-secret`) exists to
  catch. The no-plaintext contract (`zzop_core::string_literals`'s module doc) keeps hash + entropy
  inside the LOCAL analysis cache; putting them on the wire would launder that contract through the
  submission channel. If this channel is ever opened, that judgment — not a plumbing change — is what
  has to be revisited, versioned, and disclosed. Consequence: a `literal-scan` rule is silent on every
  envelope-projected file, same degrade as `call-scan` above.


## Delivery

One process invocation per TREE (a file list in, one envelope out) or a daemon mode — never one
process per file (see the batching decision: JVM-style startup times multiplied by thousands of files
must not dominate wall time). The engine feeds the envelope through the same per-file pipeline as
native parsers: DSL rules run against `symbols` + source lines, whole-graph passes run on the merged
IR.

## Validation

A conforming producer can be checked against the Rust types by round-tripping through `zzop-core`
serde (`CommonIr`/`SourceSymbol`/`ImportBinding`/`ReExport`/`IoFacts` all derive `Deserialize`).

`zzop_core::validate_envelope(json: &str) -> Result<NormalizedEnvelope, Vec<String>>` is that validator:
beyond plain deserialization, it rejects an unknown `format` string, a `version` greater than
`zzop_core::SUPPORTED_NORMALIZED_AST_VERSION`, an empty or duplicate file `path`, a symbol whose
`body_end` is less than its `body_start`, a populated `overrides`/`calls` below its feature's version
floor, and a `calls` entry whose `from_symbol` is not `"<this file's path>#<symbol>"` (or whose
`callee_name` is empty) — collecting every issue found rather than stopping at the first. The engine-side receiver, `zzop_engine::analyze_envelope(envelope, config) -> AnalyzeOutput`,
projects an already-validated envelope into the same per-file artifact shape a native parser produces
and runs every language-neutral whole-graph analysis over it (see that function's own module doc for
exactly which per-file DSL rules and analyses run in envelope mode, and why line-scan/method-scan rules
and git-history-dependent analyses do not). Envelope analysis also gets the same bundled-rule-pack
default as every other analyze path: the facade entry point (`zzop_facade::analyze_envelope_json`, the
one code path every host drives) seeds the bundled packs as inline `packDefs` unless the config passes
an explicit `packsDir: null` — they appear in the output's `packsLoaded` (`source: "inline"`; the
removed JS wrapper's bundled packs used to report `"dir"` instead, since its on-disk bundled copy won
the id collision), and since only
`symbol-scan`/`io-scan` rules can fire without source text, what the default actually contributes is the
two bundled `io-scan` rules — `http/protected-path-no-auth-evidence` and `http/dev-path-no-guard-hint` —
plus pack-load confirmation
for every other bundled rule (no bundled rule uses `symbol-scan` today). Those two do fire in Mode A,
with one documented deviation from the same rules on a native tree: `analyze_envelope` supplies an
anchor-line lookup that always returns `None`, because an envelope carries no source text to look one up
in. Every channel that reads the anchor line is therefore inert here — the derived `zzop-<rule-id>-ok`
suppress marker, its near-miss disclosure, and `dev-path-no-guard-hint`'s `anchor_exclude_pattern` guard-hint
carve-out. All of them fail toward FIRING, never toward silence: a matching route reports even when its
registration line would have carried a marker or a guard-hint argument, rather than the engine guessing
at line text it does not have. Clear a vetted route in Mode A by injecting the attribute the rule reads
(`auth-guarded` for `protected-path-no-auth-evidence`) or by disabling the rule in config. Mode B
overlays are unaffected —
they merge onto a natively-parsed tree whose source text is readable off disk, so both channels stay
live there even for a language with no native parser. A
caller-supplied pack reusing a bundled id keeps the existing collision semantics (a later inline def,
or any directory pack, wins whole). See `docs/modules/facade.md`'s "Defaults" section for the full
contract. `docs/contracts/example-envelope.json` is a hand-written,
crude-parser-shaped fixture (symbols with no body spans, one `http` provide, one `db-table` consume, no
imports) that validates cleanly against this contract — see `zzop-core`'s `normalized::tests::
jsp_contract_example_validates` for the fixture-based check. A JSON Schema export for this contract
already ships at [`adapters/envelope.schema.json`](adapters/envelope.schema.json), derived field-for-field
from the same Rust serde types this document describes — see `adapters/README.md`'s "Envelope schema &
versioning policy" section for how it tracks this document.

## Casing

Casing is not uniform across the envelope, and which part you get wrong changes the failure mode:

- **`FileProjection` top-level fields are snake_case** (`re_exports`, `dynamic_imports`,
  `const_map_fragment`, `procedure_router_fragments`, `router_mount_fragments`, `class_shape_fragments`,
  `is_entry`, ...) — this struct carries no `#[serde(rename_all = ...)]` (`crates/core/src/
  normalized.rs`). The exceptions are the three span fields — `loop_spans`, `function_spans` and
  `test_spans` — which additionally accept camelCase `loopSpans`/`functionSpans`/`testSpans` on input
  (`#[serde(alias = ...)]`). A camelCase spelling of any
  OTHER `FileProjection` field (e.g. `reExports`) matches no struct field, so serde treats it as an
  unrecognized key.
- **`SourceSymbol` (the `symbols` array) outputs camelCase, but accepts snake_case input for exactly
  three fields**: `is_default`, `body_start`, and `body_end` each carry a `#[serde(alias = ...)]` back
  to their long-standing snake_case spelling (`crates/core/src/ir.rs`) so the original external-parser
  contract keeps working alongside the newer camelCase-uniform output. `writeSites` has no snake_case
  alias — camelCase-only, both directions.
- **The `io` payload types are camelCase with no snake_case aliasing at all**: `IoProvide`/`IoConsume`
  and their nested body-shape payloads (`ConsumeBodyShape`'s `completeAt`, `ProvideBodyShape`'s
  `subKey`/`dtoRef`, all `#[serde(rename_all = "camelCase")]` in `crates/core/src/io.rs`) only ever
  match the camelCase spelling — there is no legacy snake_case form to fall back to here.

**The failure mode depends on whether the misspelled field is required or optional**, not on which of
the two casing conventions above it belongs to:

- An optional field spelled in the wrong casing (no alias covering it) is **silently dropped**: every
  `FileProjection` top-level field is `#[serde(default)]`, and most `io`-payload fields are too
  (`ProvideBodyShape`'s `subKey`/`dtoRef`/`fields`/`complete`, `IoConsume`'s `client`/`raw`/`method`,
  ...) — a wrong spelling just means the field reads back at its empty/default value, and the file
  still validates cleanly. There is no error; the data is just quietly missing.
- A **required** field spelled in the wrong casing makes the WHOLE envelope hard-fail. The concrete
  case in this contract: `ConsumeBodyShape`'s `keys` and `completeAt` carry no `#[serde(default)]` —
  if a producer attaches a `body` object to a consume but spells the second field `complete_at`
  instead of `completeAt`, deserialization sees `completeAt` as missing (a required field), which fails
  the top-level `serde_json::from_str` call in `zzop_core::validate_envelope` before that function's own
  semantic checks ever run — so one wrongly-cased nested field fails deserialization of the entire
  envelope JSON document, every file in it, not just the one `body` payload.

## Reserved sentinel kinds

Two `kind` strings are reserved for the native TypeScript parser's own project-wide rewrite passes and
must NEVER be emitted by an external producer, in either Mode A (full envelope) or Mode B (adapter
overlay, below):

- `nest-global-prefix` (an `IoProvide` kind) — the NestJS `app.setGlobalPrefix('api')` sentinel. Only
  `zzop_parser_typescript::adapters::global_prefix` (native TS) emits it, and only the native
  `analyze::assemble` pipeline's `apply_and_strip_global_prefix` seam consumes and strips it.
- `client-base-prefix` (an `IoConsume` kind; the string constant is
  `zzop_parser_typescript::CLIENT_BASE_PREFIX_KIND`) — the `axios.defaults.baseURL` path-prefix
  sentinel. Only `zzop_parser_typescript::adapters::client_base` emits it, and only
  `compose::apply_client_base_prefixes` consumes and strips it.

Both rewrite seams run once, over the WHOLE native tree's merged `io_provides`/`io_consumes`, and only
exist on the native in-process parsing path — envelope ingestion never runs either of them. An external
producer therefore has no way to trigger the intended rewrite, so the engine treats both kinds as
producer-forbidden and drops them at the boundary rather than leaking a raw, unrewritten sentinel into
output or a rule:

- **Mode A** (`analyze_envelope`) drops any `nest-global-prefix`/`client-base-prefix` entry per file, at
  ingestion, before it ever reaches `MinimalIr::io` or a rule (`crates/engine/src/envelope.rs`,
  `is_reserved_provide_kind`/`is_reserved_consume_kind` filtering the per-file `io_provides`/
  `io_consumes` extend, ~lines 165-190). This drop is not silent: the envelope gets one aggregate
  `AnalyzeOutput::warnings` entry naming the envelope's `parser`, the dropped count, and the reserved
  kinds — a partial drop, not a validation failure, so the envelope's other `io`/fragment data still
  analyzes normally.
- **Mode B** (`apply_adapter_overlays`) drops the same two kinds from every overlay `FileProjection`
  before either merge branch runs (`crates/engine/src/envelope.rs`, ~lines 608-628), with the same
  not-silent posture: an overlay a sentinel was dropped from gets one aggregate
  `AnalyzeOutput::warnings` entry naming the overlay's `parser`, the dropped count, and the reserved
  kinds — a partial drop, not a validation failure, so the overlay's other `io`/fragment data still
  merges normally. (Reasoning for the drop: an overlay sentinel that survived the merge would get
  re-applied project-wide by the native seam once merged — every native route re-prefixed by an
  overlay's accident, not scoped to the overlay's own files.)

If your framework has an equivalent concept (a global route prefix, a per-client base URL), fold it into
the normalized `key` you emit yourself rather than trying to reproduce either native rewrite.

Deployment-topology declarations (config-declared as `trees[].topology`, not a sentinel kind — see
[modules/facade.md](modules/facade.md#functions)'s `mounts`/`mountedAt`/`hosts`/`clientBase` `AnalyzeRequest` fields) are
NOT part of the reserved-kind drop above: they apply uniformly to Mode A envelopes and natively-parsed trees alike, at the structurally
equivalent seam after fragment composition and before the IO freeze — a config mount rewrites a Mode A
tree's `http` provide keys exactly like it would a native tree's, and `clientBase` prefixes its relative
`http` consume keys the same way (idempotently: a key already under the declared base is left alone).

## Adapter overlays

An **external adapter** is any producer of a Normalized AST envelope for a framework/language the
engine has no native in-process parser for — or, in overlay mode (below), a producer that ADDS
framework-specific facts on top of a language the engine DOES parse natively (e.g. router-mount
knowledge layered onto TypeScript). There are two ways an envelope reaches the engine; name them so
callers can refer to either unambiguously.

- **Mode A — full envelope** (already documented above). One `NormalizedEnvelope` stands in for an
  ENTIRE source tree; `zzop_engine::analyze_envelope(envelope, config)` runs the whole language-neutral
  analysis over it alone — no native parsing involved at all.
- **Mode B — adapter overlay.** A PARTIAL envelope — typically only `io` plus the three fragment
  channels (`const_map_fragment`/`procedure_router_fragments`/`router_mount_fragments`) populated, with
  `symbols`/`imports`/etc. often left empty — is merged ON TOP of a NATIVE `analyze_tree` run over the
  same tree. Supplied via the Rust `EngineConfig::adapter_overlays: Vec<NormalizedEnvelope>` field
  (empty by default: zero behavior change for every existing caller).

  **Availability — where each mode actually runs.** Mode B overlays work everywhere: the config-file
  `overlays`/`trees[].overlays` key (`zzop.config.jsonc`, mapped by the Rust-hosted `zzop-config`
  crate), and the `adapterOverlays` request field it compiles down to, are honored by every host — the
  Node-free `zzop-mcp` binary runs a config's overlays through the exact same `analyze`/`analyzeTrees`
  path any direct `zzop-facade` embedder does. Mode A (full-envelope `analyze_envelope`) is reachable
  from Rust (`zzop_engine::analyze_envelope`), from a direct `zzop-facade` embedding
  (`analyze_envelope_json`), AND from the Node-free `zzop-mcp` binary — its `analyze_envelope` MCP tool
  and `zzop analyze-envelope <envelope.json>` CLI subcommand both run the same
  `zzop_summary::analyze_envelope_summary` call path (`crates/summary`, over
  `zzop_facade::analyze_envelope_json`), the one lane that REQUIRES no config — an envelope document
  carries no filesystem location inside it, so nothing is auto-discovered from the document. Where the
  CALLER knows that location, though, the config beside it is read: `zzop analyze-envelope
  <envelope.json>` looks for a `zzop.config.jsonc` in the envelope file's own directory (the same
  discovery `zzop analyze <path>` makes at a tree root) and applies that project's declared
  `vocabulary` — ONLY that key, since every other key configures a tree analysis this lane does not
  perform — disclosing the applied file in the reply's `configWarnings`. The `analyze_envelope` MCP
  tool is handed envelope TEXT with no location, so it discovers nothing and runs on the product's
  built-in convention vocabulary; a project whose own naming conventions decide the verdict should use
  the CLI form. In short: to run a Mode A envelope, use the `zzop` / `zzop-mcp` binary
  (its tool or CLI subcommand) or embed `zzop-facade` directly.

  Each overlay is validated with `validate_envelope` independently; an invalid overlay is skipped with
  one `AnalyzeOutput::warnings` entry naming its `parser` id — never a crash, never a failed analysis
  for the other overlays or for the native files. Per `FileProjection` in a valid overlay:
  - If a native artifact exists at the SAME `path`/`rel`: its `io` is extended with the overlay's `io`
    entries (an overlay entry EXACTLY duplicating a native one — same kind/key/file/line — is deduped,
    never double-counted), its fragment channels (`procedure_router_fragments`/`router_mount_fragments`) are
    appended, and `const_map_fragment` merges NATIVE-FIRST (a key the native pass already resolved is
    never overwritten by an overlay). The three dep-graph channels
    (`imports`/`re_exports`/`dynamic_imports`) merge under that SAME native-first rule, applied per KEY:
    `imports` is a `localName -> binding` map, so a local name the native pass already bound keeps its
    native binding and only names it never bound are added; `re_exports` and `dynamic_imports` have no
    key and so append minus exact duplicates (a type-only re-export stays distinct from an otherwise
    identical runtime one, since only the latter is an edge). A native fact is therefore never
    overridden, but it no longer SILENCES the overlay either: before 2026-07-30 this was all-or-nothing
    per FILE — one native binding discarded the overlay's entire dep-graph contribution for that file,
    which made an adapter's worth depend on what the native parser happened to leave empty rather than on
    what the adapter knows. Additive merging is what lets native and injected extraction combine on one
    file, so an adapter can supply exactly the imports the native parser could not resolve.
  - If no native artifact exists at that path (an adapter-only file — e.g. a `.svelte`/`.vue`/`.astro`
    file, or a generated route table the native TS parser never sees as a distinct file): a synthetic
    artifact is created from the projection, carrying its OWN `imports`/`re_exports`/`dynamic_imports`
    in addition to `io`/fragments — so an adapter for any non-TS file type can complete the dep graph:
    its imports give their native TS targets real fan-in edges, exactly like a native TS importer's
    would (keeping `dead-candidates` from false-positiving them). `imports` stays absent when the
    projection carries no dep-graph data at all (none of the three fields populated).

  Independently of the merge branch above, every `is_entry: true` `FileProjection`'s `path` from an
  APPLIED overlay is unioned into the `dead-candidates` analysis's exempt set (the overlay counterpart of
  a package.json manifest entry) — a framework-loaded file an adapter declares reachable by convention is
  not flagged dead for having zero in-repo importers. APPLIED, not merely configured: the path is
  recorded inside the apply loop, past the `validate_envelope` gate, so an overlay this seam REJECTED
  exempts nothing. (`dead-candidates` only — `unreachable` has its own entry seed and does not read
  `is_entry`.)

  **Self-disclosure: `source`, coverage, and synthetic entries.** Three checks run once per ACCEPTED
  overlay, independent of the merge branch each `FileProjection` takes:
  - `source` is the overlay's own declared tree/source id, but every projection still merges onto
    whichever tree's `overlays`/`adapterOverlays` entry carried it, regardless of what `source` says.
    A non-empty `source` that differs from that tree's own id triggers a warning — its facts will join
    as intra-source, not cross-source — UNLESS the overlay carries no join-relevant `io` (an
    attributes-/`is_entry`-only overlay is source-agnostic and never warns here).
    **What to write for a single-tree overlay:** match whatever id the tree ACTUALLY resolves to for the
    invocation you're using. Two different defaults apply depending on how the tree is invoked, and they
    are NOT the same string: a `zzop.config.jsonc` `trees[]` entry with no explicit `sourceId` defaults
    to that entry's own raw `root` string exactly as written (e.g. `root: "./api"` → `sourceId: "./api"`
    — see `crates/config/config-surface.json`'s `treeFields` docs); a bare-path run (a config with no
    `trees[]`, or a direct `zzop-facade` embedding calling `analyze()` with `root` only) instead
    defaults to the analyzed ROOT DIRECTORY's own basename (`apply_source_id_default`,
    `crates/facade/src/analyze.rs` — the shared chokepoint every host funnels through: both binaries'
    single-path lanes, and any embedder driving the facade with no config-file front end). Set an
    explicit `sourceId` in your request/config when you want one fixed value regardless of which path
    invokes the tree, rather than relying on either default.
  - A declared `files[].path` matching no file in the tree is still merged in, as a synthetic
    `FileArtifact` (the "not found" branch above) — but the overlay gets one warning naming how many
    of its declared paths were synthetic, with up to 3 sample paths. `path` must be tree-root-relative;
    a mismatch is usually a typo.
  - An entry counts as adapter coverage for the per-extension "no native parser" diagnostic (the
    self-report warning a file whose extension has no native parser, naming the `overlays: [...]`
    remedy) only if its overlay PASSED the validation above — a skipped overlay merges nothing, so it
    covers nothing and every file it declared keeps triggering the diagnostic — and only if the entry
    carries at least one fact the merge actually consumes: non-reserved `io`, `imports`,
    `re_exports`, `dynamic_imports`, a fragment channel, non-empty `attributes`, or `is_entry: true`.
    **`symbols` does not count** — neither merge branch above reads an overlay projection's `symbols`,
    so a symbols-only entry is empty coverage. An overlay whose every entry carries none of these gets
    one "zero-fact" warning, and every file behind it keeps triggering the native-parser diagnostic.

  Overlay-added fragments then flow through the EXACT SAME whole-tree composition passes as anything
  else (`compose_trpc_provides`/`compose_router_mount_provides`) — an overlay is not a separate code path
  past the merge point.

**Start minimal — partial-envelope-first.** You do not need a parser to close a coverage gap: the
default on-ramp is a Mode B partial envelope covering just the missing channel and files — a
tens-of-lines script, not an hours-long native parser. Exhibit A:
[`examples/adapters/java-imports-adapter/`](../examples/adapters/java-imports-adapter/) filled exactly ONE channel
(dep-graph `imports`) in ~90 lines, back when Java's built-in projector was lexical and extracted
no imports at all — the native Java parser has since closed that specific gap, but the recipe is
unchanged for any extension still missing a channel. Iterate against the embedded contract
(`zzop contract envelope-guide` / `zzop contract envelope-schema` print this document and
its JSON Schema straight from the binary), validate offline with `zzop validate-envelope
<envelope.json>` (or the `validate_envelope` MCP tool, which performs the same check;
`zzop contract example-envelope` prints a complete valid sample), and add further channels only
when an analysis you care about needs them. The
per-extension "no native parser" warning and the consume-silence tripwires point here for exactly
this reason.

**Minimal overlay example.** A `router_mount_fragments` overlay contributing `POST
/api/auth/two-factor/setup`, split across two files exactly as the source tree splits it: one file
mounts a sub-router at prefix `/api/auth/two-factor` (a `Mount` entry pointing at the second file),
the second file registers the `POST /setup` verb on that mounted router (a `Verb` entry) — the
composed key is the mount prefix joined with the verb path. The `Mount`'s
`specifier` (`"./two-factor"`) is a `./`-relative path resolved from the emitting file's own directory
(`src/routes/auth/`) with `.ts` extension-guessing, landing on the second file's `path` exactly — the
resolution rule documented above for fragment specifiers.

```json
{
  "format": "zzop-normalized-ast",
  "version": "0.29.0",
  "parser": "hono-router-overlay/1",
  "source": "api",
  "files": [
    {
      "path": "src/routes/auth/index.ts",
      "loc": 8,
      "router_mount_fragments": [
        {
          "name": "auth",
          "entries": [
            {
              "Mount": {
                "prefix": "/api/auth/two-factor",
                "ident": "twoFactorRoute",
                "specifier": "./two-factor"
              }
            }
          ]
        }
      ]
    },
    {
      "path": "src/routes/auth/two-factor.ts",
      "loc": 14,
      "router_mount_fragments": [
        {
          "name": "twoFactorRoute",
          "entries": [
            {
              "Verb": {
                "method": "POST",
                "path": "/setup",
                "handler": "setupTwoFactor",
                "line": 9
              }
            }
          ]
        }
      ]
    }
  ]
}
```

**Determinism/dedup.** Overlays are processed in a deterministic order — sorted by their `parser`
field — so a multi-overlay run's output does not depend on caller-supplied `Vec` order. The io-entry
dedup key is `(kind, key, file, line)`, applied to both `provides` and `consumes`.

**Line anchoring.** Because `line` is part of the dedup key above, a multi-line call expression needs one
unambiguous, reproducible rule or two adapters can double-count the same call site at two different
lines. Anchor each `io` fact to the line where the call expression STARTS — the first token of the call
chain, not the line the invoked method name happens to sit on — and emit exactly one fact per call site.
For example:

```ts
api.articles
  .createArticleComment(articleId, body);
```

anchors to the `api.articles` line, not the `.createArticleComment(...)` line. This matches how zzop's own
native TypeScript parser attributes a call: every adapter in `parser/parser-typescript/src/adapters`
(e.g. `egress/collector.rs`, `hono_client/consume.rs`, `trpc_consume.rs`) and the call-graph attributor
(`lang/calls.rs`) all take the enclosing `CallExpr` node's own span start, which for a chained member-call
callee is the position of the chain's first identifier — so anchoring to the call chain's first line is
the verified, not merely conventional, behavior for native TypeScript/JavaScript extraction. Treat it as
the normative rule for any other language/adapter.

The same rule applies outside TypeScript too — a shell script's call site anchors to the line the command
STARTS on, not a continuation line an option/URL happens to wrap onto:

```sh
curl \
  http://localhost:8080/api/ping
```

anchors to the `curl \` line, not the `http://localhost:8080/api/ping` line — one fact for this call site,
regardless of how many lines the invocation wraps across.

**Wire exposure.** Overlays are reachable from Rust (`EngineConfig::adapter_overlays`) AND from every wire
host: `analyze`/`analyzeTrees`'s config accepts an `adapterOverlays` array of envelopes with this
same shape (`AnalyzeRequest::adapter_overlays` in `crates/facade/src/lib.rs`), e.g.:

```json
{
  "root": "/path/to/tree",
  "sourceId": "api",
  "adapterOverlays": [
    { "format": "zzop-normalized-ast", "version": "0.29.0", "parser": "hono-router-overlay/1", "source": "api", "files": [ ... ] }
  ]
}
```

An overlay is re-validated and soft-skipped with a warning if invalid, same as the Rust path above.
`analyzeEnvelope` (Mode A) has no equivalent field — a full envelope REPLACES native analysis rather
than augmenting it, so the two modes don't combine.
