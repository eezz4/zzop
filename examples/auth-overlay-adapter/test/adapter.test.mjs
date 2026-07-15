// Snapshot test: runs adapter.mjs as a subprocess against a minimal, inline-written fixture tree and
// deep-equals the parsed envelope JSON against a committed expected object. Fixture exercises: a
// router-level `app.use('/admin', requireAuth)` auth registration (must inject an `auth-guarded`
// PathScope attribute for `/admin`) and a non-auth `app.use('/public', logger)` registration in the SAME
// file (must NOT be captured — `logger` doesn't match the guard-name vocabulary).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ADAPTER = path.join(__dirname, '..', 'adapter.mjs');

// Every file entry goes through adapter-kit's EnvelopeBuilder, which always emits the full
// FileProjection shape (symbols/re_exports/dynamic_imports/used_names/const_map_fragment/
// procedure_router_fragments/router_mount_fragments/io/degraded/is_entry) at their zero values on top of
// whatever this adapter itself sets (`loc`, `attributes`) — mirrors svelte-adapter's own test helper.
function fileProjection({ path: p, loc, attributes }) {
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
    io: { provides: [], consumes: [] },
    degraded: false,
    is_entry: false,
    attributes,
  };
}

function makeFixture() {
  const root = mkdtempSync(path.join(tmpdir(), 'auth-overlay-adapter-test-'));
  mkdirSync(path.join(root, 'src'), { recursive: true });
  writeFileSync(
    path.join(root, 'src', 'app.ts'),
    ["app.use('/admin', requireAuth);", "app.use('/public', logger);", ''].join('\n')
  );
  return root;
}

test('auth-overlay-adapter: envelope matches committed snapshot', () => {
  const root = makeFixture();
  try {
    const stdout = execFileSync(process.execPath, [ADAPTER, '--root', root], { encoding: 'utf8' });
    const envelope = JSON.parse(stdout);
    assert.deepEqual(envelope, {
      format: 'zzop-normalized-ast',
      version: 1,
      parser: 'auth-overlay-adapter/1',
      source: 'web',
      files: [
        fileProjection({
          path: 'src/app.ts',
          loc: 3,
          attributes: [
            {
              target: { pathScope: { prefix: '/admin' } },
              key: 'auth-guarded',
              value: true,
            },
          ],
        }),
      ],
    });
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
