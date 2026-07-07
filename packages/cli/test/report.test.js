'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

const { buildReports, buildMarkdownReports, DEFAULT_FORMATS } = require('../lib/report');

const output = {
  fileCount: 2,
  findings: [
    { ruleId: 'a-rule', severity: 'critical', file: 'src/a.ts', line: 5, message: 'boom' },
    { ruleId: 'b-rule', severity: 'warning', file: 'src\\win.ts', line: 10, message: 'zzz' },
    { ruleId: 'c-rule', severity: 'info', file: 'src/c.ts', line: 1, message: 'note' },
  ],
};

test('default formats produce report.json and report.sarif', () => {
  const files = buildReports(output);
  assert.deepEqual(DEFAULT_FORMATS, ['json', 'sarif']);
  assert.deepEqual(files.map((f) => f.name), ['report.json', 'report.sarif']);
});

test('json report is the pretty-printed raw output', () => {
  const [json] = buildReports(output, { formats: ['json'] });
  assert.equal(json.content, JSON.stringify(output, null, 2));
});

test('sarif report is valid 2.1.0 with mapped levels and forward-slash uris', () => {
  const [sarif] = buildReports(output, { formats: ['sarif'], toolVersion: '9.9.9' });
  const doc = JSON.parse(sarif.content);
  assert.equal(doc.version, '2.1.0');
  const driver = doc.runs[0].tool.driver;
  assert.equal(driver.name, 'zzop');
  assert.equal(driver.version, '9.9.9');
  // one reportingDescriptor per unique ruleId, sorted
  assert.deepEqual(driver.rules.map((r) => r.id), ['a-rule', 'b-rule', 'c-rule']);
  const results = doc.runs[0].results;
  assert.equal(results.length, 3);
  // severity -> SARIF level mapping
  assert.deepEqual(results.map((r) => r.level), ['error', 'warning', 'note']);
  // backslash path normalized to forward slashes
  assert.equal(results[1].locations[0].physicalLocation.artifactLocation.uri, 'src/win.ts');
  assert.equal(results[0].locations[0].physicalLocation.region.startLine, 5);
});

test('multi-tree output is flattened into sarif results', () => {
  const trees = {
    trees: [
      { root: './api', sourceId: 'api', output: { fileCount: 1, findings: [{ ruleId: 'x', severity: 'warning', file: 'api/h.ts', line: 1, message: 'm' }] } },
      { root: './web', sourceId: 'web', output: { fileCount: 1, findings: [{ ruleId: 'y', severity: 'critical', file: 'web/i.ts', line: 9, message: 'n' }] } },
    ],
    crossLayer: {},
  };
  const [sarif] = buildReports(trees, { formats: ['sarif'] });
  assert.equal(JSON.parse(sarif.content).runs[0].results.length, 2);
});

test('unknown format throws', () => {
  assert.throws(() => buildReports(output, { formats: ['pdf'] }), /Unknown report format/);
});

test('empty findings still produce a valid (empty-results) sarif', () => {
  const [sarif] = buildReports({ fileCount: 0, findings: [] }, { formats: ['sarif'] });
  const doc = JSON.parse(sarif.content);
  assert.equal(doc.runs[0].results.length, 0);
  assert.deepEqual(doc.runs[0].tool.driver.rules, []);
});

// --- markdown report ------------------------------------------------------------------------------------

