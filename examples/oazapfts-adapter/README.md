# oazapfts adapter (Mode B overlay example)

Makes calls through an [oazapfts](https://github.com/oazapfts/oazapfts)-generated OpenAPI SDK
(`oazapfts.fetchJson('/activities', { ...opts })` — route and verb statically visible, but only via
one generator's call shape) visible to zzop's cross-layer join as route-keyed `IoConsume` facts.
Provenance: this call family was recognized natively by the TypeScript egress extractor
(`oazapfts-v1` in `parser/parser-typescript/src/adapters/egress.rs`) and was retired to this adapter
under zzop's "generated SDKs = injection adapters" decision. See
[examples/README.md](../README.md) for the Mode A/B overview,
[docs/adapters/README.md](../../docs/adapters/README.md) for key-normalization parity, and
[docs/modules/mcp.md](../../docs/modules/mcp.md#the-zzop-facade-json-contract) for the host API that accepts the overlay.

## Run

```sh
node adapter.mjs --root <tree-root> [--source web] > overlay.json   # envelope to stdout, summary to stderr
node --test test/*.test.mjs                                         # ported native test cases
```

Attach the stdout envelope to a tree's `adapterOverlays` array on an `analyze`/`analyzeTrees`
request.

## Contract points

- Recognized calls: `oazapfts.fetchJson|fetchText|fetchBlob(url, opts?)`; the receiver must be
  exactly `oazapfts` (no bare-name allowlist); nesting inside `oazapfts.ok(...)` still matches.
- URL: bare string/template literal only — anything else (variable, concatenation) is skipped and
  counted in the stderr summary, never guessed. Template `${...}` collapses to `{}`, EXCEPT a
  trailing `QS.`-prefixed interpolation, which is dropped entirely (contributes nothing, not `{}`).
- Method: `GET` default; overridden by a literal `method: "..."` directly in the 2nd-arg options
  object or inside any `oazapfts.<helper>({...})` wrapper (receiver-matched, no helper allowlist);
  upper-cased.
- Emitted consume: `kind: 'http'`, `key` via `adapter-kit`'s `resolveConsumeKey`,
  `client: 'oazapfts'` attached by post-processing the built envelope (`EnvelopeBuilder.addConsume`
  has no `client` option). `IoConsume.body` (the retired recognizer's body-shape witness) is NOT
  reproduced — `addConsume` has no `body` option and this adapter must not modify `adapter-kit`.
- Scanning is lexical (bracket/quote/template-aware balanced scanner, not a real AST); generic type
  args (`<{ status: 200 }>`) are skipped by `<`/`>` depth count; walked extensions and test-file
  exclusion follow `adapter-kit`'s `walk` defaults.

## Measured result

All 8 `test/adapter.test.mjs` cases — 7 direct ports of the retired native `oazapfts-v1` test block
in `egress.rs` plus 1 adapted body-shape-limitation case — pass with keys byte-identical to the
native recognizer's output.
