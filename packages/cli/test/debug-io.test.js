'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

const { renderDebugIo, debugIoTreeCount, debugIoTreeCountNote, BUCKETS } = require('../lib/debug-io');

// A fabricated crossLayer object covering all six buckets, deliberately fed OUT of sorted order so the
// tests also assert renderDebugIo sorts deterministically rather than echoing input order.
function fabricatedCrossLayer() {
  return {
    edges: [
      {
        kind: 'http',
        key: 'GET /health',
        from: { source: 'web', file: 'src/health.ts', line: 1 },
        to: { source: 'api', file: 'src/health.ts', line: 1 },
        crossSource: true,
      },
      {
        kind: 'http',
        key: 'GET /api/articles',
        from: { source: 'web', file: 'src/client.ts', line: 4 },
        to: { source: 'api', file: 'src/routes.ts', line: 3, symbol: 'listArticles' },
        crossSource: true,
      },
    ],
    unconsumedProvides: [
      { source: 'api', kind: 'topic', key: 'articles.created', file: 'src/routes.ts', line: 9 },
      { source: 'api', kind: 'http', key: 'GET /admin', file: 'src/admin.ts', line: 2 },
    ],
    unprovidedConsumes: [
      { source: 'web', kind: 'http', key: 'GET /api/missing', file: 'src/client.ts', line: 20 },
    ],
    unresolvedConsumes: [
      { source: 'web', kind: 'http', key: null, raw: 'sdk.fetch(x)', file: 'src/client.ts', line: 10, method: 'GET' },
      { source: 'web', kind: 'http', key: null, raw: 'wrapped(y)', file: 'src/client.ts', line: 5 },
    ],
    externalConsumes: [
      { source: 'web', kind: 'http', key: 'GET https://vendor.com/api', file: 'src/client.ts', line: 30 },
    ],
    ambiguousConsumes: [
      {
        source: 'gateway',
        kind: 'http',
        key: 'GET /shared',
        file: 'src/gw.ts',
        line: 5,
        candidates: [
          { source: 'svc-a', kind: 'http', key: 'GET /shared', file: 'a.ts', line: 1 },
          { source: 'svc-b', kind: 'http', key: 'GET /shared', file: 'b.ts', line: 1 },
        ],
      },
    ],
  };
}

test('renders one section per bucket, in a fixed order, each with a count header', () => {
  const output = renderDebugIo(fabricatedCrossLayer());
  const headerIdx = Object.fromEntries(BUCKETS.map((b) => [b, output.indexOf(`${b} (`)]));
  for (const b of BUCKETS) {
    assert.ok(headerIdx[b] > -1, `missing header for ${b}`);
  }
  // Order matches BUCKETS (edges, unconsumedProvides, unprovidedConsumes, unresolvedConsumes,
  // externalConsumes, ambiguousConsumes).
  const indices = BUCKETS.map((b) => headerIdx[b]);
  const sorted = [...indices].sort((a, b) => a - b);
  assert.deepEqual(indices, sorted);
});

test('edges section: <bucket> <from.source> <from.file>:<from.line> <key> -> <to.source> <to.file>:<to.line>, sorted by key', () => {
  const output = renderDebugIo(fabricatedCrossLayer());
  const lines = output.split('\n\n')[0].split('\n');
  assert.equal(lines[0], 'edges (2)');
  // Sorted by key: "GET /api/articles" < "GET /health"
  assert.equal(lines[1], 'edges web src/client.ts:4 GET /api/articles -> api src/routes.ts:3');
  assert.equal(lines[2], 'edges web src/health.ts:1 GET /health -> api src/health.ts:1');
});

test('unconsumedProvides section: <bucket> <source> <file>:<line> <key>', () => {
  const output = renderDebugIo(fabricatedCrossLayer());
  const section = output.split('\n\n')[1].split('\n');
  assert.equal(section[0], 'unconsumedProvides (2)');
  assert.ok(section.includes('unconsumedProvides api src/routes.ts:9 articles.created'));
  assert.ok(section.includes('unconsumedProvides api src/admin.ts:2 GET /admin'));
});