function multiTreeOutput() {
  return {
    trees: [
      {
        root: './services/api',
        sourceId: 'api',
        output: {
          fileCount: 3,
          findings: [
            { ruleId: 'a-rule', severity: 'critical', file: 'src/a.ts', line: 5, message: 'boom' },
            { ruleId: 'b-rule', severity: 'warning', file: 'src/a.ts', line: 1, message: 'zzz' },
            { ruleId: 'dead-candidates', severity: 'info', file: 'src/c.ts', line: 1, message: 'note' },
            { ruleId: 'dead-candidates', severity: 'info', file: 'src/d.ts', line: 1, message: 'note2' },
            { ruleId: 'as-cast', severity: 'info', file: 'src/e.ts', line: 1, message: 'note3' },
          ],
          warnings: ['git not requested'],
          ir: {
            ir: {
              io: {
                provides: [
                  { kind: 'http', key: 'GET /api/articles', file: 'src/routes.ts', line: 3 },
                  { kind: 'topic', key: 'articles.created', file: 'src/routes.ts', line: 9 },
                ],
                consumes: [],
              },
            },
          },
        },
      },
      {
        root: './apps/web',
        sourceId: 'web',
        output: {
          fileCount: 2,
          findings: [],
          warnings: [],
          // Single-nested `ir.io` — the real napi wire shape (see packages/core/src/ir.rs's serde
          // flatten). The sibling tree above uses the doubly-nested `ir.ir.io` shape to exercise the
          // defensive fallback; both must resolve to the same HTTP-interface behavior.
          ir: {
            io: {
              provides: [],
              consumes: [
                { kind: 'http', key: 'GET /api/articles', file: 'src/client.ts', line: 4 },
                { kind: 'http', key: null, raw: 'sdk.fetch(x)', file: 'src/client.ts', line: 10 },
              ],
            },
          },
        },
      },
    ],
    crossLayer: {
      edges: [
        {
          kind: 'http',
          key: 'GET /api/articles',
          from: { source: 'web', file: 'src/client.ts', line: 4 },
          to: { source: 'api', file: 'src/routes.ts', line: 3, symbol: 'listArticles' },
          crossSource: true,
        },
        {
          kind: 'http',
          key: '/health',
          from: { source: 'web', file: 'src/health.ts', line: 1 },
          to: { source: 'api', file: 'src/health.ts', line: 1 },
          crossSource: true,
          lowConfidenceReason: 'generic path shared across many services',
        },
      ],
      unconsumedProvides: [
        { source: 'api', kind: 'topic', key: 'articles.created', file: 'src/routes.ts', line: 9 },
      ],
      unprovidedConsumes: [
        { source: 'web', kind: 'http', key: 'GET /api/missing', file: 'src/client.ts', line: 20 },
      ],
      unresolvedConsumes: [
        { source: 'web', kind: 'http', key: null, raw: 'sdk.fetch(x)', file: 'src/client.ts', line: 10 },
      ],
      externalConsumes: [],
      ambiguousConsumes: [],
    },
    crossLayerFindings: [
      {
        ruleId: 'cross-layer/sdk-import-no-visible-consume',
        severity: 'info',
        file: 'src/client.ts',
        line: 1,
        message: 'source `web` imports the client/SDK package `foo-sdk` from 3 files. Prefer literal paths.',
        data: { source: 'web' },
      },
      {
        ruleId: 'cross-layer/unconsumed-endpoint',
        severity: 'warning',
        file: 'src/routes.ts',
        line: 9,
        message: 'route articles.created is never consumed by any known tree. Consider removing it.',
      },
    ],
  };
}

test('buildMarkdownReports: multi-tree emits cross-repo.md + one file per tree', () => {
  const files = buildMarkdownReports(multiTreeOutput());
  assert.deepEqual(files.map((f) => f.name), ['cross-repo.md', 'api.md', 'web.md']);
});

