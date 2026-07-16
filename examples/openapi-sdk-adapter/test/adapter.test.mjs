// Snapshot test: runs adapter.mjs as a subprocess against a minimal, inline-written fixture tree and
// deep-equals the parsed envelope JSON against a committed expected object. This is the byte-parity
// gate for the openapi-sdk-adapter -> adapter-kit refactor: written and greenlit against the
// pre-refactor code first, then re-checked after the refactor to catch any output drift.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ADAPTER = path.join(__dirname, '..', 'adapter.mjs');

// Post-refactor, every file entry goes through adapter-kit's EnvelopeBuilder, which always emits the
// FULL FileProjection shape (symbols/imports/re_exports/dynamic_imports/used_names/
// const_map_fragment/procedure_router_fragments/router_mount_fragments/degraded/is_entry, all at their zero
// values when unset) instead of the pre-refactor adapter's sparse `{path, loc, io}` object. This is a
// deliberate, flagged consequence of adopting the kit (schema-complete output, matching the Rust
// engine's FileProjection field-for-field) — not a semantic change to what gets keyed. This helper
// documents the exact padding so the expected objects below stay honest about the shape.
function fileProjection({ path: p, loc, io }) {
  return {
    path: p,
    loc,
    symbols: [],
    imports: {},
    re_exports: [],
    dynamic_imports: [],
    used_names: [],
    const_map_fragment: {},
    procedure_router_fragments: [],
    router_mount_fragments: [],
    io,
    degraded: false,
    is_entry: false,
  };
}

function makeFixture() {
  const root = mkdtempSync(path.join(tmpdir(), 'openapi-sdk-adapter-test-'));
  const specPath = path.join(root, 'openapi.json');
  writeFileSync(
    specPath,
    JSON.stringify({
      servers: [{ url: '/api' }],
      paths: {
        '/users/{id}': { get: { operationId: 'getUser' } },
        '/users': { post: { operationId: 'createUser' } },
      },
    })
  );
  const feRoot = path.join(root, 'fe');
  mkdirSync(path.join(feRoot, 'src'), { recursive: true });
  writeFileSync(
    path.join(feRoot, 'src', 'api-consumer.ts'),
    [
      "import { getUser, createUser } from '@immich/sdk';",
      '',
      'async function load(id) {',
      '  const user = await getUser(id);',
      '  return user;',
      '}',
      '',
      'async function add() {',
      "  return createUser({ name: 'x' });",
      '}',
      '',
    ].join('\n')
  );
  // A file with no SDK import at all — must not appear in the output.
  writeFileSync(path.join(feRoot, 'src', 'noop.ts'), 'export const x = 1;\n');
  return { root, specPath, feRoot };
}

test('openapi-sdk-adapter --mode consume: envelope matches committed snapshot', () => {
  const { root, specPath, feRoot } = makeFixture();
  try {
    const stdout = execFileSync(
      process.execPath,
      [ADAPTER, '--mode', 'consume', '--root', feRoot, '--spec', specPath],
      { encoding: 'utf8' }
    );
    const envelope = JSON.parse(stdout);
    assert.deepEqual(envelope, {
      format: 'zzop-normalized-ast',
      version: 1,
      parser: 'openapi-sdk-adapter/1',
      source: 'api',
      files: [
        fileProjection({
          path: 'src/api-consumer.ts',
          loc: 11,
          io: {
            provides: [],
            consumes: [
              { kind: 'http', key: 'GET /api/users/{}', file: 'src/api-consumer.ts', line: 4 },
              { kind: 'http', key: 'POST /api/users', file: 'src/api-consumer.ts', line: 9 },
            ],
          },
        }),
      ],
    });
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('openapi-sdk-adapter --mode provide: envelope matches committed snapshot', () => {
  const { root, specPath } = makeFixture();
  try {
    const stdout = execFileSync(process.execPath, [ADAPTER, '--mode', 'provide', '--spec', specPath], {
      encoding: 'utf8',
    });
    const envelope = JSON.parse(stdout);
    assert.deepEqual(envelope, {
      format: 'zzop-normalized-ast',
      version: 1,
      parser: 'openapi-sdk-adapter/1',
      source: 'api',
      files: [
        fileProjection({
          path: 'openapi.spec',
          loc: 1,
          io: {
            provides: [
              { kind: 'http', key: 'GET /api/users/{}', file: 'openapi.spec', line: 1 },
              { kind: 'http', key: 'POST /api/users', file: 'openapi.spec', line: 1 },
            ],
            consumes: [],
          },
        }),
      ],
    });
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
