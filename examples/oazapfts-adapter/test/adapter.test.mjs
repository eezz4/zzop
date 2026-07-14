// Ports of the 8 native `oazapfts-generated-SDK call family` test cases from
// `parser/parser-typescript/src/adapters/egress.rs` (before that recognizer's removal — oazapfts
// recognition moved here, to an injected Mode B adapter, per zzop's "generated SDKs = injection
// adapters" decision). Each test runs `adapter.mjs` as a subprocess against a minimal one-line fixture
// file (the exact call-site text from the corresponding Rust `#[test]`) and asserts the emitted
// `IoConsume`(s), the same way the native test asserted `extract_http_egress`'s output.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ADAPTER = path.join(__dirname, '..', 'adapter.mjs');

/** Writes `line` (a single source line, same call-site text the ported native `#[test]` used) to a
 * fresh temp tree at `src/api.ts`, runs the adapter against that tree, and returns the parsed envelope.
 * Cleans up the temp tree before returning. */
function runAdapter(line) {
  const root = mkdtempSync(path.join(tmpdir(), 'oazapfts-adapter-test-'));
  try {
    mkdirSync(path.join(root, 'src'), { recursive: true });
    writeFileSync(path.join(root, 'src', 'api.ts'), `${line}\n`);
    const stdout = execFileSync(process.execPath, [ADAPTER, '--root', root, '--source', 'web'], {
      encoding: 'utf8',
    });
    return JSON.parse(stdout);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

function consume(key) {
  return { kind: 'http', key, file: 'src/api.ts', line: 1, client: 'oazapfts' };
}

test('oazapfts-adapter: GET with trailing ${QS...} suffix drops the interpolation from the key', () => {
  const envelope = runAdapter(
    'oazapfts.ok(oazapfts.fetchJson<{ status: 200 }>(`/activities${QS.query(QS.explode({ albumId }))}`, { ...opts }));'
  );
  assert.equal(envelope.files.length, 1);
  assert.deepEqual(envelope.files[0].io.consumes, [consume('GET /activities')]);
});

test('oazapfts-adapter: POST read from an oazapfts.json({ method }) wrapper', () => {
  const envelope = runAdapter(
    'oazapfts.ok(oazapfts.fetchJson<{ status: 201 }>("/activities", oazapfts.json({ ...opts, method: "POST", body: activityCreateDto })));'
  );
  assert.equal(envelope.files.length, 1);
  assert.deepEqual(envelope.files[0].io.consumes, [consume('POST /activities')]);
});

test('oazapfts-adapter: POST read from an oazapfts.multipart({ method }) wrapper', () => {
  const envelope = runAdapter(
    'oazapfts.fetchJson("/admin/backups/upload", oazapfts.multipart({ ...opts, method: "POST", body }))'
  );
  assert.equal(envelope.files.length, 1);
  assert.deepEqual(envelope.files[0].io.consumes, [consume('POST /admin/backups/upload')]);
});

test('oazapfts-adapter: call nested inside oazapfts.ok(...) is still detected', () => {
  const envelope = runAdapter('return oazapfts.ok(oazapfts.fetchJson("/activities", { ...opts }));');
  assert.equal(envelope.files.length, 1);
  assert.deepEqual(envelope.files[0].io.consumes, [consume('GET /activities')]);
});

test('oazapfts-adapter: fetchBlob with a path-param template keeps the {} placeholder', () => {
  const envelope = runAdapter('oazapfts.ok(oazapfts.fetchBlob(`/assets/${id}/thumbnail`, { ...opts }));');
  assert.equal(envelope.files.length, 1);
  assert.deepEqual(envelope.files[0].io.consumes, [consume('GET /assets/{}/thumbnail')]);
});

test('oazapfts-adapter: a mid-path QS. interpolation (not trailing) stays a {} placeholder', () => {
  const envelope = runAdapter('oazapfts.ok(oazapfts.fetchJson(`/foo/${QS.query(x)}/bar`, { ...opts }));');
  assert.equal(envelope.files.length, 1);
  assert.deepEqual(envelope.files[0].io.consumes, [consume('GET /foo/{}/bar')]);
});

test('oazapfts-adapter: a non-oazapfts receiver with the same method name is not recognized', () => {
  const envelope = runAdapter('other.fetchJson("/activities", { ...opts });');
  assert.equal(envelope.files.length, 0);
});

test('oazapfts-adapter: a sibling body prop does not block the consume (body shape is a documented limitation)', () => {
  // The removed native recognizer additionally witnessed this call's `body: { albumId }` shape
  // (`oazapfts_sibling_body_prop_is_witnessed` in egress.rs). This adapter does not capture body shape
  // (see adapter.mjs's LIMITATIONS note: adapter-kit's EnvelopeBuilder.addConsume has no `body` option)
  // — only asserts that the `method:` read still works and the consume is still emitted, with no `body`
  // field on the result.
  const envelope = runAdapter(
    'oazapfts.fetchJson("/activities", oazapfts.json({ ...opts, method: "POST", body: { albumId } }));'
  );
  assert.equal(envelope.files.length, 1);
  assert.deepEqual(envelope.files[0].io.consumes, [consume('POST /activities')]);
});
