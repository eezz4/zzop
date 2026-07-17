'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

const { expandAutoTrees } = require('../lib/workspaces');
const { configToRequest, ConfigError } = require('../lib/mapper');

// One scratch dir per run for the on-disk workspace fixtures; cleaned up at process exit so a crashed run
// needs no manual cleanup (mirrors mapper.test.js's overlay scratch dir).
const scratchRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'zzop-workspaces-test-'));
process.on('exit', () => {
  fs.rmSync(scratchRoot, { recursive: true, force: true });
});

let fixtureSeq = 0;
/**
 * Materialize a workspace fixture on disk. `layout` maps a relative file path to its string contents;
 * directories are created as needed. Returns the fixture's absolute base dir.
 */
function makeFixture(layout) {
  const base = path.join(scratchRoot, `ws-${fixtureSeq++}`);
  for (const [rel, contents] of Object.entries(layout)) {
    const abs = path.join(base, rel);
    fs.mkdirSync(path.dirname(abs), { recursive: true });
    fs.writeFileSync(abs, contents);
  }
  fs.mkdirSync(base, { recursive: true }); // ensure base exists even with an empty layout
  return base;
}

const pkg = (name) => JSON.stringify(name ? { name } : {});

// ---------------------------------------------------------------------------------------------------
// Pass-through: anything that is not exactly `trees: "auto"` is returned unchanged (no disk access).
// ---------------------------------------------------------------------------------------------------

test('expandAutoTrees is a no-op for an explicit trees array', () => {
  const config = { trees: [{ root: './a', sourceId: 'a' }] };
  const out = expandAutoTrees(config, scratchRoot);
  assert.equal(out.config, config); // same object reference — untouched
  assert.deepEqual(out.warnings, []);
});

test('expandAutoTrees is a no-op for roots / omitted trees', () => {
  for (const config of [{ roots: ['.'] }, {}, { roots: ['a', 'b'] }]) {
    const out = expandAutoTrees(config, scratchRoot);
    assert.equal(out.config, config);
    assert.deepEqual(out.warnings, []);
  }
});

test('expandAutoTrees tolerates non-object configs', () => {
  for (const bad of [null, undefined, 42, 'auto', ['auto']]) {
    const out = expandAutoTrees(bad, scratchRoot);
    assert.equal(out.config, bad);
    assert.deepEqual(out.warnings, []);
  }
});

// ---------------------------------------------------------------------------------------------------
// pnpm-workspace.yaml detection.
// ---------------------------------------------------------------------------------------------------

test('detects packages from a pnpm-workspace.yaml block list', () => {
  const base = makeFixture({
    'pnpm-workspace.yaml': "packages:\n  - 'packages/*'\n  - \"apps/*\"\n",
    'packages/api/package.json': pkg('@acme/api'),
    'packages/db/package.json': pkg('@acme/db'),
    'apps/web/package.json': pkg('web'),
    'apps/notes.txt': 'not a package', // file, not a dir — ignored
  });
  const { config, warnings } = expandAutoTrees({ trees: 'auto' }, base);
  assert.deepEqual(config.trees, [
    { root: 'apps/web', sourceId: 'web' },
    { root: 'packages/api', sourceId: '@acme/api' },
    { root: 'packages/db', sourceId: '@acme/db' },
  ]);
  assert.ok(warnings.some((w) => w.includes('expanded to 3 tree(s)')));
  assert.ok(warnings.some((w) => w.includes('pnpm-workspace.yaml')));
});

test('detects packages from a pnpm-workspace.yaml inline flow list', () => {
  const base = makeFixture({
    'pnpm-workspace.yaml': "packages: ['packages/*']\n",
    'packages/one/package.json': pkg('one'),
    'packages/two/package.json': pkg('two'),
  });
  const { config } = expandAutoTrees({ trees: 'auto' }, base);
  assert.deepEqual(config.trees, [
    { root: 'packages/one', sourceId: 'one' },
    { root: 'packages/two', sourceId: 'two' },
  ]);
});

test('pnpm-workspace.yaml comments and blank lines are ignored; a later top-level key ends the block', () => {
  const base = makeFixture({
    'pnpm-workspace.yaml':
      '# top comment\npackages:\n  - packages/* # inline comment\n\ncatalog:\n  foo: 1\n  - not-a-package\n',
    'packages/x/package.json': pkg('x'),
  });
  const { config } = expandAutoTrees({ trees: 'auto' }, base);
  assert.deepEqual(config.trees, [{ root: 'packages/x', sourceId: 'x' }]);
});

