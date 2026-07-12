// Parity contract: replays every row of docs/adapters/key-normalization.fixture.json (generated from
// the real zzop_core::http_interface_key / http_consume_interface_key Rust functions) against this
// kit's JS port. If a row here fails, the fix belongs in lib/keys.js — never in the fixture.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import {
  normalizeProvideKey,
  normalizeConsumeKey,
  resolveConsumeKey,
  isExternalUrl,
  baseRelativePath,
} from '../lib/keys.js';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const FIXTURE_PATH = path.join(__dirname, '..', '..', '..', 'docs', 'adapters', 'key-normalization.fixture.json');

const rows = JSON.parse(readFileSync(FIXTURE_PATH, 'utf8'));

test('key-normalization fixture is non-empty', () => {
  assert.ok(rows.length > 0, 'fixture must not be empty');
});

test('key-normalization fixture parity', async (t) => {
  for (const [i, row] of rows.entries()) {
    await t.test(`row ${i}: ${row.side} ${row.method} ${JSON.stringify(row.path)} -> ${row.key}`, () => {
      const actual =
        row.side === 'provide' ? normalizeProvideKey(row.method, row.path) : normalizeConsumeKey(row.method, row.path);
      assert.equal(actual, row.key);
    });
  }
});

test('resolveConsumeKey: internal (leading slash) keys directly', () => {
  assert.equal(resolveConsumeKey('get', '/users/{id}'), 'GET /users/{}');
});

test('resolveConsumeKey: external (http/https) keys verbatim, never normalized', () => {
  assert.equal(resolveConsumeKey('get', 'https://api.example.com/v1/users?x=1'), 'GET https://api.example.com/v1/users?x=1');
  assert.equal(isExternalUrl('https://api.example.com/x'), true);
  assert.equal(isExternalUrl('ws://api.example.com/x'), false);
});

test('resolveConsumeKey: base-relative resolves via baseRelativePath then normalizes', () => {
  assert.equal(resolveConsumeKey('get', 'users/login'), 'GET /users/login');
  assert.equal(baseRelativePath('users/login'), '/users/login');
});

test('resolveConsumeKey: base-carrier head-drop keys the visible path, parity with native', () => {
  // Mirrors `consume_key_for`'s 4th bucket (base-carrier-drop-v1): a single opaque `{}` head
  // followed by a `/`-headed literal keys as the visible path — the base is dropped, never valued.
  assert.equal(resolveConsumeKey('get', '{}/me/achievements'), 'GET /me/achievements');
  assert.equal(resolveConsumeKey('get', '{}/articles?limit=10'), 'GET /articles');
  // Refusals stay null: dynamic-dynamic head, non-`/` suffix, protocol-relative host carrier.
  for (const literal of ['{}{}', '{}users', '{}//example.com/x']) {
    assert.equal(resolveConsumeKey('get', literal), null, `expected null for ${JSON.stringify(literal)}`);
  }
});

test('resolveConsumeKey: vetoed literals resolve to null, never guessed', () => {
  for (const literal of ['', './relative', '{base}/x', '?page=2', 'has space/x']) {
    assert.equal(resolveConsumeKey('get', literal), null, `expected null for ${JSON.stringify(literal)}`);
    assert.equal(baseRelativePath(literal), null, `expected null for ${JSON.stringify(literal)}`);
  }
});
