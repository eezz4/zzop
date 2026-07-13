# adapter-kit

Shared boilerplate for zzop external-parser / adapter-overlay producers, extracted from the ~70%
of code every hand-rolled JS adapter in this repo's `examples/` tree (`openapi-sdk-adapter`,
`react-query-adapter`, `wrapper-adapter`, `svelte-adapter`) re-derives on its own: recursive file
walking, `NormalizedEnvelope` assembly, and HTTP key normalization that is byte-identical to
`zzop_core`'s Rust implementation. Plain JS, no dependencies, not published to npm — this is a
reference kit meant to be copied or imported directly from within this repo (see
[`docs/adapters/README.md`](../../docs/adapters/README.md) for the envelope contract and its
versioning policy).

## The three pieces

- **`lib/walk.js`** — `walk(root, { include, exclude, excludeFile, skipDirs })` returns a
  deterministic, lexically sorted, forward-slash file list, skipping `node_modules`/`.git` by
  default.
- **`lib/keys.js`** — `normalizeProvideKey(method, path)` / `normalizeConsumeKey(method, url)` are
  exact ports of `zzop_core::http_interface_key` / `http_consume_interface_key`
  (`packages/core/src/io.rs`); `resolveConsumeKey(method, url)` additionally applies the
  internal/external/base-relative veto list every adapter needs before it can key a raw call-site
  literal at all (ported from `consume_key_for`/`base_relative_path` in
  `parser/parser-typescript/src/adapters/egress.rs`). Cross-layer linking is an exact string join —
  a key computed even slightly differently than the engine's silently fails to join, so
  `test/keys.test.js` replays every row of
  [`docs/adapters/key-normalization.fixture.json`](../../docs/adapters/key-normalization.fixture.json)
  against these functions on every test run.
- **`lib/envelope.js`** — `EnvelopeBuilder` assembles a schema-valid envelope
  (`docs/adapters/envelope.schema.json`) from four calls: `addFile`, `addProvide`, `addConsume`,
  `markEntry`. `toEnvelope()` validates the result (`validateEnvelope`, mirroring
  `zzop_core::validate_envelope`) and throws with every issue listed at once rather than handing back
  a broken envelope.

Import everything from the package root:

```js
import { walk, normalizeConsumeKey, EnvelopeBuilder } from '../adapter-kit/index.js';
```

## How they compose

A typical adapter is: **walk** the tree -> for each file, extract raw facts and **key** them ->
**assemble** the envelope -> write it to stdout. With the kit, that main loop collapses to:

```js
import { readFileSync } from 'node:fs';
import { walk, resolveConsumeKey, EnvelopeBuilder } from '../adapter-kit/index.js';

const root = process.argv[2];
const builder = new EnvelopeBuilder({ parser: 'my-adapter/1', source: 'web' });

for (const rel of walk(root, { include: ['ts', 'tsx'] })) {
  const text = readFileSync(`${root}/${rel}`, 'utf8');
  builder.addFile(rel, { loc: text.split('\n').length });

  for (const { url, line } of findCallSites(text)) {       // adapter-specific extraction
    const key = resolveConsumeKey('GET', url);              // null when unresolvable — never guessed
    builder.addConsume(rel, { kind: 'http', key, line, raw: key === null ? url : undefined });
  }
}

process.stdout.write(JSON.stringify(builder.toEnvelope()));
```

## Mode A vs Mode B

The kit produces the same envelope shape either way — the difference is what you populate and how
the engine consumes it (see [`docs/NORMALIZED_AST.md`](../../docs/NORMALIZED_AST.md) for the full
contract):

- **Mode A — full envelope** (a new language/parser the engine has no in-workspace crate for). Emit a
  complete projection per file: `symbols`, `imports`, `io`, everything you can extract. Feed the
  output to `analyzeEnvelope(envelopeJson, configJson)` — it *replaces* native analysis for that tree.
  Use `addFile`'s `opts` to set `symbols`/`imports`/`re_exports` alongside `addProvide`/`addConsume`
  for `io`.
- **Mode B — adapter overlay** (adding framework knowledge on top of a language zzop already parses
  natively, e.g. a router convention or an SDK client). Emit a *partial* envelope — usually only `io`
  via `addProvide`/`addConsume`, plus `markEntry` for framework-loaded files — and leave
  `symbols`/`imports` empty. Feed the output into a tree's `adapterOverlays` array on an
  `analyze`/`analyzeTrees` request; the engine merges it on top of native analysis, deduping
  identical `(kind, key, file, line)` entries.

Both modes go through the same `validateEnvelope`/schema check before the engine trusts them —
an invalid Mode B overlay is skipped with a warning rather than failing the whole analysis; an invalid
Mode A envelope simply cannot be analyzed.

## Testing

```sh
node --test test/*.test.js
```

Three suites: `keys.test.js` (parity-replays the full key-normalization fixture, plus veto-list unit
tests for `resolveConsumeKey`), `envelope.test.js` (`EnvelopeBuilder` valid-output and error-case
coverage), `walk.test.js` (determinism, filtering, default skip-dirs).
