// Snapshot test: runs adapter.mjs as a subprocess against the COMMITTED fixture tree and deep-equals
// the parsed envelope against test/expected-envelope.json — the same bytes the engine contract
// validator accepts, so a change to either side shows up here first.
//
// The fixture is the shape this adapter exists for in miniature: `train.py` imports itself through an
// alias (`mypkg`) that NO file in the tree declares, so the native parser binds every one of those
// names to a specifier that resolves to nothing. All three Python import forms are covered, because
// their local-name rules differ and getting one wrong produces a sibling binding instead of a
// replacement: `from a.b import c` binds `c`, `import a.b.c` binds only `a`.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ADAPTER = path.join(__dirname, '..', 'adapter.mjs');
const FIXTURE = path.join(__dirname, 'fixture');
const EXPECTED = JSON.parse(readFileSync(path.join(__dirname, 'expected-envelope.json'), 'utf8'));

function run() {
  const stdout = execFileSync(process.execPath, [
    ADAPTER,
    '--root', FIXTURE,
    '--alias', 'mypkg',
    '--source', 'demo',
  ], { encoding: 'utf8' });
  return JSON.parse(stdout);
}

test('python-package-alias-adapter: envelope matches the committed snapshot', () => {
  assert.deepEqual(run(), EXPECTED);
});

test('declares the overrides version floor, because it displaces rather than adds', () => {
  const envelope = run();
  // The floor is a RELEASE number since 0.27.0 (docs/NORMALIZED_AST.md), not a counter — an engine
  // below it drops `overrides` silently, so declaring less would make this correction a no-op there.
  assert.equal(envelope.version, '0.27.0');
  // Every projected binding is one the native parser also bound (to the dead alias specifier), so every
  // one of them must be declared — an undeclared collision loses to the native side and the correction
  // silently does nothing. This is the defect the corpus run surfaced: 179 of 188 bindings dropped.
  for (const file of envelope.files) {
    assert.deepEqual(
      [...file.overrides.imports].sort(),
      Object.keys(file.imports).sort(),
      `${file.path}: every binding must declare its displacement`
    );
  }
});

test('local names follow CPython binding rules, not the specifier text', () => {
  const { imports } = run().files.find((f) => f.path === 'train.py');
  // `from mypkg.core.config import base_config` binds the imported name...
  assert.ok(imports.base_config, 'from-import binds the imported name');
  // ...and `import mypkg.common.utils` binds ONLY the top-level segment. Keying this as
  // `mypkg.common.utils` would miss the native binding entirely and arrive as a sibling entry.
  assert.ok(imports.mypkg, 'plain import binds the top-level package segment');
  assert.equal(imports.mypkg.original, '*');
});
