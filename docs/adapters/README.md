# Adapter key-normalization parity

Cross-layer linking (`link_cross_layer_io`) joins a CONSUME to a PROVIDE by an exact string match
on a normalized `"METHOD /path"` key. If a non-Rust adapter (a different language, a hand-rolled
extractor) computes that key even slightly differently than the engine does, the join misses —
silently. There is no error: the consume just lands in `unprovidedConsumes` / the provide in
`unconsumedProvides` instead of forming an edge. Byte-identical keys are the whole contract.

## The fixture

[`key-normalization.fixture.json`](key-normalization.fixture.json) is a table of
`{ "side": "provide" | "consume", "method": "...", "path": "...", "key": "..." }` rows, generated
by calling the real engine functions — `zzop_core::http_interface_key` (provide side) and
`zzop_core::http_consume_interface_key` (consume side) — on a curated input list that exercises
every normalization rule: leading-slash addition, duplicate-slash collapse, trailing-slash drop,
`{x}`/`:x` path-param collapse to `{}`, method upper-casing, and (consume side only) `?query`/
`#fragment` suffix drop. It also covers edge inputs: empty path, root path (`/`), a path that is
only a param, mixed `{a}/:b` params, and query-only input.

It is committed, not generated at build time, and is verified byte-for-byte against the real
functions by `packages/core/tests/key_normalization_fixture.rs` on every `cargo test`. A diff in
this file is a **breaking, adapter-facing change** — call it out in release notes.

## Using it from another language

1. Read the JSON array.
2. For each row, run your adapter's own key-normalization logic on `(method, path)` — using the
   provide-side rule if `side` is `"provide"`, the consume-side rule if `side` is `"consume"`.
3. Assert your result equals `key`.

That is the entire parity check — no engine build, no FFI, just string in / string out.

## The one asymmetry: query suffixes

- **Provide-side** (`http_interface_key`) must **not** drop a `?...` suffix — in a route pattern
  `?` is not always a query separator (e.g. a single-character wildcard), so it is data, not noise.
- **Consume-side** (`http_consume_interface_key`) must drop `?...`/`#...` — in a call-site URL a
  `?` is always a query separator, and a provide's key never carries one, so an un-stripped consume
  key can never join.

Get this backwards and every consume with a query string silently stops joining.

## Envelope schema & versioning policy

[`envelope.schema.json`](envelope.schema.json) is a draft-07 JSON Schema for the v1
`NormalizedEnvelope`/`FileProjection` contract, derived field-for-field from the real Rust serde
types (`packages/core/src/normalized.rs`, `io.rs`, `ir.rs`, `fragments.rs`) — field names,
required-ness, and nullability all trace back to whether the Rust field carries `#[serde(default)]`.

The versioning policy is the same tolerance philosophy zzop's config surface already uses: this is
**v1**, and both the engine and any well-behaved consumer **ignore unknown fields with a warning,
never a hard fail** — `additionalProperties: true` at every level in the schema reflects that. A
producer emitting a field ahead of the schema, or omitting any field marked optional, still produces
a valid envelope. Only a **breaking** change — removing, renaming, or re-typing a *required* field —
needs a new envelope `version`; additive fields never do. See the schema's own `$comment` for the
same statement in machine-readable form.

## Adapter kit

[`examples/adapter-kit/`](../../examples/adapter-kit/) is a plain-JS, dependency-free package that
extracts the boilerplate every hand-rolled adapter in `examples/` (openapi-sdk-adapter,
react-query-adapter, wrapper-adapter, svelte-adapter) re-derives on its own: deterministic file
walking (`lib/walk.js`), a validating `EnvelopeBuilder` (`lib/envelope.js`) that assembles a
schema-valid envelope from `addFile`/`addProvide`/`addConsume`/`markEntry` calls, and the byte-exact
`normalizeProvideKey`/`normalizeConsumeKey` HTTP key normalizers (`lib/keys.js`) — parity-tested
against this document's own `key-normalization.fixture.json` above. It lives under `examples/`
deliberately (not published to npm); copy it into a new adapter the same way you'd copy any other
example.
