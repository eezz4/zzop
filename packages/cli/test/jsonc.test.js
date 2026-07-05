'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

const { stripJsonComments } = require('../lib/jsonc');
const { CONFIG_TEMPLATE } = require('../lib/init');

function roundtrip(jsonc) {
  return JSON.parse(stripJsonComments(jsonc));
}

test('strips line comments', () => {
  assert.deepEqual(roundtrip('{\n  "a": 1 // trailing\n}'), { a: 1 });
});

test('strips block comments (incl. multi-line)', () => {
  assert.deepEqual(roundtrip('{\n  /* block\n   * comment */\n  "a": 1\n}'), { a: 1 });
});

test('preserves // inside string values', () => {
  assert.deepEqual(roundtrip('{ "url": "https://example.com/x" }'), {
    url: 'https://example.com/x',
  });
});

test('preserves /* */ inside string values', () => {
  assert.deepEqual(roundtrip('{ "glob": "a/*/b", "c": "/* not a comment */" }'), {
    glob: 'a/*/b',
    c: '/* not a comment */',
  });
});

test('preserves escaped quotes then real comment', () => {
  assert.deepEqual(roundtrip('{ "a": "he said \\"hi\\"" /* c */ }'), { a: 'he said "hi"' });
});

test('strips trailing commas in objects and arrays', () => {
  assert.deepEqual(roundtrip('{ "a": [1, 2, ], "b": 3, }'), { a: [1, 2], b: 3 });
});

test('does not strip commas inside strings', () => {
  assert.deepEqual(roundtrip('{ "a": "1, 2, " }'), { a: '1, 2, ' });
});

test('the init template is valid, runnable JSONC', () => {
  const config = roundtrip(CONFIG_TEMPLATE);
  assert.deepEqual(config.roots, ['.']);
  assert.equal(config.format, 'pretty');
  assert.equal(config.failOn, 'warn');
  assert.ok(typeof config.rules === 'object');
});
