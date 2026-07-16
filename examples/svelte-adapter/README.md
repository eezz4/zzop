# Svelte adapter (Mode B overlay example)

## What it does

A reference adapter that completes zzop's dependency graph for a Svelte / SvelteKit frontend. zzop
parses `.ts`/`.tsx`/`.js` natively but not `.svelte`, so a TS module imported only from a `.svelte`
`<script>` block has fan-in 0 and false-positives `dead-candidates`; SvelteKit convention files
(`hooks.client.ts`, `+page.ts`, `+layout.server.ts`, ...) are loaded by the framework by filename,
never imported, so their fan-in 0 is expected — not a dead-code signal. This adapter extracts
`import` statements from `.svelte` `<script>` blocks and matches SvelteKit entry conventions,
projecting per-file `FileProjection`s that carry dep-graph `imports` and/or `is_entry` in a
[Mode B overlay](../../docs/NORMALIZED_AST.md) envelope (`adapterOverlays`), keeping SvelteKit
vocabulary out of the engine. Overlay background:
[docs/adapters/README.md](../../docs/adapters/README.md).

## Run

```sh
# emit the overlay, then pass it via the analyze/analyzeTrees `adapterOverlays` array
node adapter.mjs --root <webRoot> [--lib-alias '$lib=src/lib'] > overlay.json

# tests
node --test test/adapter.test.mjs
```

## Contract points

- Channel: dep-graph `imports` + `is_entry` — this adapter emits no `io` facts. The engine creates a
  synthetic artifact for each `.svelte` path and treats its `imports` exactly like a native
  importer's, giving the imported TS targets real fan-in; `is_entry: true` files are unioned into
  the `dead-candidates` exempt set across all configured overlays.
- Import specifiers are emitted **relative to the importing file's directory**, so the engine's
  relative-import resolver picks them up with no alias config; bare-package and `$app/*` specifiers
  are dropped; a single `$lib`-style alias (default `$lib=src/lib`, `--lib-alias`) is rewritten the
  same way.
- Entry matching: `hooks.client`/`hooks.server` and `+page`/`+layout`/`+server`/`+error` (with
  `.server`/`.client` variants) across `.ts`/`.js`/`.svelte`.
- Envelope: `zzop-normalized-ast` v1, `parser: svelte-adapter/1`; only a file with resolvable
  `.svelte` imports or an entry match is projected — native `.ts`/`.js` imports are left to the
  engine ([schema](../../docs/adapters/envelope.schema.json)).

## Measured result

immich's SvelteKit `web/` frontend: `dead-candidates` 204 (native only) -> 48 (with the overlay) —
the remaining 48 are reference-adapter recall limits, not an engine contract limit.
