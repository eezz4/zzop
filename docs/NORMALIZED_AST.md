# Normalized AST contract (external parser protocol) — v1 freeze

External/custom parsers (Java, Python, JSP, anything the engine does not parse natively) join the
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
  "version": 1,
  "parser": "<parser id>/<impl version>",
  "source": "<tree/source id>",
  "files": [ <FileProjection> ]
}
```

- `version` is the contract version (this document freezes v1). A consumer rejects `version` greater
  than it supports — same policy as the DSL pack `schema_version`.
- `parser` doubles as the cache fingerprint segment: bump the impl version whenever the projection
  changes for identical input.

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
  "trpc_router_fragments": [ <TrpcRouterFragment> ],
  "router_mount_fragments": [ <RouterMountFragment> ],
  "class_shape_fragments": [ <ClassShapeFragment> ],
  "degraded": false,
  "is_entry": false,
  "loop_spans": [[10, 14]]
}
```

Field semantics (all mirror the Rust `zzop-core` serde types — those are the normative schema):

- `loc` — raw physical line count (`text.split('\n').length` semantics, trailing newline adds one).
- `symbols` — declarations. `body_start`/`body_end` (1-based, inclusive) power method-scan DSL rules;
  a parser that cannot produce spans omits them and those rules silently skip the file (graceful
  degrade, never an error). A lexical brace-matcher is an acceptable span source (the built-in Java
  projector works this way).
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
  `packages/core/src/io.rs`'s `http_interface_key`/`http_consume_interface_key` (provide-side vs.
  consume-side asymmetry, path-param collapse, slash/method normalization); the byte-exact,
  language-agnostic parity contract for reproducing them is
  [`adapters/key-normalization.fixture.json`](adapters/key-normalization.fixture.json) (see
  [`adapters/README.md`](adapters/README.md) for how to replay it), the machine-checkable shape is
  [`adapters/envelope.schema.json`](adapters/envelope.schema.json), and a ready-made JS implementation
  of both is [`examples/adapter-kit/`](../examples/adapter-kit/)'s `lib/keys.js`. **Absolute-URL keys
  (`"://"` present) are the one case that must NOT go through that normalization at all** — see
  `adapters/README.md`'s "Absolute URLs bypass normalization entirely" section.
  - OPTIONAL client provenance (additive since `axios-defaults-base-v1`; omit and nothing changes):
    an `IoConsume` may carry `client: "axios"` naming the HTTP client that produced the call site.
    `client` is a free-form string (`Option<String>` in `packages/core/src/io.rs`), not a closed enum —
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
    `packages/core/src/io.rs`'s `ConsumeBodyShape`/`ProvideBodyShape` for the normative semantics
    (evidence-only: anything not statically witnessed is omitted, never approximated).
