// Snapshot test: runs adapter.mjs as a subprocess against a minimal, inline-written fixture tree and
// deep-equals the parsed envelope JSON against a committed expected object. Fixture exercises
// wrapper-adapter's normalization edge cases (a `#fragment` literal, a base-relative literal with no
// leading slash) — all delegated to adapter-kit's `resolveConsumeKey`/`normalizeConsumeKey` (matching
// `zzop_core`/`parser-typescript`'s own consume-key semantics exactly), so this guards parity WITH
// the kit: a `#fragment` suffix drops like a `?query` suffix, and a base-relative literal (no leading
// `/`) resolves via `baseRelativePath`.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ADAPTER = path.join(__dirname, '..', 'adapter.mjs');

// adapter-kit's EnvelopeBuilder always emits the full FileProjection shape (all fields present, at
// their zero values when unset); this helper pads the sparse expectation to that shape.
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
  const root = mkdtempSync(path.join(tmpdir(), 'wrapper-adapter-test-'));
  mkdirSync(path.join(root, 'src'), { recursive: true });
  writeFileSync(
    path.join(root, 'src', 'service.ts'),
    [
      "import { requests } from './agent';",
      '',
      'export function getArticles() {',
      "  return requests.get('/articles');",
      '}',
      '',
      'export function login(user) {',
      "  return requests.post('/users/login', { user });",
      '}',
      '',
      'export function getComments(slug) {',
      '  return requests.get(`/articles/${slug}/comments`);',
      '}',
      '',
      'export function byId(id) {',
      '  return requests.get(`/articles/:id`);',
      '}',
      '',
      'export function baseRelative() {',
      "  return requests.get('users/login');",
      '}',
      '',
      'export function withHash() {',
      "  return requests.get('/articles#section');",
      '}',
      '',
    ].join('\n')
  );
  return root;
}

test('wrapper-adapter: envelope matches committed snapshot (incl. normalization edge cases)', () => {
  const root = makeFixture();
  try {
    const stdout = execFileSync(process.execPath, [ADAPTER, '--root', root, '--source', 'web'], {
      encoding: 'utf8',
    });
    const envelope = JSON.parse(stdout);
    assert.deepEqual(envelope, {
      format: 'zzop-normalized-ast',
      version: 1,
      parser: 'wrapper-adapter/1',
      source: 'web',
      files: [
        fileProjection({
          path: 'src/service.ts',
          loc: 26,
          io: {
            provides: [],
            consumes: [
              { kind: 'http', key: 'GET /articles', file: 'src/service.ts', line: 4 },
              { kind: 'http', key: 'POST /users/login', file: 'src/service.ts', line: 8 },
              { kind: 'http', key: 'GET /articles/{}/comments', file: 'src/service.ts', line: 12 },
              { kind: 'http', key: 'GET /articles/{}', file: 'src/service.ts', line: 16 },
              // baseRelative(): a literal with NO leading slash resolves via `resolveConsumeKey`'s
              // `baseRelativePath` branch.
              { kind: 'http', key: 'GET /users/login', file: 'src/service.ts', line: 20 },
              // withHash(): a trailing `#fragment` drops, same as a `?query` suffix
              // (`normalizeConsumeKey` splits on `[?#]`).
              { kind: 'http', key: 'GET /articles', file: 'src/service.ts', line: 24 },
            ],
          },
        }),
      ],
    });
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