// ---------------------------------------------------------------------------------------------------
// package.json workspaces detection (used only when there is no pnpm-workspace.yaml).
// ---------------------------------------------------------------------------------------------------

test('detects packages from package.json "workspaces" array', () => {
  const base = makeFixture({
    'package.json': JSON.stringify({ name: 'root', workspaces: ['packages/*'] }),
    'packages/a/package.json': pkg('a'),
    'packages/b/package.json': pkg('b'),
  });
  const { config, warnings } = expandAutoTrees({ trees: 'auto' }, base);
  assert.deepEqual(config.trees, [
    { root: 'packages/a', sourceId: 'a' },
    { root: 'packages/b', sourceId: 'b' },
  ]);
  assert.ok(warnings.some((w) => w.includes('package.json "workspaces"')));
});

test('detects packages from package.json "workspaces" object form', () => {
  const base = makeFixture({
    'package.json': JSON.stringify({ workspaces: { packages: ['libs/*'] } }),
    'libs/core/package.json': pkg('core'),
  });
  const { config } = expandAutoTrees({ trees: 'auto' }, base);
  assert.deepEqual(config.trees, [{ root: 'libs/core', sourceId: 'core' }]);
});

test('pnpm-workspace.yaml takes precedence over package.json workspaces', () => {
  const base = makeFixture({
    'pnpm-workspace.yaml': 'packages:\n  - pnpm-pkgs/*\n',
    'package.json': JSON.stringify({ workspaces: ['npm-pkgs/*'] }),
    'pnpm-pkgs/p/package.json': pkg('p'),
    'npm-pkgs/n/package.json': pkg('n'),
  });
  const { config } = expandAutoTrees({ trees: 'auto' }, base);
  assert.deepEqual(config.trees, [{ root: 'pnpm-pkgs/p', sourceId: 'p' }]);
});

// ---------------------------------------------------------------------------------------------------
// Glob semantics: `**`, negation, nested; and only dirs WITH a package.json count.
// ---------------------------------------------------------------------------------------------------

test('`**` matches packages at any depth', () => {
  const base = makeFixture({
    'pnpm-workspace.yaml': 'packages:\n  - "packages/**"\n',
    'packages/a/package.json': pkg('a'),
    'packages/group/b/package.json': pkg('b'),
    'packages/group/deep/c/package.json': pkg('c'),
    'packages/group/no-pkg/readme.md': 'x', // no package.json -> excluded
  });
  const { config } = expandAutoTrees({ trees: 'auto' }, base);
  assert.deepEqual(config.trees.map((t) => t.root), [
    'packages/a',
    'packages/group/b',
    'packages/group/deep/c',
  ].sort());
});

test('`!` negation excludes matched packages', () => {
  const base = makeFixture({
    'pnpm-workspace.yaml': "packages:\n  - 'packages/*'\n  - '!packages/examples'\n",
    'packages/keep/package.json': pkg('keep'),
    'packages/examples/package.json': pkg('examples'),
  });
  const { config } = expandAutoTrees({ trees: 'auto' }, base);
  assert.deepEqual(config.trees, [{ root: 'packages/keep', sourceId: 'keep' }]);
});

test('node_modules is never scanned or returned', () => {
  const base = makeFixture({
    'pnpm-workspace.yaml': 'packages:\n  - "**"\n',
    'app/package.json': pkg('app'),
    'node_modules/dep/package.json': pkg('dep'),
  });
  const { config } = expandAutoTrees({ trees: 'auto' }, base);
  assert.deepEqual(config.trees, [{ root: 'app', sourceId: 'app' }]);
});

test('a nameless package falls back to its relative dir as sourceId', () => {
  const base = makeFixture({
    'pnpm-workspace.yaml': 'packages:\n  - "packages/*"\n',
    'packages/anon/package.json': pkg(null), // {} — no name
  });
  const { config } = expandAutoTrees({ trees: 'auto' }, base);
  assert.deepEqual(config.trees, [{ root: 'packages/anon', sourceId: 'packages/anon' }]);
});

test('duplicate sourceIds produce a warning but still expand', () => {
  const base = makeFixture({
    'pnpm-workspace.yaml': 'packages:\n  - "a/*"\n  - "b/*"\n',
    'a/dup/package.json': pkg('same'),
    'b/dup/package.json': pkg('same'),
  });
  const { config, warnings } = expandAutoTrees({ trees: 'auto' }, base);
  assert.equal(config.trees.length, 2);
  assert.ok(warnings.some((w) => w.includes('duplicate sourceId "same"')));
});