- `const_map_fragment`, `trpc_router_fragments`, `router_mount_fragments`, `class_shape_fragments` —
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
  - `trpc_router_fragments` is `[ <TrpcRouterFragment> ]`, same shape as the native tRPC-router-fragment
    projection: a named router binding plus entries, each either a `Ref` to another router by
    identifier/import-specifier, a `Nested` inline sub-router, or a `Leaf` procedure. See
    `packages/core/src/fragments.rs`'s `TrpcRouterFragment`/`TrpcRouterEntry` for the normative field
    names.
  - `router_mount_fragments` is `[ <RouterMountFragment> ]`, same shape as the native Hono-style
    router-mount projection: a named router identifier plus entries, each either a `Verb` registration
    (`{method, path, handler, line}`) or a `Mount` sub-router mount (`{prefix, ident, specifier}`). See
    `packages/core/src/fragments.rs`'s `RouterMountFragment`/`RouterMountEntry` for the normative field
    names.
  - `class_shape_fragments` is `[ {"name": "CreateUserDto", "fields": [{"name": "email",
    "optional": false}], "complete": true} ]` — one entry per class declaration the adapter's language
    can see, the resolution substrate for `IoProvide.body.dtoRef` (above): at assemble time the
    tree-wide merge resolves each `dtoRef` by class name, dropping unresolvable or cross-file-conflicting
    names with a warning rather than guessing. `complete: false` means the field list may be partial
    (inheritance, mixins, index signatures — whatever the source language's equivalent is). See
    `packages/core/src/fragments.rs`'s `ClassShapeFragment`.

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
- `is_entry` — OPTIONAL (`#[serde(default)]`, default `false`). Marks this file a framework/runtime
  ENTRY loaded by convention rather than imported (a SvelteKit `hooks.*`/`+page`, a `.vue` route, ...),
  so zero in-repo importers is expected, not dead-code signal — the overlay counterpart of a
  package.json manifest entry. Meaningful in Mode B (adapter overlays, below): every `is_entry: true`
  file's `path` across all configured overlays is unioned into the `dead-candidates` analysis's exempt
  set. Mode A (`analyze_envelope`) does not read this field.
- `loop_spans` — OPTIONAL (`#[serde(default)]`; camelCase `loopSpans` also accepted on input). `[[startLine,
  endLine], ...]`, 1-based and inclusive. Each pair is either a loop statement's whole span (`for`/
  `for-in`/`for-of`/`while`/`do-while`, header line included) or an array-iteration callback argument's
  span (`.map`/`.forEach`/`.filter`/... — the callback only, never the whole call). Feeds
  `MethodScan::trigger_in_loop`; absent means no structural loop facts for this file, and that matcher
  silently skips it (graceful degrade, same convention as `symbols`' `body_start`/`body_end`).

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
`zzop_core::SUPPORTED_NORMALIZED_AST_VERSION`, an empty or duplicate file `path`, and a symbol whose
`body_end` is less than its `body_start` — collecting every issue found rather than stopping at the
first. The engine-side receiver, `zzop_engine::analyze_envelope(envelope, config) -> AnalyzeOutput`,
projects an already-validated envelope into the same per-file artifact shape a native parser produces
and runs every language-neutral whole-graph analysis over it (see that function's own module doc for
exactly which per-file DSL rules and analyses run in envelope mode, and why line-scan/method-scan rules
and git-history-dependent analyses do not). `examples/jsp-envelope.example.json` is a hand-written,
crude-parser-shaped fixture (symbols with no body spans, one `http` provide, one `db-table` consume, no
imports) that validates cleanly against this contract — see `zzop-core`'s `normalized::tests::
jsp_contract_example_validates` for the fixture-based check. A JSON Schema export for this contract
already ships at [`adapters/envelope.schema.json`](adapters/envelope.schema.json), derived field-for-field
from the same Rust serde types this document describes — see `adapters/README.md`'s "Envelope schema &
versioning policy" section for how it tracks this document.

## Casing

Casing is not uniform across the envelope, and which part you get wrong changes the failure mode:

- **`FileProjection` top-level fields are snake_case** (`re_exports`, `dynamic_imports`,
  `const_map_fragment`, `trpc_router_fragments`, `router_mount_fragments`, `class_shape_fragments`,
  `is_entry`, ...) — this struct carries no `#[serde(rename_all = ...)]` (`packages/core/src/
  normalized.rs`). The one exception is `loop_spans`, which additionally accepts camelCase
  `loopSpans` on input (`#[serde(alias = "loopSpans")]`). A camelCase spelling of any OTHER
  `FileProjection` field (e.g. `reExports`) matches no struct field, so serde treats it as an
  unrecognized key.
- **`SourceSymbol` (the `symbols` array) outputs camelCase, but accepts snake_case input for exactly
  three fields**: `is_default`, `body_start`, and `body_end` each carry a `#[serde(alias = ...)]` back
  to their frozen v1 snake_case spelling (`packages/core/src/ir.rs`) so the original external-parser
  contract keeps working alongside the newer camelCase-uniform output. `writeSites` has no snake_case
  alias — camelCase-only, both directions.
- **The `io` payload types are camelCase with no snake_case aliasing at all**: `IoProvide`/`IoConsume`
  and their nested body-shape payloads (`ConsumeBodyShape`'s `completeAt`, `ProvideBodyShape`'s
  `subKey`/`dtoRef`, all `#[serde(rename_all = "camelCase")]` in `packages/core/src/io.rs`) only ever
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
  ingestion, before it ever reaches `MinimalIr::io` or a rule (`packages/engine/src/envelope.rs`,
  `is_reserved_provide_kind`/`is_reserved_consume_kind` filtering the per-file `io_provides`/
  `io_consumes` extend, ~lines 120-146). This drop is not silent: the envelope gets one aggregate
  `AnalyzeOutput::warnings` entry naming the envelope's `parser`, the dropped count, and the reserved
  kinds — a partial drop, not a validation failure, so the envelope's other `io`/fragment data still
  analyzes normally.
- **Mode B** (`apply_adapter_overlays`) drops the same two kinds from every overlay `FileProjection`
  before either merge branch runs (`packages/engine/src/envelope.rs`, ~lines 444-535), with the same
  not-silent posture: an overlay a sentinel was dropped from gets one aggregate
  `AnalyzeOutput::warnings` entry naming the overlay's `parser`, the dropped count, and the reserved
  kinds — a partial drop, not a validation failure, so the overlay's other `io`/fragment data still
  merges normally. (Reasoning for the drop: an overlay sentinel that survived the merge would get
  re-applied project-wide by the native seam once merged — every native route re-prefixed by an
  overlay's accident, not scoped to the overlay's own files.)

If your framework has an equivalent concept (a global route prefix, a per-client base URL), fold it into
the normalized `key` you emit yourself rather than trying to reproduce either native rewrite.

Deployment-topology `mounts`/`mountedAt` (config-declared, not a sentinel kind — see
[packages/cli/README.md](../packages/cli/README.md#connection-topology)) are NOT part of the reserved-kind
drop above: they apply uniformly to Mode A envelopes and natively-parsed trees alike, at the structurally
equivalent seam after fragment composition and before the IO freeze — a config mount rewrites a Mode A
tree's `http` provide keys exactly like it would a native tree's.

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
  channels (`const_map_fragment`/`trpc_router_fragments`/`router_mount_fragments`) populated, with
  `symbols`/`imports`/etc. often left empty — is merged ON TOP of a NATIVE `analyze_tree` run over the
  same tree. Supplied via the Rust `EngineConfig::adapter_overlays: Vec<NormalizedEnvelope>` field
  (empty by default: zero behavior change for every existing caller).

  Each overlay is validated with `validate_envelope` independently; an invalid overlay is skipped with
  one `AnalyzeOutput::warnings` entry naming its `parser` id — never a crash, never a failed analysis
  for the other overlays or for the native files. Per `FileProjection` in a valid overlay:
  - If a native artifact exists at the SAME `path`/`rel`: its `io` is extended with the overlay's `io`
    entries (an overlay entry EXACTLY duplicating a native one — same kind/key/file/line — is deduped,
    never double-counted), its fragment channels (`trpc_router_fragments`/`router_mount_fragments`) are
    appended, and `const_map_fragment` merges NATIVE-FIRST (a key the native pass already resolved is
    never overwritten by an overlay). The native artifact's own `imports`/`re_exports`/`dynamic_imports`
    are left untouched — native dep-graph facts stay authoritative; this merge branch never adds to them.
  - If no native artifact exists at that path (an adapter-only file — e.g. a `.svelte`/`.vue`/`.astro`
    file, or a generated route table the native TS parser never sees as a distinct file): a synthetic
    artifact is created from the projection, carrying its OWN `imports`/`re_exports`/`dynamic_imports`
    in addition to `io`/fragments — so an adapter for any non-TS file type can complete the dep graph:
    its imports give their native TS targets real fan-in edges, exactly like a native TS importer's
    would (keeping `dead-candidates` from false-positiving them). `imports` stays absent when the
    projection carries no dep-graph data at all (none of the three fields populated).

  Independently of the merge branch above, every `is_entry: true` `FileProjection`'s `path` across ALL
  configured overlays is unioned into the `dead-candidates` analysis's exempt set (the overlay
  counterpart of a package.json manifest entry) — a framework-loaded file an adapter declares reachable
  by convention is never flagged dead for having zero in-repo importers.

  Overlay-added fragments then flow through the EXACT SAME whole-tree composition passes as anything
  else (`compose_trpc_provides`/`compose_router_mount_provides`) — an overlay is not a separate code path
  past the merge point.

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
  "version": 1,
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

**napi exposure.** Overlays are reachable from Rust (`EngineConfig::adapter_overlays`) AND from napi
callers: `analyze`/`analyzeTrees`'s config accepts an `adapterOverlays` array of envelopes with this
same shape (`AnalyzeRequest::adapter_overlays` in `packages/napi/src/api.rs`, `Array<Record<string,
unknown>>` in `packages/napi/index.d.ts`'s `AnalyzeConfig`), e.g.:

```json
{
  "root": "/path/to/tree",
  "sourceId": "api",
  "adapterOverlays": [
    { "format": "zzop-normalized-ast", "version": 1, "parser": "hono-router-overlay/1", "source": "api", "files": [ ... ] }
  ]
}
```

An overlay is re-validated and soft-skipped with a warning if invalid, same as the Rust path above.
`analyzeEnvelope` (Mode A) has no equivalent field — a full envelope REPLACES native analysis rather
than augmenting it, so the two modes don't combine.
