# adapter-kit

Shared library for zzop external-parser / adapter-overlay producers, extracted from the JS example
adapters' formerly hand-rolled copies (they now import it): deterministic file walking
(`lib/walk.js`), `NormalizedEnvelope` assembly (`lib/envelope.js`), and HTTP key normalization
byte-identical to `zzop_core`'s Rust implementation (`lib/keys.js`). Plain JS, no dependencies, not
published to npm — copy it or import it from within this repo. Envelope contract and versioning:
[`docs/adapters/README.md`](../../docs/adapters/README.md).

## Run

```js
import { walk, resolveConsumeKey, EnvelopeBuilder } from '../adapter-kit/index.js';

const builder = new EnvelopeBuilder({ parser: 'my-adapter/1', source: 'web' });
for (const rel of walk(root, { include: ['ts', 'tsx'] })) {
  builder.addFile(rel, { loc });
  builder.addConsume(rel, { kind: 'http', key: resolveConsumeKey('GET', url), line });
}
process.stdout.write(JSON.stringify(builder.toEnvelope()));
```

Tests: `node --test test/*.test.js`

## Contract points

- **Keying** — `normalizeProvideKey`/`normalizeConsumeKey` are exact ports of
  `zzop_core::http_interface_key`/`http_consume_interface_key` (`crates/core/src/io.rs`);
  `resolveConsumeKey` adds the internal/external/base-relative veto list from
  `parser/parser-typescript/src/adapters/egress.rs`. Cross-layer linking is an exact string join —
  a key computed even slightly differently silently fails to join, so `test/keys.test.js` replays
  every row of
  [`docs/adapters/key-normalization.fixture.json`](../../docs/adapters/key-normalization.fixture.json).
- **Envelope** — `EnvelopeBuilder` (`addFile`/`addProvide`/`addConsume`/`markEntry`) emits a
  schema-valid envelope ([`docs/adapters/envelope.schema.json`](../../docs/adapters/envelope.schema.json));
  `toEnvelope()` runs `validateEnvelope` (mirroring `zzop_core::validate_envelope`, including both
  the canonical camelCase `bodyStart`/`bodyEnd` and the frozen-v1 snake_case alias) and throws with
  every issue at once. Same shape serves Mode A (full envelope → `analyzeEnvelope`) and Mode B
  (partial `io` overlay → `adapterOverlays`) — see
  [`docs/NORMALIZED_AST.md`](../../docs/NORMALIZED_AST.md).
- **Walk** — `walk(root, { include, exclude, excludeFile, skipDirs })` returns a lexically sorted,
  forward-slash, repo-relative list (deterministic across platforms), skipping
  `node_modules`/`.git` by default.

## Measured result

As of 2026-07-16: `node --test test/*.test.js` — 58/58 tests pass (keys fixture parity replay,
envelope builder/validator, walk determinism).