test('a single detected package warns that the join needs >= 2 trees', () => {
  const base = makeFixture({
    'pnpm-workspace.yaml': 'packages:\n  - "packages/*"\n',
    'packages/only/package.json': pkg('only'),
  });
  const { config, warnings } = expandAutoTrees({ trees: 'auto' }, base);
  assert.deepEqual(config.trees, [{ root: 'packages/only', sourceId: 'only' }]);
  assert.ok(warnings.some((w) => w.includes('only one workspace package')));
});

test('other config keys survive expansion', () => {
  const base = makeFixture({
    'pnpm-workspace.yaml': 'packages:\n  - "p/*"\n',
    'p/a/package.json': pkg('a'),
    'p/b/package.json': pkg('b'),
  });
  const { config } = expandAutoTrees(
    { trees: 'auto', failOn: 'critical', overlays: ['./x.json'], rules: { 'sql/nplus1': 'off' } },
    base
  );
  assert.equal(config.failOn, 'critical');
  assert.deepEqual(config.overlays, ['./x.json']);
  assert.deepEqual(config.rules, { 'sql/nplus1': 'off' });
  assert.equal(config.trees.length, 2);
});

test('roots alongside trees:"auto" warns and is stripped (parity with the Rust config port)', () => {
  const base = makeFixture({
    'pnpm-workspace.yaml': 'packages:\n  - "p/*"\n',
    'p/a/package.json': pkg('a'),
    'p/b/package.json': pkg('b'),
  });
  const { config, warnings } = expandAutoTrees({ trees: 'auto', roots: ['./x'] }, base);
  assert.ok(
    warnings.some((w) => /config has both "roots" and "trees": "auto"/.test(w) && /ignored in auto mode/.test(w)),
    `expected the shadowed-roots warning, got: ${warnings.join(' | ')}`
  );
  assert.ok(!('roots' in config), 'inert roots key must be stripped from the expanded config');

  const clean = expandAutoTrees({ trees: 'auto' }, base);
  assert.ok(!clean.warnings.some((w) => /has both "roots"/.test(w)));
});

// ---------------------------------------------------------------------------------------------------
// Error paths: no manifest, or a manifest that yields no packages.
// ---------------------------------------------------------------------------------------------------

test('no workspace manifest throws an actionable ConfigError', () => {
  const base = makeFixture({ 'src/index.ts': 'export const x = 1;' });
  assert.throws(() => expandAutoTrees({ trees: 'auto' }, base), (err) => {
    assert.ok(err instanceof ConfigError);
    assert.match(err.message, /found no workspace manifest/);
    return true;
  });
});

test('a manifest matching zero package dirs throws a ConfigError naming the patterns', () => {
  const base = makeFixture({
    'pnpm-workspace.yaml': 'packages:\n  - "packages/*"\n',
    'packages/not-a-pkg/readme.md': 'x', // dir exists but has no package.json
  });
  assert.throws(() => expandAutoTrees({ trees: 'auto' }, base), (err) => {
    assert.ok(err instanceof ConfigError);
    assert.match(err.message, /matched no package directories/);
    assert.match(err.message, /packages\/\*/);
    return true;
  });
});

// ---------------------------------------------------------------------------------------------------
// End-to-end into the mapper, plus the direct-embedder guard.
// ---------------------------------------------------------------------------------------------------

test('expanded config maps to an analyzeTrees request', () => {
  const base = makeFixture({
    'pnpm-workspace.yaml': 'packages:\n  - "packages/*"\n',
    'packages/api/package.json': pkg('api'),
    'packages/web/package.json': pkg('web'),
  });
  const { config } = expandAutoTrees({ trees: 'auto' }, base);
  const { method, request } = configToRequest(config);
  assert.equal(method, 'analyzeTrees');
  assert.deepEqual(request.trees.map((t) => ({ root: t.root, sourceId: t.sourceId })), [
    { root: 'packages/api', sourceId: 'api' },
    { root: 'packages/web', sourceId: 'web' },
  ]);
});

test('configToRequest rejects an unexpanded trees: "auto" with a pointer to expandAutoTrees', () => {
  assert.throws(() => configToRequest({ trees: 'auto' }), (err) => {
    assert.ok(err instanceof ConfigError);
    assert.match(err.message, /expandAutoTrees/);
    return true;
  });
});
