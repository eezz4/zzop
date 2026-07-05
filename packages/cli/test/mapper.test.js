'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

const { configToRequest, normalizeSeverity, severityRank, ConfigError, OFF } = require('../lib/mapper');

test('normalizeSeverity maps friendly names to engine severities', () => {
  assert.equal(normalizeSeverity('warn'), 'warning');
  assert.equal(normalizeSeverity('warning'), 'warning');
  assert.equal(normalizeSeverity('error'), 'critical');
  assert.equal(normalizeSeverity('critical'), 'critical');
  assert.equal(normalizeSeverity('info'), 'info');
  assert.equal(normalizeSeverity('INFO'), 'info');
  assert.equal(normalizeSeverity('  Warn '), 'warning');
});

test('normalizeSeverity returns OFF sentinel for off-aliases', () => {
  assert.equal(normalizeSeverity('off'), OFF);
  assert.equal(normalizeSeverity('none'), OFF);
  assert.equal(normalizeSeverity('disabled'), OFF);
});

test('normalizeSeverity throws on unknown / non-string', () => {
  assert.throws(() => normalizeSeverity('bogus'), ConfigError);
  assert.throws(() => normalizeSeverity(3), ConfigError);
});

test('severityRank orders info < warning < critical', () => {
  assert.ok(severityRank('info') < severityRank('warning'));
  assert.ok(severityRank('warning') < severityRank('critical'));
  assert.equal(severityRank('nonsense'), 0);
});

test('single root -> analyze with just root', () => {
  const { method, request } = configToRequest({ roots: ['.'] });
  assert.equal(method, 'analyze');
  assert.deepEqual(request, { root: '.' });
});

test('default (no roots) -> analyze on "."', () => {
  const { method, request } = configToRequest({});
  assert.equal(method, 'analyze');
  assert.deepEqual(request, { root: '.' });
});

test('multiple roots -> analyzeTrees, each tree gets a distinct sourceId (its root)', () => {
  // Distinct sourceIds are required or cross-source rules (shared-db-table, cross-tree shadowing,
  // ...) never fire — they gate on >= 2 distinct sources.
  const { method, request } = configToRequest({ roots: ['./a', './b'] });
  assert.equal(method, 'analyzeTrees');
  assert.deepEqual(request, {
    trees: [
      { root: './a', sourceId: './a' },
      { root: './b', sourceId: './b' },
    ],
  });
});

test('explicit trees -> analyzeTrees, sourceId preserved, even for one tree', () => {
  const { method, request } = configToRequest({ trees: [{ root: './api', sourceId: 'api' }] });
  assert.equal(method, 'analyzeTrees');
  assert.deepEqual(request, { trees: [{ root: './api', sourceId: 'api' }] });
});

test('explicit trees without sourceId -> defaults sourceId to the tree root', () => {
  const { request } = configToRequest({ trees: [{ root: './api' }, { root: './web' }] });
  assert.deepEqual(request, {
    trees: [
      { root: './api', sourceId: './api' },
      { root: './web', sourceId: './web' },
    ],
  });
});

test('packs.extraDirs -> packsDir (user dirs only), empty omitted', () => {
  const a = configToRequest({ roots: ['.'], packs: { extraDirs: ['./zzop-packs'] } });
  assert.deepEqual(a.request.packsDir, ['./zzop-packs']);

  const b = configToRequest({ roots: ['.'], packs: { extraDirs: [] } });
  assert.ok(!('packsDir' in b.request));

  const c = configToRequest({ roots: ['.'] });
  assert.ok(!('packsDir' in c.request));
});

test('packs.disabled + rule "off" -> disabledRules (deduped)', () => {
  const { request } = configToRequest({
    roots: ['.'],
    packs: { disabled: ['browser'] },
    rules: { 'no-explicit-any': 'off', 'browser': 'off' },
  });
  assert.deepEqual(new Set(request.disabledRules), new Set(['browser', 'no-explicit-any']));
});

test('rule severity string -> severityOverrides', () => {
  const { request } = configToRequest({ roots: ['.'], rules: { 'n-plus-one': 'warn' } });
  assert.deepEqual(request.severityOverrides, { 'n-plus-one': 'warning' });
});

test('rule object severity + exclude -> severityOverrides + suppressions', () => {
  const { request } = configToRequest({
    roots: ['.'],
    rules: { toctou: { severity: 'warn', exclude: ['legacy/', 'vendor/'] } },
  });
  assert.deepEqual(request.severityOverrides, { toctou: 'warning' });
  assert.deepEqual(request.suppressions, [
    { rule: 'toctou', path: 'legacy/' },
    { rule: 'toctou', path: 'vendor/' },
  ]);
});

test('rule object severity "off" -> disabledRules, not severityOverrides', () => {
  const { request } = configToRequest({ roots: ['.'], rules: { toctou: { severity: 'off' } } });
  assert.deepEqual(request.disabledRules, ['toctou']);
  assert.ok(!('severityOverrides' in request));
});

test('pass-through git/cacheDir/sizeCap', () => {
  const { request } = configToRequest({
    roots: ['.'],
    git: { recentDays: 30 },
    cacheDir: '.zzop-cache',
    sizeCap: 500000,
  });
  assert.deepEqual(request.git, { recentDays: 30 });
  assert.equal(request.cacheDir, '.zzop-cache');
  assert.equal(request.sizeCap, 500000);
});

test('shared options replicate across every tree', () => {
  const { request } = configToRequest({
    roots: ['./a', './b'],
    rules: { 'n-plus-one': 'warn' },
    cacheDir: '.zzop-cache',
  });
  for (const tree of request.trees) {
    assert.deepEqual(tree.severityOverrides, { 'n-plus-one': 'warning' });
    assert.equal(tree.cacheDir, '.zzop-cache');
  }
});

test('full design-doc example maps as documented', () => {
  const { method, request } = configToRequest({
    roots: ['.'],
    packs: { extraDirs: ['./zzop-packs'], disabled: ['browser'] },
    rules: {
      'no-explicit-any': 'off',
      'n-plus-one': 'warn',
      toctou: { severity: 'warn', exclude: ['legacy/'] },
    },
    git: { recentDays: 30 },
    cacheDir: '.zzop-cache',
    sizeCap: 500000,
  });
  assert.equal(method, 'analyze');
  assert.deepEqual(request, {
    root: '.',
    packsDir: ['./zzop-packs'],
    disabledRules: ['browser', 'no-explicit-any'],
    severityOverrides: { 'n-plus-one': 'warning', toctou: 'warning' },
    suppressions: [{ rule: 'toctou', path: 'legacy/' }],
    git: { recentDays: 30 },
    cacheDir: '.zzop-cache',
    sizeCap: 500000,
  });
});

test('invalid shapes throw ConfigError', () => {
  assert.throws(() => configToRequest(null), ConfigError);
  assert.throws(() => configToRequest({ rules: [] }), ConfigError);
  assert.throws(() => configToRequest({ roots: [] }), ConfigError);
  assert.throws(() => configToRequest({ roots: [''] }), ConfigError);
  assert.throws(() => configToRequest({ trees: [] }), ConfigError);
  assert.throws(() => configToRequest({ trees: [{ sourceId: 'x' }] }), ConfigError);
  assert.throws(
    () => configToRequest({ rules: { toctou: { exclude: 'legacy/' } } }),
    ConfigError
  );
  assert.throws(() => configToRequest({ packs: { extraDirs: 'x' } }), ConfigError);
});
