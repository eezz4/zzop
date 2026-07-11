import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { walk } from '../lib/walk.js';

function makeFixtureTree() {
  const root = mkdtempSync(path.join(os.tmpdir(), 'zzop-walk-'));
  mkdirSync(path.join(root, 'src', 'nested'), { recursive: true });
  mkdirSync(path.join(root, 'node_modules', 'dep'), { recursive: true });
  mkdirSync(path.join(root, '.git'), { recursive: true });
  writeFileSync(path.join(root, 'b.ts'), '');
  writeFileSync(path.join(root, 'a.ts'), '');
  writeFileSync(path.join(root, 'a.spec.ts'), '');
  writeFileSync(path.join(root, 'src', 'index.ts'), '');
  writeFileSync(path.join(root, 'src', 'nested', 'deep.ts'), '');
  writeFileSync(path.join(root, 'README.md'), '');
  writeFileSync(path.join(root, 'node_modules', 'dep', 'index.js'), '');
  writeFileSync(path.join(root, '.git', 'HEAD'), '');
  return root;
}

test('walk returns a deterministic, lexically sorted, forward-slash file list', () => {
  const root = makeFixtureTree();
  try {
    const result = walk(root);
    assert.deepEqual(result, [...result].sort());
    assert.ok(result.every((p) => !p.includes('\\')));
    const again = walk(root);
    assert.deepEqual(result, again);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('walk skips node_modules and .git by default', () => {
  const root = makeFixtureTree();
  try {
    const result = walk(root);
    assert.ok(!result.some((p) => p.startsWith('node_modules/')));
    assert.ok(!result.some((p) => p.startsWith('.git/')));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('walk descends nested directories and returns repo-relative paths', () => {
  const root = makeFixtureTree();
  try {
    const result = walk(root);
    assert.ok(result.includes('src/index.ts'));
    assert.ok(result.includes('src/nested/deep.ts'));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('walk include/exclude filter by extension (dot optional)', () => {
  const root = makeFixtureTree();
  try {
    const onlyTs = walk(root, { include: ['ts'] });
    assert.deepEqual(onlyTs, ['a.spec.ts', 'a.ts', 'b.ts', 'src/index.ts', 'src/nested/deep.ts']);

    const noMd = walk(root, { exclude: ['.md'] });
    assert.ok(!noMd.includes('README.md'));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('walk excludeFile skips files by name pattern (e.g. test files)', () => {
  const root = makeFixtureTree();
  try {
    const noSpec = walk(root, { include: ['ts'], excludeFile: /\.spec\.ts$/ });
    assert.ok(!noSpec.includes('a.spec.ts'));
    assert.ok(noSpec.includes('a.ts'));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('walk skipDirs accepts additional directory names on top of the defaults', () => {
  const root = makeFixtureTree();
  try {
    const withoutSrc = walk(root, { skipDirs: ['src'] });
    assert.ok(!withoutSrc.some((p) => p.startsWith('src/')));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
