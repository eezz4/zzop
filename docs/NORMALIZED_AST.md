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
  "used_names": ["identifiersReferencedLocally", "..."],
  "io": { "provides": [ <IoProvide> ], "consumes": [ <IoConsume> ] },
  "const_map_fragment": { "<dotted.const.KEY>": "<literal-string-value>" },
  "trpc_router_fragments": [ <TrpcRouterFragment> ],
  "router_mount_fragments": [ <RouterMountFragment> ],
  "degraded": false
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
- `used_names` — identifiers referenced in the file body (feeds symbol-level dead-export analysis).
- `io` — the cross-layer contract: each provide/consume carries `(kind, key)` where `key` is fully
  normalized by the parser (`"GET /users/{}"`, `"table:users"`). Unresolvable dynamic keys are `null`
  (unresolved), never guessed. Key normalization quality is the parser's whole responsibility and the
  only thing cross-layer accuracy depends on.
- `const_map_fragment`, `trpc_router_fragments`, `router_mount_fragments` — all three are OPTIONAL
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
and git-history-dependent analyses do not). `docs/examples/jsp-envelope.example.json` is a hand-written,
crude-parser-shaped fixture (symbols with no body spans, one `http` provide, one `db-table` consume, no
imports) that validates cleanly against this contract — see `zzop-core`'s `normalized::tests::
jsp_contract_example_validates` for the fixture-based check. A JSON Schema export ships with a future
external adapter.

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
    never overwritten by an overlay).
  - If no native artifact exists at that path (an adapter-only file — e.g. a generated route table the
    native TS parser never sees as a distinct file): a synthetic minimal artifact is created from the
    projection so it still contributes its `io`/fragments to the whole-tree composition.

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

**Scope note.** Overlays are engine-config-supplied only — Rust `EngineConfig::adapter_overlays`. napi
(`packages/napi`) exposure is a PLANNED FOLLOW-UP, not yet wired: do not assume it is reachable from
JS/napi callers today.
