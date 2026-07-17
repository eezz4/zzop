'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

const {
  configToRequest,
  collectConfigWarnings,
  normalizeSeverity,
  severityRank,
  ConfigError,
  OFF,
} = require('../lib/mapper');

// Isolated scratch dir for the overlay-file tests below (the only mapper tests that touch disk — overlay
// loading is this module's one deliberate I/O exception, see mapper.js's own module doc). One dir per test
// run, cleaned up at the end via a process-exit hook so a crashed run doesn't need manual cleanup either.
const overlayScratchDir = fs.mkdtempSync(path.join(os.tmpdir(), 'zzop-mapper-overlay-test-'));
process.on('exit', () => {
  fs.rmSync(overlayScratchDir, { recursive: true, force: true });
});

const VALID_OVERLAY = {
  format: 'zzop-normalized-ast',
  version: 1,
  parser: 'test-adapter/1',
  source: 'legacy',
  files: [
    {
      path: 'a.ts',
      loc: 10,
      io: { provides: [{ kind: 'http', key: 'GET /foo', file: 'a.ts', line: 1 }], consumes: [] },
    },
  ],
};
fs.writeFileSync(path.join(overlayScratchDir, 'valid.json'), JSON.stringify(VALID_OVERLAY));
fs.writeFileSync(path.join(overlayScratchDir, 'not-json.json'), '{ this is not json');

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

test('exclude routes glob patterns to `glob` and plain fragments to `path`', () => {
  const { request } = configToRequest({
    roots: ['.'],
    rules: {
      'dead-candidates': {
        exclude: ['legacy/', '**/app/**/{page,layout}.tsx', 'src/app/[locale]/'],
      },
    },
  });
  assert.deepEqual(request.suppressions, [
    // plain fragment -> substring path
    { rule: 'dead-candidates', path: 'legacy/' },
    // glob metachars -> full-path glob
    { rule: 'dead-candidates', glob: '**/app/**/{page,layout}.tsx' },
    // `[...]` is NOT a glob metachar here, so a raw dynamic-segment path stays a substring
    { rule: 'dead-candidates', path: 'src/app/[locale]/' },
  ]);
});

test('top-level exclude -> globalExcludes with the same glob/path split', () => {
  const { request } = configToRequest({
    roots: ['.'],
    exclude: ['**/*.stories.tsx', 'legacy/'],
  });
  assert.deepEqual(request.globalExcludes, [
    { glob: '**/*.stories.tsx' },
    { path: 'legacy/' },
  ]);
});

test('top-level exclude rejects non-array / non-string entries', () => {
  assert.throws(() => configToRequest({ roots: ['.'], exclude: 'legacy/' }), ConfigError);
  assert.throws(() => configToRequest({ roots: ['.'], exclude: [123] }), ConfigError);
});

test('collectConfigWarnings warns when an explicit trees array shadows roots (parity with Rust port)', () => {
  const { collectConfigWarnings } = require('../lib/mapper');
  const warnings = collectConfigWarnings({ roots: ['./a'], trees: [{ root: './b' }] });
  assert.ok(
    warnings.some((w) => /config has both "roots" and "trees"/.test(w) && /"trees" wins/.test(w)),
    `expected the shadowed-roots warning, got: ${warnings.join(' | ')}`
  );
  // Only one of the two keys -> silent. trees:"auto" is expandAutoTrees' warning, not this one.
  assert.ok(!collectConfigWarnings({ roots: ['./a'] }).some((w) => /has both "roots"/.test(w)));
  assert.ok(!collectConfigWarnings({ trees: [{ root: './b' }] }).some((w) => /has both "roots"/.test(w)));
  assert.ok(!collectConfigWarnings({ roots: ['./a'], trees: 'auto' }).some((w) => /"trees" wins/.test(w)));
});

