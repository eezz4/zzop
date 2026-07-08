# Svelte adapter (Mode B overlay example)

A reference **adapter** that completes zzop's dependency graph for a Svelte / SvelteKit frontend —
without adding any Svelte-specific vocabulary to the engine.

## The problem it solves

zzop parses `.ts`/`.tsx`/`.js` natively but has no in-process `.svelte` parser. Two things fall out of
that gap:

```ts
// src/lib/actions/clickOutside.ts — only ever imported from a .svelte <script> block
export function clickOutside(node: HTMLElement) { /* ... */ }
```

A TS module imported ONLY from a `.svelte` component's `<script>` block has fan-in 0 in the native TS
dep graph, because the importer itself is invisible to the engine — it looks dead even though it isn't.
And SvelteKit convention files (`hooks.client.ts`, `+page.ts`, `+layout.server.ts`, `+error.svelte`, ...)
are loaded by the framework by filename, never imported anywhere, so their fan-in 0 is *expected*, not a
dead-code signal — but the engine has no way to know that without being taught SvelteKit's file
conventions.

## The approach

Both gaps are framework-specific, so — per zzop's direction — they're resolved by an **injected
adapter**, not by teaching the engine Svelte vocabulary:

1. Walk the frontend tree for `.svelte`, `.ts`, and `.js` files.
2. For each `.svelte` file, extract its `<script>`/`<script context="module">` bodies and parse their
   `import` statements (default, named, namespace). Native `.ts`/`.js` files are skipped —
   the engine already parses their imports natively.
3. Resolve each import specifier to a path relative to the importing file's own directory (so the
   engine's relative-import resolver picks it up with no alias config), dropping bare-package and
   `$app/*` specifiers that don't resolve to an in-repo file. A configurable `$lib` alias (default
   `$lib=src/lib`) is rewritten the same way.
4. Match every file path against SvelteKit's entry-file convention (`hooks.client`/`hooks.server`,
   `+page`/`+layout`/`+server`/`+error`, with their `.server`/`.client` variants, across `.ts`/`.js`/
   `.svelte`) and mark it `is_entry: true`.
5. Emit one `FileProjection` per walked file that either is a `.svelte` file with resolvable imports or
   matched the SvelteKit entry convention (`.svelte`/`.ts`/`.js` alike), carrying `imports` and/or
   `is_entry`, grouped into a Normalized-AST envelope.
6. Feed that envelope to zzop via the **`adapterOverlays`** config field ([Mode B overlay](../../docs/NORMALIZED_AST.md)),
   which the engine merges on top of native TypeScript analysis.

Because no native artifact exists at a `.svelte` file's path, the engine creates a synthetic artifact
from the projection and treats its `imports` exactly like a native importer's — giving the TS targets
real fan-in edges. `is_entry: true` files are unioned into the `dead-candidates` exempt set across all
configured overlays. This is "Mode B": the overlay *augments* native analysis (unlike `analyzeEnvelope`,
which *replaces* it). The engine stays framework-neutral; the SvelteKit knowledge lives only in this
adapter.

## Usage

```sh
node adapter.mjs --root <webRoot> [--lib-alias '$lib=src/lib'] > overlay.json
```

Pass the resulting envelope through your zzop config's `adapterOverlays` array (a field on the
`analyze` / `analyzeTrees` request):

```js
const overlay = JSON.parse(execSync(`node adapter.mjs --root web`));
const out = JSON.parse(native.analyzeTrees(JSON.stringify({
  trees: [
    { root: 'web', sourceId: 'web', adapterOverlays: [overlay] },
  ],
})));
```

## Worked result (immich)

Against immich's SvelteKit `web/` frontend, running `dead-candidates` with vs. without this adapter's
overlay:

| | dead-candidates findings |
|---|---|
| native only (no overlay) | 204 |
| with the overlay | 48 |

The overlay clears the false positives that came from `.svelte`-only importers and SvelteKit entry
conventions. The remaining 48 are candidates the adapter doesn't resolve — reference-adapter recall,
not an engine contract limit (see Limitations).

## Limitations (a production adapter can go further)

- Import parsing is regex-based over the extracted `<script>` body, not a real Svelte/TS parser — it
  covers the common import shapes but is not a full grammar.
- Only `.svelte` files get import projection; if a `.svelte` file re-exports something another
  `.svelte` file relies on, that chain isn't followed.
- The SvelteKit entry regex covers the standard convention files; custom entry points (e.g. a
  non-standard framework integration) aren't recognized.
- Single `$lib`-style alias only; multi-alias `svelte.config.js`/`tsconfig.json` path maps aren't read.
- `parseImports` only matches `import ... from '<spec>'`; a bare side-effect import (e.g. `import './x.css'`) has no `from` clause and is never captured.
