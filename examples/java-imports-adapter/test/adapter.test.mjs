// Snapshot test: runs adapter.mjs as a subprocess against the COMMITTED fixture tree
// (test/fixture/, a 3-file Java mini-app) and deep-equals the parsed envelope JSON against the
// committed test/expected-envelope.json. That same expected-envelope.json is also consumed by the
// engine-side test (crates/engine/tests/analyze_java_imports_overlay.rs), which validates it with
// the REAL `zzop_core::validate_envelope` and proves the overlay yields dep-graph import edges on a
// lexically-parsed Java tree — the two tests pin the same bytes from both sides of the contract.
//
// Fixture exercises: a plain intra-tree import, a static member import (edge to the owning class),
// a JDK import (skipped), a package wildcard import (skipped), and a leaf file with no intra-tree
// imports of its own (Config.java — must not be projected at all).
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

test('java-imports-adapter: envelope matches the committed snapshot', () => {
  const stdout = execFileSync(process.execPath, [ADAPTER, '--root', FIXTURE], { encoding: 'utf8' });
  assert.deepEqual(JSON.parse(stdout), EXPECTED);
});

test('java-imports-adapter: leaf file with no intra-tree imports is not projected', () => {
  // Config.java is imported BY TextUtil.java but imports nothing intra-tree itself — the one-channel
  // adapter has no fact to project for it, so it must not appear in files[] (see README "Limits").
  const paths = EXPECTED.files.map((f) => f.path);
  assert.ok(!paths.some((p) => p.endsWith('Config.java')), `unexpected: ${paths}`);
  assert.deepEqual(paths, [
    'src/main/java/com/example/app/App.java',
    'src/main/java/com/example/util/TextUtil.java',
  ]);
});