test('collectConfigWarnings flags unknown keys (never rejects) but stays silent on known ones', () => {
  const { collectConfigWarnings } = require('../lib/mapper');
  // all-known config -> no warnings
  assert.deepEqual(
    collectConfigWarnings({
      roots: ['.'],
      packs: { extraDirs: [], disabled: [] },
      rules: { toctou: { severity: 'warn', exclude: [] } },
      git: { recentDays: 30 },
      report: { dir: 'r', formats: ['json'] },
      format: 'pretty',
      failOn: 'warn',
    }),
    []
  );
  // unknown keys at each scope -> one warning each
  const w = collectConfigWarnings({
    rulez: {}, // top-level typo
    packs: { extraDirz: [] }, // nested typo
    git: { recentDays: 30, foo: 1 }, // nested typo alongside a known key
    trees: [{ root: '.', srcId: 'x' }], // tree typo
    rules: { toctou: { severty: 'warn' } }, // rule-object typo
  });
  assert.ok(w.some((x) => /"rulez"/.test(x)));
  assert.ok(w.some((x) => /"packs\.extraDirz"/.test(x)));
  assert.ok(w.some((x) => /"git\.foo"/.test(x)));
  assert.ok(w.some((x) => /"trees\[0\]\.srcId"/.test(x)));
  assert.ok(w.some((x) => /"rules\.toctou\.severty"/.test(x)));
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

test('git.commitTypePatterns passes through verbatim (no mapper transform) and is a known key', () => {
  const commitTypePatterns = [
    { pattern: '^\\s*corrige\\b', tag: 'FIX' },
    { pattern: '^\\s*nouveau\\b', tag: 'FEAT' },
  ];
  const { request } = configToRequest({ roots: ['.'], git: { commitTypePatterns } });
  assert.deepEqual(request.git, { commitTypePatterns });

  const { collectConfigWarnings } = require('../lib/mapper');
  assert.deepEqual(collectConfigWarnings({ git: { commitTypePatterns } }), []);
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

test('packs must be an object: a truthy non-object packs fails loudly, like rules', () => {
  for (const v of [[], 'x', 5, true]) {
    assert.throws(
      () => configToRequest({ packs: v }),
      (err) => {
        assert.ok(err instanceof ConfigError);
        assert.equal(
          err.message,
          'packs must be an object ({ "extraDirs": [...], "disabled": [...] }).'
        );
        return true;
      },
      `expected a ConfigError for packs: ${JSON.stringify(v)}`
    );
  }
});

test('falsy packs values are treated as absent, not an error (mirrors rules)', () => {
  for (const v of [null, false, 0, '']) {
    const { request } = configToRequest({ roots: ['.'], packs: v });
    assert.ok(!('packsDir' in request));
    assert.ok(!('disabledRules' in request));
  }
});

test('top-level overlays -> adapterOverlays on the single-tree request, resolved relative to the root', () => {
  const { method, request } = configToRequest({
    roots: [overlayScratchDir],
    overlays: ['valid.json'],
  });
  assert.equal(method, 'analyze');
  assert.deepEqual(request.adapterOverlays, [VALID_OVERLAY]);
});

test('trees[i].overlays -> adapterOverlays on that tree only; top-level overlays apply to every tree', () => {
  const { method, request } = configToRequest({
    trees: [
      { root: overlayScratchDir, sourceId: 'with-overlay', overlays: ['valid.json'] },
      { root: overlayScratchDir, sourceId: 'without-overlay' },
    ],
  });
  assert.equal(method, 'analyzeTrees');
  assert.deepEqual(request.trees[0].adapterOverlays, [VALID_OVERLAY]);
  assert.ok(!('adapterOverlays' in request.trees[1]));
});

test('top-level overlays broadcast to every tree when trees[] is used, resolved per-tree', () => {
  const { request } = configToRequest({
    trees: [{ root: overlayScratchDir, sourceId: 'a' }],
    overlays: ['valid.json'],
  });
  assert.deepEqual(request.trees[0].adapterOverlays, [VALID_OVERLAY]);
});

test('overlays: not an array, or non-string entries -> ConfigError (shape, not a read failure)', () => {
  assert.throws(() => configToRequest({ roots: ['.'], overlays: 'valid.json' }), ConfigError);
  assert.throws(() => configToRequest({ roots: ['.'], overlays: [123] }), ConfigError);
  assert.throws(
    () => configToRequest({ trees: [{ root: '.', overlays: 'x' }] }),
    ConfigError
  );
});

test('missing/unparseable overlay file: dropped from the request, never aborts', () => {
  const { request } = configToRequest({
    roots: [overlayScratchDir],
    overlays: ['valid.json', 'does-not-exist.json', 'not-json.json'],
  });
  // Only the valid overlay survives; the other two are silently skipped (see collectConfigWarnings test
  // below for where their diagnostics surface).
  assert.deepEqual(request.adapterOverlays, [VALID_OVERLAY]);
});

test('overlays that are all missing/unparseable -> adapterOverlays omitted entirely (no empty array)', () => {
  const { request } = configToRequest({
    roots: [overlayScratchDir],
    overlays: ['does-not-exist.json', 'not-json.json'],
  });
  assert.ok(!('adapterOverlays' in request));
});

test('collectConfigWarnings reports a missing overlay file, naming the path', () => {
  const warnings = collectConfigWarnings({ roots: [overlayScratchDir], overlays: ['does-not-exist.json'] });
  assert.ok(
    warnings.some((w) => /does-not-exist\.json/.test(w) && /could not be read/.test(w)),
    `expected a "could not be read" warning naming the path, got: ${JSON.stringify(warnings)}`
  );
});

test('collectConfigWarnings reports an unparseable overlay file, naming the path', () => {
  const warnings = collectConfigWarnings({ roots: [overlayScratchDir], overlays: ['not-json.json'] });
  assert.ok(
    warnings.some((w) => /not-json\.json/.test(w) && /not valid JSON/.test(w)),
    `expected a "not valid JSON" warning naming the path, got: ${JSON.stringify(warnings)}`
  );
});

test('collectConfigWarnings stays silent when every overlay loads cleanly', () => {
  const warnings = collectConfigWarnings({ roots: [overlayScratchDir], overlays: ['valid.json'] });
  assert.deepEqual(warnings, []);
});

test('collectConfigWarnings covers trees[i].overlays too, naming the tree', () => {
  const warnings = collectConfigWarnings({
    trees: [{ root: overlayScratchDir, sourceId: 'a', overlays: ['does-not-exist.json'] }],
  });
  assert.ok(warnings.some((w) => /does-not-exist\.json/.test(w)));
});

// -------------------------------------------------------------------------------------------------
// Connection topology: trees[i].mountedAt / trees[i].mounts / trees[i].hosts.
// -------------------------------------------------------------------------------------------------

test('trees[i].mountedAt/mounts/hosts map through to the tree request', () => {
  const { method, request } = configToRequest({
    trees: [
      {
        root: './api',
        sourceId: 'api',
        mountedAt: '/gateway',
        mounts: [{ dir: 'apps/admin', at: '/admin' }],
        hosts: ['internal.example.com'],
      },
    ],
  });
  assert.equal(method, 'analyzeTrees');
  assert.equal(request.trees[0].mountedAt, '/gateway');
  assert.deepEqual(request.trees[0].mounts, [{ dir: 'apps/admin', at: '/admin' }]);
  assert.deepEqual(request.trees[0].hosts, ['internal.example.com']);
});

test('multi-tree topology: each tree carries its own mountedAt/mounts/hosts independently', () => {
  const { request } = configToRequest({
    trees: [
      { root: './api', sourceId: 'api', mountedAt: '/api' },
      { root: './web', sourceId: 'web', hosts: ['web.internal'] },
    ],
  });
  assert.equal(request.trees[0].mountedAt, '/api');
  assert.ok(!('hosts' in request.trees[0]));
  assert.ok(!('mountedAt' in request.trees[1]));
  assert.deepEqual(request.trees[1].hosts, ['web.internal']);
});

test('empty mounts/hosts arrays are omitted from the request, mirroring other array knobs', () => {
  const { request } = configToRequest({
    trees: [{ root: './api', sourceId: 'api', mounts: [], hosts: [] }],
  });
  assert.ok(!('mounts' in request.trees[0]));
  assert.ok(!('hosts' in request.trees[0]));
});

test('roots-shorthand trees never carry mountedAt/mounts/hosts (only explicit trees[] accepts them)', () => {
  const { request } = configToRequest({ roots: ['.'] });
  assert.ok(!('mountedAt' in request));
  assert.ok(!('mounts' in request));
  assert.ok(!('hosts' in request));
});

test('trees[i].mountedAt validation: must be a string starting with "/", non-empty after trimming slashes, no scheme/placeholder/whitespace', () => {
  const withMountedAt = (v) => configToRequest({ trees: [{ root: '.', sourceId: 't', mountedAt: v }] });
  assert.throws(() => withMountedAt(123), ConfigError);
  assert.throws(() => withMountedAt('///'), ConfigError);
  assert.throws(() => withMountedAt('gateway'), ConfigError); // missing leading slash
  assert.throws(() => withMountedAt('https://gateway'), ConfigError);
  assert.throws(() => withMountedAt('/api/{}'), ConfigError);
  assert.throws(() => withMountedAt('/api foo'), ConfigError);
  assert.equal(withMountedAt('/gateway/').request.trees[0].mountedAt, '/gateway/');
});

test('trees[i].mounts shape/content validation', () => {
  const withMounts = (m) => configToRequest({ trees: [{ root: '.', sourceId: 't', mounts: m }] });
  assert.throws(() => withMounts('not-an-array'), ConfigError);
  assert.throws(() => withMounts(['not-an-object']), ConfigError);
  assert.throws(() => withMounts([{ dir: 1, at: '/x' }]), ConfigError);
  assert.throws(() => withMounts([{ dir: 'apps', at: 1 }]), ConfigError);
  assert.throws(() => withMounts([{ dir: '/apps', at: '/x' }]), ConfigError); // dir starts with "/"
  assert.throws(() => withMounts([{ dir: 'apps\\api', at: '/x' }]), ConfigError); // dir has a backslash
  assert.throws(() => withMounts([{ dir: 'apps', at: 'x' }]), ConfigError); // at missing leading "/"
  assert.throws(() => withMounts([{ dir: 'apps', at: '' }]), ConfigError); // at empty after trimming
  assert.throws(() => withMounts([{ dir: 'apps', at: 'https://x' }]), ConfigError); // at has a scheme
  assert.throws(() => withMounts([{ dir: 'apps', at: '/x/{}' }]), ConfigError); // at has a placeholder
  assert.throws(() => withMounts([{ dir: 'apps', at: '/x y' }]), ConfigError); // at has whitespace
  assert.deepEqual(
    withMounts([{ dir: 'apps/api', at: '/api' }]).request.trees[0].mounts,
    [{ dir: 'apps/api', at: '/api' }]
  );
});

test('trees[i].hosts shape/content validation', () => {
  const withHosts = (h) => configToRequest({ trees: [{ root: '.', sourceId: 't', hosts: h }] });
  assert.throws(() => withHosts('not-an-array'), ConfigError);
  assert.throws(() => withHosts(['']), ConfigError);
  assert.throws(() => withHosts([123]), ConfigError);
  assert.throws(() => withHosts(['a/b']), ConfigError);
  assert.throws(() => withHosts(['a b']), ConfigError);
  assert.deepEqual(withHosts(['internal.example.com']).request.trees[0].hosts, ['internal.example.com']);
});

// F3 (release-audit v0.14.0): a full-URL value (which always ALSO contains "/") must trip the
// URL-specific message, not the generic bare-path message the "/" check would otherwise fire first —
// pins the check-order fix in validateHostsArray.
test('trees[i].hosts full-URL value yields the URL-specific message, not the generic path one', () => {
  const withHosts = (h) => configToRequest({ trees: [{ root: '.', sourceId: 't', hosts: h }] });
  assert.throws(() => withHosts(['https://api.foo.com']), (err) => {
    assert.ok(err instanceof ConfigError);
    assert.match(err.message, /not a full URL/);
    assert.doesNotMatch(err.message, /not a path/);
    return true;
  });
});

test('collectConfigWarnings flags unknown keys inside a trees[i].mounts[] entry with the mount scope', () => {
  const warnings = collectConfigWarnings({
    trees: [{ root: '.', sourceId: 't', mounts: [{ dir: 'apps/api', at: '/api', prefx: true }] }],
  });
  assert.ok(
    warnings.some((w) => /"trees\[0\]\.mounts\[0\]\.prefx"/.test(w)),
    `expected a warning naming trees[0].mounts[0].prefx, got: ${JSON.stringify(warnings)}`
  );
});

test('collectConfigWarnings stays silent on well-formed mountedAt/mounts/hosts', () => {
  assert.deepEqual(
    collectConfigWarnings({
      trees: [
        {
          root: '.',
          sourceId: 't',
          mountedAt: '/gateway',
          mounts: [{ dir: 'apps/api', at: '/api' }],
          hosts: ['internal.example.com'],
        },
      ],
    }),
    []
  );
});
