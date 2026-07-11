'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

const { lintEnvelope, isAbsolutePath } = require('../lib/validate');

function envelope(files) {
  return {
    format: 'zzop-normalized-ast',
    version: 1,
    parser: 'test/1',
    source: 'legacy',
    files,
  };
}

function fileWithIo({ path = 'a.ts', provides = [], consumes = [] } = {}) {
  return { path, loc: 1, io: { provides, consumes } };
}

test('a clean envelope produces no hints', () => {
  const env = envelope([
    fileWithIo({
      provides: [{ kind: 'http', key: 'GET /users/{}', file: 'a.ts', line: 1 }],
      consumes: [{ kind: 'http', key: 'GET /orders/{}', file: 'a.ts', line: 2 }],
    }),
  ]);
  assert.deepEqual(lintEnvelope(env), []);
});

test('an unrecognized/malformed envelope shape never throws', () => {
  assert.deepEqual(lintEnvelope(null), []);
  assert.deepEqual(lintEnvelope({}), []);
  assert.deepEqual(lintEnvelope({ files: 'not-an-array' }), []);
  assert.deepEqual(lintEnvelope({ files: [{ path: 'a.ts', io: null }] }), []);
  assert.deepEqual(lintEnvelope({ files: [{ path: 'a.ts', io: { provides: 'nope' } }] }), []);
});

// --- Check 1: unnormalized http key (^[A-Z]+ /) ------------------------------------------------------

test('an unnormalized http provide key is flagged', () => {
  const env = envelope([
    fileWithIo({ provides: [{ kind: 'http', key: 'get /users', file: 'a.ts', line: 5 }] }),
  ]);
  const hints = lintEnvelope(env);
  assert.equal(hints.length, 1);
  assert.match(hints[0], /Provide at a\.ts:5/);
  assert.match(hints[0], /does not match the normalized "METHOD \/path" shape/);
  assert.match(hints[0], /http_interface_key/);
});

test('an unnormalized http consume key is flagged', () => {
  const env = envelope([
    fileWithIo({ consumes: [{ kind: 'http', key: 'GET/users', file: 'b.ts', line: 9 }] }),
  ]);
  const hints = lintEnvelope(env);
  assert.equal(hints.length, 1);
  assert.match(hints[0], /Consume at b\.ts:9/);
  assert.match(hints[0], /http_consume_interface_key/);
});

test('a normalized http key ("METHOD /path") is not flagged', () => {
  const env = envelope([
    fileWithIo({
      provides: [{ kind: 'http', key: 'POST /orders/{}', file: 'a.ts', line: 1 }],
      consumes: [{ kind: 'http', key: 'DELETE /orders/{}', file: 'a.ts', line: 2 }],
    }),
  ]);
  assert.deepEqual(lintEnvelope(env), []);
});

test('a non-http kind is never checked against the http key shape', () => {
  const env = envelope([
    fileWithIo({ provides: [{ kind: 'db-table', key: 'table:users', file: 'a.ts', line: 1 }] }),
  ]);
  assert.deepEqual(lintEnvelope(env), []);
});

test('an unresolved consume (key: null) is never flagged as unnormalized', () => {
  const env = envelope([
    fileWithIo({ consumes: [{ kind: 'http', key: null, file: 'a.ts', line: 1, raw: 'url' }] }),
  ]);
  assert.deepEqual(lintEnvelope(env), []);
});

// --- Check 2: host-carrying provide key ---------------------------------------------------------------

test('a provide key containing "://" is flagged', () => {
  const env = envelope([
    fileWithIo({
      provides: [{ kind: 'http', key: 'GET https://vendor.example.com/api', file: 'a.ts', line: 3 }],
    }),
  ]);
  const hints = lintEnvelope(env);
  // Check 1 (not "METHOD /path"-shaped) may ALSO fire on this key — check 2 just needs to be present.
  assert.ok(
    hints.some((h) => h.startsWith('Provide at a.ts:3') && h.includes('contains "://"')),
    `expected a "://" hint for a.ts:3, got: ${JSON.stringify(hints)}`
  );
});

