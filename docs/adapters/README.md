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
functions by `crates/core/tests/key_normalization_fixture.rs` on every `cargo test`. A diff in
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

## Absolute URLs bypass normalization entirely

A consume key that carries a host (`"://"` present — a call site resolved to an absolute URL, e.g.
`"GET https://vendor.com/api/users"`) must be preserved **verbatim** — never run through either
normalizer above. Neither `http_interface_key` nor `http_consume_interface_key` special-cases a
scheme: feeding one an absolute URL prepends a leading `/` and then collapses the scheme's `//` into a
single `/`, so `https://vendor.com/api/users` comes out as `/https:/vendor.com/api/users` — the host
gets corrupted into something that looks like a path segment. `zzop_core::link_cross_layer_io`'s
`"://"` gate (`crates/core/src/io.rs`) depends on the host surviving intact: it routes a
host-carrying key to `externalConsumes` — third-party egress, never cross-tree joined, never counted as
`unprovidedConsumes` — and normalizing the key first breaks that routing (the corrupted key no longer
contains `"://"`, so it silently falls through to a normal, wrong, join attempt instead).

The native TS parser gets this right by never calling either normalizer on an external URL in the first
place: `consume_key_for` (`parser/parser-typescript/src/adapters/egress.rs`) checks `is_external(url)`
(an `http://`/`https://` prefix) BEFORE normalizing, and for that branch keys the consume as
`format!("{} {}", method.to_uppercase(), url)` — the raw URL, untouched. An external adapter must do
the same: detect `://` in the resolved target first, and if present, key it as `"<METHOD> <url>"` with
the URL exactly as resolved, skipping every normalization rule in this document entirely.

(Note: [`key-normalization.fixture.json`](key-normalization.fixture.json) deliberately carries no
`"://"` row. That fixture pins `http_interface_key`/`http_consume_interface_key`'s own output on a
`(method, path)` input — and per the rule above, neither function is ever correctly called with an
absolute URL in the first place, so there is no "correct normalizer output" for such a row to pin; the
verbatim-preservation behavior itself is pinned instead by `crates/core/src/io.rs`'s
`host_carrying_consume_key_is_external_never_dangling_even_with_a_matching_internal_provide` test and
`egress.rs`'s own external-URL extraction tests.)

## Envelope schema & versioning policy

[`envelope.schema.json`](envelope.schema.json) is a draft-07 JSON Schema for the v1
`NormalizedEnvelope`/`FileProjection` contract, derived field-for-field from the real Rust serde
types (`crates/core/src/normalized.rs`, `io.rs`, `ir.rs`, `fragments.rs`) — field names,
required-ness, and nullability all trace back to whether the Rust field carries `#[serde(default)]`.

The versioning policy is the same tolerance philosophy zzop's config surface already uses: this is
**v1**, and both the engine and any well-behaved consumer **silently ignore unknown fields — no
warning, never a hard fail**. Verified against the engine, not just asserted: none of
`crates/core/src/{normalized,io,ir,fragments}.rs` sets `#[serde(deny_unknown_fields)]` on any
envelope-related type, and there is no warning code anywhere in `zzop-engine`/`zzop-core` for an
unrecognized envelope field — an unknown key is simply dropped during deserialization, the same as any
other serde struct in this codebase without `deny_unknown_fields` (see e.g.
`crates/facade/src/lib.rs`'s or `crates/engine/tests/rule_contracts.rs`'s own doc comments on that
same convention). The only envelope-related `AnalyzeOutput::warnings` entries that exist today are for
a whole Mode B overlay failing `validate_envelope`, or for a reserved-sentinel-kind drop (see
`docs/NORMALIZED_AST.md`'s "Reserved sentinel kinds" section) — neither is about unknown fields.
`additionalProperties: true` at every level in the schema reflects the "ignored" half of that
tolerance, not a warning. A producer emitting a field ahead of the schema, or omitting any field
marked optional, still produces a valid envelope. Only a **breaking** change — removing, renaming, or
re-typing a *required* field — needs a new envelope `version`; additive fields never do. See the
schema's own `$comment` for the same statement in machine-readable form.

## Self-disclosure: coverage, source, and synthetic entries

Beyond key normalization, a Mode B overlay (`adapterOverlays`) is checked against three
self-disclosure rules once it's accepted — get one wrong and the overlay still merges, but the
engine warns rather than staying silent:

- **`source` must name the tree you're attached to.** An overlay's `source` field is its own
  declared tree/source id, but its facts always merge onto whichever tree's `overlays`/
  `adapterOverlays` entry carried it, regardless of what `source` says. If `source` is non-empty and
  differs from that tree's own id, and the overlay carries any `io`, you get a warning that its
  facts will join as intra-source rather than cross-source — fix it by moving the overlay to the
  tree `source` actually names. An attributes-/`is_entry`-only overlay is source-agnostic and never
  triggers this warning.
- **Every declared `files[].path` must be tree-root-relative and match a real file.** A path
  matching nothing in the tree is still merged in — as a synthetic entry, never silently dropped —
  but the overlay gets one warning naming how many of its declared paths were synthetic, with a
  handful of sample paths. This is almost always a typo in `path`.
- **An entry needs at least one consumed fact to count as coverage.** The per-extension "no native
  parser" diagnostic is suppressed for a file only when its overlay entry carries a fact the merge
  actually consumes — non-reserved `io`, `imports`, `re_exports`, `dynamic_imports`, a fragment
  channel, `attributes`, or `is_entry`. An entry with none of these still gets merged (or added as a
  synthetic entry) and simply does not count as coverage — the file's extension keeps triggering the
  native-parser diagnostic. (The zero-fact warning itself is per-overlay: it fires only when NO
  entry in the overlay carries a fact.) **`symbols` is not a consumed fact for Mode B** — the overlay merge
  never reads an entry's `symbols`, so a symbols-only entry does not count as coverage.

See [`docs/NORMALIZED_AST.md`](../NORMALIZED_AST.md)'s "Adapter overlays" section for the
normative merge semantics these warnings are checking.

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