test('unprovidedConsumes section renders the resolved key', () => {
  const output = renderDebugIo(fabricatedCrossLayer());
  const section = output.split('\n\n')[2].split('\n');
  assert.equal(section[0], 'unprovidedConsumes (1)');
  assert.equal(section[1], 'unprovidedConsumes web src/client.ts:20 GET /api/missing');
});

test('unresolvedConsumes section falls back to raw, appending [method] only when present, sorted deterministically', () => {
  const output = renderDebugIo(fabricatedCrossLayer());
  const section = output.split('\n\n')[3].split('\n');
  assert.equal(section[0], 'unresolvedConsumes (2)');
  // Sorted by (source, file, line): line 5 before line 10.
  assert.equal(section[1], 'unresolvedConsumes web src/client.ts:5 wrapped(y)');
  assert.equal(section[2], 'unresolvedConsumes web src/client.ts:10 sdk.fetch(x) [GET]');
});

test('externalConsumes section renders the (already-keyed) external key', () => {
  const output = renderDebugIo(fabricatedCrossLayer());
  const section = output.split('\n\n')[4].split('\n');
  assert.equal(section[0], 'externalConsumes (1)');
  assert.equal(section[1], 'externalConsumes web src/client.ts:30 GET https://vendor.com/api');
});

test('ambiguousConsumes section lists every candidate, nothing omitted', () => {
  const output = renderDebugIo(fabricatedCrossLayer());
  const section = output.split('\n\n')[5].split('\n');
  assert.equal(section[0], 'ambiguousConsumes (1)');
  assert.equal(
    section[1],
    'ambiguousConsumes gateway src/gw.ts:5 GET /shared candidates=2: svc-a@a.ts:1, svc-b@b.ts:1'
  );
});

test('rendering the same input twice is byte-identical (determinism)', () => {
  const first = renderDebugIo(fabricatedCrossLayer());
  const second = renderDebugIo(fabricatedCrossLayer());
  assert.equal(first, second);
});

test('every bucket renders at count 0, nothing thrown, when crossLayer is absent/empty', () => {
  for (const input of [undefined, null, {}, { edges: undefined }]) {
    const output = renderDebugIo(input);
    for (const b of BUCKETS) {
      assert.match(output, new RegExp(`${b} \\(0\\)`));
    }
  }
});

// --- debugIoTreeCount / debugIoTreeCountNote — item 3's "<2 trees" explainer ---------------------------

test('debugIoTreeCount: single-tree analyze() shape (no trees array) is 1', () => {
  assert.equal(debugIoTreeCount({ findings: [], fileCount: 1 }), 1);
  assert.equal(debugIoTreeCount(undefined), 1);
  assert.equal(debugIoTreeCount(null), 1);
});

test('debugIoTreeCount: multi-tree shape counts trees[], including an explicit single-entry trees[]', () => {
  assert.equal(debugIoTreeCount({ trees: [{ sourceId: 'api' }], crossLayer: {} }), 1);
  assert.equal(debugIoTreeCount({ trees: [{ sourceId: 'api' }, { sourceId: 'web' }], crossLayer: {} }), 2);
});

test('debugIoTreeCountNote: fires for a single-tree run, naming the actual count', () => {
  assert.equal(
    debugIoTreeCountNote({ findings: [], fileCount: 1 }),
    'note: cross-layer buckets need >= 2 trees; this run analyzed 1 tree'
  );
  assert.equal(
    debugIoTreeCountNote({ trees: [{ sourceId: 'api' }], crossLayer: {} }),
    'note: cross-layer buckets need >= 2 trees; this run analyzed 1 tree'
  );
});

test('debugIoTreeCountNote: null (no note) once the run analyzed >= 2 trees', () => {
  assert.equal(
    debugIoTreeCountNote({ trees: [{ sourceId: 'api' }, { sourceId: 'web' }], crossLayer: {} }),
    null
  );
});

test('debugIoTreeCountNote: degrades gracefully (assumes 1) on an absent/malformed output', () => {
  assert.match(debugIoTreeCountNote(undefined), /analyzed 1 tree$/);
  assert.match(debugIoTreeCountNote(null), /analyzed 1 tree$/);
});