test('a consume key containing "://" is NOT flagged (external egress is consume-side only)', () => {
  const env = envelope([
    fileWithIo({
      consumes: [{ kind: 'http', key: 'GET https://vendor.example.com/api', file: 'a.ts', line: 4 }],
    }),
  ]);
  const hints = lintEnvelope(env);
  // Check 2 (host-carrying key) only ever runs over `provides`, never `consumes` — a raw URL in a
  // consume key is expected external egress, not a mistake. (Check 1 may still fire on this same key
  // since it also isn't "METHOD /path"-shaped, and its message legitimately quotes the key verbatim —
  // so this asserts the check-2-specific wording is absent, not that "://" never appears anywhere.)
  assert.ok(!hints.some((h) => h.includes('external egress')));
});

// --- Check 3: duplicate identical provide at the same file:line ----------------------------------------

test('two identical provides at the same file:line are flagged as a duplicate', () => {
  const env = envelope([
    fileWithIo({
      provides: [
        { kind: 'http', key: 'GET /users/{}', file: 'a.ts', line: 10 },
        { kind: 'http', key: 'GET /users/{}', file: 'a.ts', line: 10 },
      ],
    }),
  ]);
  const hints = lintEnvelope(env);
  assert.ok(hints.some((h) => /Duplicate provide at a\.ts:10/.test(h)));
});

test('two provides with the same key at DIFFERENT lines are not flagged as duplicates', () => {
  const env = envelope([
    fileWithIo({
      provides: [
        { kind: 'http', key: 'GET /users/{}', file: 'a.ts', line: 10 },
        { kind: 'http', key: 'GET /users/{}', file: 'a.ts', line: 20 },
      ],
    }),
  ]);
  const hints = lintEnvelope(env);
  assert.ok(!hints.some((h) => h.includes('Duplicate provide')));
});

test('two DIFFERENT provides at the same file:line are not flagged as duplicates', () => {
  const env = envelope([
    fileWithIo({
      provides: [
        { kind: 'http', key: 'GET /users/{}', file: 'a.ts', line: 10 },
        { kind: 'http', key: 'POST /users/{}', file: 'a.ts', line: 10 },
      ],
    }),
  ]);
  const hints = lintEnvelope(env);
  assert.ok(!hints.some((h) => h.includes('Duplicate provide')));
});

// --- Check 4: absolute file paths -----------------------------------------------------------------------

test('an absolute POSIX file path is flagged', () => {
  const env = envelope([fileWithIo({ path: '/abs/src/a.ts' })]);
  const hints = lintEnvelope(env);
  assert.ok(hints.some((h) => h.includes('files[0].path "/abs/src/a.ts" is an absolute path')));
});

test('an absolute Windows file path (drive letter) is flagged', () => {
  const env = envelope([fileWithIo({ path: 'C:\\repo\\src\\a.ts' })]);
  const hints = lintEnvelope(env);
  assert.ok(hints.some((h) => h.includes('is an absolute path')));
});

test('a tree-relative file path is not flagged', () => {
  const env = envelope([fileWithIo({ path: 'src/a.ts' })]);
  assert.deepEqual(lintEnvelope(env), []);
});

test('isAbsolutePath recognizes POSIX, Windows-drive, and UNC forms; rejects relative paths', () => {
  assert.equal(isAbsolutePath('/a/b.ts'), true);
  assert.equal(isAbsolutePath('C:\\a\\b.ts'), true);
  assert.equal(isAbsolutePath('C:/a/b.ts'), true);
  assert.equal(isAbsolutePath('\\\\host\\share\\b.ts'), true);
  assert.equal(isAbsolutePath('src/a.ts'), false);
  assert.equal(isAbsolutePath('./src/a.ts'), false);
  assert.equal(isAbsolutePath(''), false);
  assert.equal(isAbsolutePath(undefined), false);
});
