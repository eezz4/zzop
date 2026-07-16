import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  EnvelopeBuilder,
  validateEnvelope,
  NORMALIZED_AST_FORMAT,
  SUPPORTED_NORMALIZED_AST_VERSION,
} from '../lib/envelope.js';

test('EnvelopeBuilder produces a schema-shaped minimal envelope', () => {
  const b = new EnvelopeBuilder({ parser: 'test-adapter/1', source: 'web' });
  b.addFile('src/api.ts', { loc: 10 });
  const envelope = b.toEnvelope();

  assert.equal(envelope.format, NORMALIZED_AST_FORMAT);
  assert.equal(envelope.version, SUPPORTED_NORMALIZED_AST_VERSION);
  assert.equal(envelope.parser, 'test-adapter/1');
  assert.equal(envelope.source, 'web');
  assert.equal(envelope.files.length, 1);
  assert.equal(envelope.files[0].path, 'src/api.ts');
  assert.deepEqual(envelope.files[0].io, { provides: [], consumes: [] });
  assert.equal(envelope.files[0].is_entry, false);
  assert.deepEqual(validateEnvelope(envelope), []);
});

test('addProvide/addConsume/markEntry populate io and is_entry', () => {
  const b = new EnvelopeBuilder({ parser: 'test-adapter/1', source: 'api' });
  b.addFile('src/routes.ts', { loc: 5 });
  b.addProvide('src/routes.ts', { kind: 'http', key: 'GET /users', line: 3 });
  b.addConsume('src/routes.ts', { kind: 'http', key: 'GET /orders', line: 4 });
  b.markEntry('src/routes.ts');

  const envelope = b.toEnvelope();
  const file = envelope.files[0];
  assert.equal(file.io.provides.length, 1);
  assert.deepEqual(file.io.provides[0], { kind: 'http', key: 'GET /users', file: 'src/routes.ts', line: 3 });
  assert.equal(file.io.consumes.length, 1);
  assert.equal(file.is_entry, true);
});

test('addConsume defaults key to null — unresolved, never guessed', () => {
  const b = new EnvelopeBuilder({ parser: 'test-adapter/1', source: 'web' });
  b.addFile('src/api.ts', { loc: 1 });
  b.addConsume('src/api.ts', { kind: 'http', line: 1, raw: 'someDynamicUrl', method: 'GET' });

  const envelope = b.toEnvelope();
  const consume = envelope.files[0].io.consumes[0];
  assert.equal(consume.key, null);
  assert.equal(consume.raw, 'someDynamicUrl');
  assert.equal(consume.method, 'GET');
});

test('constructor rejects a missing parser/source id', () => {
  assert.throws(() => new EnvelopeBuilder({ source: 'web' }), /parser id is required/);
  assert.throws(() => new EnvelopeBuilder({ parser: 'x/1' }), /source id is required/);
});

test('addFile rejects a duplicate path', () => {
  const b = new EnvelopeBuilder({ parser: 'x/1', source: 'web' });
  b.addFile('a.ts', { loc: 1 });
  assert.throws(() => b.addFile('a.ts', { loc: 2 }), /duplicate path/);
});

test('addProvide/addConsume/markEntry reject an unregistered file', () => {
  const b = new EnvelopeBuilder({ parser: 'x/1', source: 'web' });
  assert.throws(() => b.addProvide('missing.ts', { kind: 'http', key: 'GET /x', line: 1 }), /unknown file/);
  assert.throws(() => b.addConsume('missing.ts', { kind: 'http', line: 1 }), /unknown file/);
  assert.throws(() => b.markEntry('missing.ts'), /unknown file/);
});

test('addProvide rejects a missing/empty key or non-positive line', () => {
  const b = new EnvelopeBuilder({ parser: 'x/1', source: 'web' });
  b.addFile('a.ts', { loc: 1 });
  assert.throws(() => b.addProvide('a.ts', { kind: 'http', line: 1 }), /key is required/);
  assert.throws(() => b.addProvide('a.ts', { kind: 'http', key: 'GET /x', line: 0 }), /positive 1-based integer/);
});

test('validateEnvelope flags duplicate paths and body_end < body_start', () => {
  const envelope = {
    format: NORMALIZED_AST_FORMAT,
    version: 1,
    parser: 'x/1',
    source: 'web',
    files: [
      { path: 'a.ts', loc: 1, symbols: [{ name: 'f', body_start: 10, body_end: 5 }] },
      { path: 'a.ts', loc: 1 },
    ],
  };
  const errors = validateEnvelope(envelope);
  assert.ok(errors.some((e) => e.includes('duplicate path')));
  assert.ok(errors.some((e) => e.includes('body_end')));
});

test('validateEnvelope reads the canonical camelCase bodyStart/bodyEnd spelling too', () => {
  // Wire-canonical names are camelCase (envelope.schema.json); snake_case is a frozen-v1 input
  // alias. Both spellings must trip the same body-range check, matching zzop_core's serde aliases.
  const base = { format: NORMALIZED_AST_FORMAT, version: 1, parser: 'x/1', source: 'web' };
  const camel = validateEnvelope({
    ...base,
    files: [{ path: 'a.ts', loc: 1, symbols: [{ name: 'f', bodyStart: 10, bodyEnd: 5 }] }],
  });
  assert.ok(camel.some((e) => e.includes('body_end')));
  const valid = validateEnvelope({
    ...base,
    files: [{ path: 'a.ts', loc: 1, symbols: [{ name: 'f', bodyStart: 5, bodyEnd: 10 }] }],
  });
  assert.deepEqual(valid, []);
});

test('validateEnvelope flags unknown format and unsupported version', () => {
  const errors = validateEnvelope({ format: 'bogus', version: 99, parser: 'x', source: 's', files: [] });
  assert.ok(errors.some((e) => e.includes('unknown format')));
  assert.ok(errors.some((e) => e.includes('unsupported version')));
});

test('toEnvelope throws (not silently emits) an invalid envelope', () => {
  // addFile's opts set FileProjection fields verbatim, so an invalid symbol body range is reachable
  // through the public API — toEnvelope must refuse to emit it.
  const bad = new EnvelopeBuilder({ parser: 'x/1', source: 'web' });
  bad.addFile('a.ts', { loc: 1, symbols: [{ name: 'f', bodyStart: 10, bodyEnd: 5 }] });
  assert.throws(() => bad.toEnvelope(), /invalid envelope[\s\S]*body_end/);

  const ok = new EnvelopeBuilder({ parser: 'x/1', source: 'web' });
  ok.addFile('a.ts', { loc: 1 });
  assert.doesNotThrow(() => ok.toEnvelope());
});