test('buildMarkdownReports: cross-repo.md surfaces coverage self-reports first, then edges/buckets', () => {
  const [crossRepo] = buildMarkdownReports(multiTreeOutput());
  const content = crossRepo.content;

  assert.match(content, /# Cross-repo analysis/);
  assert.match(content, /`api` — 3 files \(\.\/services\/api\)/);
  assert.match(content, /`web` — 2 files \(\.\/apps\/web\)/);

  // Coverage & blindness comes before the edges section and surfaces the sdk-import self-report.
  const coverageIdx = content.indexOf('## Coverage & blindness');
  const edgesIdx = content.indexOf('## Cross-repo edges');
  assert.ok(coverageIdx > -1 && edgesIdx > coverageIdx);
  assert.match(content, /\*\*web\*\* — source `web` imports the client\/SDK package/);

  // One cross-source edge is a low-confidence match; count reflects only crossSource==true edges.
  assert.match(content, /## Cross-repo edges \(2\)/);
  assert.match(
    content,
    /`GET \/api\/articles`: `web` \(src\/client\.ts:4\) -> `api` \(src\/routes\.ts:3\)/
  );
  assert.match(content, /low confidence: generic path shared across many services/);

  assert.match(content, /## Unprovided consumes \(1\)/);
  assert.match(content, /`GET \/api\/missing` consumed by `web` \(src\/client\.ts:20\)/);

  assert.match(content, /## Unconsumed provides \(1\)/);
  assert.match(content, /`articles\.created` provided by `api` \(src\/routes\.ts:9\)/);

  assert.match(content, /Unresolved consumes: 1 {3}External consumes: 0 {3}Ambiguous consumes: 0/);

  // The coverage self-report rule is excluded from the generic cross-layer findings bucket (shown once).
  assert.match(content, /## Cross-layer findings \(1\)/);
  assert.match(content, /### cross-layer\/unconsumed-endpoint \(1\)/);
  assert.doesNotMatch(content, /### cross-layer\/sdk-import-no-visible-consume/);
});

test('buildMarkdownReports: per-tree file has HTTP interface + findings incl. folded info', () => {
  const files = buildMarkdownReports(multiTreeOutput());
  const api = files.find((f) => f.name === 'api.md').content;

  assert.match(api, /^# api/);
  assert.match(api, /- Root: `\.\/services\/api`/);
  assert.match(api, /- Files analyzed: 3/);
  assert.match(api, /- Findings: 5 \(1 critical, 1 warning, 3 info\)/);
  assert.match(api, /## Warnings/);
  assert.match(api, /- git not requested/);
  assert.match(api, /### Provides \(routes served\)/);
  assert.match(api, /- `GET \/api\/articles` — src\/routes\.ts:3/);
  // non-http provide (kind "topic") is excluded from the HTTP interface section
  assert.doesNotMatch(api, /articles\.created` —/);
  assert.match(api, /### src\/a\.ts/);
  assert.match(api, /- \*\*critical\*\* L5 — boom `a-rule`/);
  assert.match(api, /- \*\*warning\*\* L1 — zzz `b-rule`/);
  assert.match(api, /#### info \(folded\)/);
  assert.match(api, /- 2 × `dead-candidates`/);
  assert.match(api, /- 1 × `as-cast`/);

  const web = files.find((f) => f.name === 'web.md').content;
  assert.match(web, /### Consumes \(routes called\)/);
  assert.match(web, /- `GET \/api\/articles` — src\/client\.ts:4/);
  assert.match(web, /- `sdk\.fetch\(x\)` \(unresolved\) — src\/client\.ts:10/);
});

test('buildMarkdownReports: single-tree emits one file, no cross-repo.md', () => {
  const output = {
    fileCount: 1,
    findings: [{ ruleId: 'a-rule', severity: 'critical', file: 'src/a.ts', line: 5, message: 'boom' }],
  };
  const files = buildMarkdownReports(output, { sourceId: 'my-app', root: '.' });
  assert.deepEqual(files.map((f) => f.name), ['my-app.md']);
  assert.match(files[0].content, /^# my-app/);
  assert.match(files[0].content, /- Root: `\.`/);
});

test('buildMarkdownReports: single-tree with no ctx falls back to sourceId "report"', () => {
  const files = buildMarkdownReports({ fileCount: 0, findings: [] });
  assert.deepEqual(files.map((f) => f.name), ['report.md']);
  assert.match(files[0].content, /^# report/);
});

test('buildMarkdownReports: sourceId slugs are sanitized and collisions get -2, -3, ...', () => {
  const output = {
    trees: [
      { root: './a', sourceId: 'My API!', output: { fileCount: 1, findings: [] } },
      { root: './b', sourceId: 'my--api', output: { fileCount: 1, findings: [] } },
      { root: './c', sourceId: 'Foo.Bar', output: { fileCount: 1, findings: [] } },
      { root: './d', sourceId: 'foo.bar', output: { fileCount: 1, findings: [] } },
    ],
  };
  const files = buildMarkdownReports(output);
  assert.deepEqual(files.map((f) => f.name), [
    'cross-repo.md',
    'my-api.md',
    'my-api-2.md',
    'foo.bar.md',
    'foo.bar-2.md',
  ]);
});

test('buildMarkdownReports: empty/all-symbol sourceIds fall back to tree-<index> (no false collision)', () => {
  const output = {
    trees: [
      { root: './a', sourceId: '???', output: { fileCount: 1, findings: [] } },
      { root: './b', sourceId: '', output: { fileCount: 1, findings: [] } },
    ],
  };
  const files = buildMarkdownReports(output);
  // Distinct fallback slugs (index-derived) — never collide with each other even though both sourceIds
  // sanitize to empty.
  assert.deepEqual(files.map((f) => f.name), ['cross-repo.md', 'tree-0.md', 'tree-1.md']);
});

test('buildMarkdownReports: building the same output twice is byte-identical (determinism)', () => {
  const output = multiTreeOutput();
  const first = buildMarkdownReports(output);
  const second = buildMarkdownReports(multiTreeOutput());
  assert.deepEqual(first, second);
});

test('buildMarkdownReports: no coverage gaps message when no self-report findings present', () => {
  const output = multiTreeOutput();
  output.crossLayerFindings = output.crossLayerFindings.filter(
    (f) => f.ruleId !== 'cross-layer/sdk-import-no-visible-consume'
  );
  const [crossRepo] = buildMarkdownReports(output);
  assert.match(
    crossRepo.content,
    /No coverage gaps detected — consume extraction was visible for all trees\./
  );
});

test('buildReports: format "md" flows through the generic build() and returns the multi-file set', () => {
  const files = buildReports(multiTreeOutput(), { formats: ['md'] });
  assert.deepEqual(files.map((f) => f.name), ['cross-repo.md', 'api.md', 'web.md']);
});
