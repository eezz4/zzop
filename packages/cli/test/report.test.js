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

test('multi-tree crossLayerFindings are included in sarif results (not just per-tree findings)', () => {
  const trees = {
    trees: [
      { root: './api', sourceId: 'api', output: { fileCount: 1, findings: [{ ruleId: 'x', severity: 'warning', file: 'api/h.ts', line: 1, message: 'm' }] } },
    ],
    crossLayer: {},
    crossLayerFindings: [
      { ruleId: 'cross-layer/duplicate-route', severity: 'warning', file: 'a.ts', line: 3, message: 'dup route' },
    ],
  };
  const [sarif] = buildReports(trees, { formats: ['sarif'] });
  const doc = JSON.parse(sarif.content);
  assert.equal(doc.runs[0].results.length, 2);
  assert.ok(doc.runs[0].results.some((r) => r.ruleId === 'cross-layer/duplicate-route'));
  assert.deepEqual(doc.runs[0].tool.driver.rules.map((r) => r.id), ['cross-layer/duplicate-route', 'x']);
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
          coverage: {
            files: 3,
            symbols: 20,
            importEdges: 8,
            ioProvides: 2,
            ioConsumesKeyed: 0,
            ioConsumesUnresolved: 0,
            degraded: 0,
            joinContributionZero: false,
          },
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
          coverage: {
            files: 2,
            symbols: 10,
            importEdges: 4,
            ioProvides: 0,
            ioConsumesKeyed: 1,
            ioConsumesUnresolved: 1,
            degraded: 0,
            joinContributionZero: false,
          },
          // Single-nested `ir.io` — the real wire shape (see crates/core/src/ir.rs's serde
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
    disclosure: [
      { id: 'consume-side-unextracted', group: 'extraction-blind', summary: 'consume side gap.', status: 'asserted' },
      { id: 'language-unparsed', group: 'extraction-blind', summary: 'lang gap.', status: 'partial' },
      { id: 'provide-side-unextracted', group: 'extraction-blind', summary: 'provider under-extracted.', status: 'notYetDetected' },
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

  // Per-tree census table, sorted by sourceId ("api" before "web").
  assert.match(content, /- `api`: 3 files, 2 provides, 0 consumes \(0 keyed \/ 0 unresolved\), 0 degraded/);
  assert.match(content, /- `web`: 2 files, 0 provides, 2 consumes \(1 keyed \/ 1 unresolved\), 0 degraded/);
  assert.ok(content.indexOf('- `api`:') < content.indexOf('- `web`:'));
  assert.ok(content.indexOf('- `api`:') < coverageIdx + '## Coverage & blindness'.length + 200);

  // Neither tree is joinContributionZero, so no BLIND assertion and the "no fully blind" line appears.
  assert.doesNotMatch(content, /BLIND:/);
  assert.match(content, /- No fully IO-blind trees detected\./);

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

  // Unresolved consume SITES are listed under the count (raw call-site text is the lead an agent
  // needs to resolve the indirection) — not just counted.
  assert.match(content, /- `sdk\.fetch\(x\)` \(unresolved\) — `web` src\/client\.ts:10/);

  // The coverage self-report rule is excluded from the generic cross-layer findings bucket (shown once).
  assert.match(content, /## Cross-layer findings \(1\)/);
  assert.match(content, /### cross-layer\/unconsumed-endpoint \(1\)/);
  assert.doesNotMatch(content, /### cross-layer\/sdk-import-no-visible-consume/);
});

test('buildMarkdownReports: unresolved consume list is capped with an announced remainder, never silently', () => {
  const output = multiTreeOutput();
  output.crossLayer.unresolvedConsumes = Array.from({ length: 25 }, (_, i) => ({
    source: 'web',
    kind: 'http',
    key: null,
    raw: `wrapped(${i})`,
    file: `src/c${String(i).padStart(2, '0')}.ts`,
    line: 1,
  }));
  const [crossRepo] = buildMarkdownReports(output);
  assert.match(crossRepo.content, /Unresolved consumes: 25/);
  assert.match(crossRepo.content, /- `wrapped\(0\)` \(unresolved\) — `web` src\/c00\.ts:1/);
  assert.match(crossRepo.content, /- \.\.\. and 5 more unresolved consume site\(s\)/);
  // The 21st entry (c20) is past the cap and must not be listed.
  assert.doesNotMatch(crossRepo.content, /src\/c20\.ts/);
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

test('buildMarkdownReports: per-tree file renders the Coverage section with census numbers', () => {
  const files = buildMarkdownReports(multiTreeOutput());
  const api = files.find((f) => f.name === 'api.md').content;

  assert.match(api, /## Coverage\n- Files: 3 {3}Symbols: 20 {3}Import edges: 8/);
  assert.match(api, /- IO: 2 provides, 0 consumes keyed, 0 unresolved {3}Degraded files: 0/);
  // api's coverage.joinContributionZero is false -> no Blindness bullet.
  assert.doesNotMatch(api, /Blindness:/);

  const web = files.find((f) => f.name === 'web.md').content;
  assert.match(web, /## Coverage\n- Files: 2 {3}Symbols: 10 {3}Import edges: 4/);
  assert.match(web, /- IO: 0 provides, 1 consumes keyed, 1 unresolved {3}Degraded files: 0/);
  assert.doesNotMatch(web, /Blindness:/);
});

test('buildMarkdownReports: per-tree file renders the Blindness bullet when joinContributionZero is true', () => {
  const output = multiTreeOutput();
  output.trees[1].output.coverage.joinContributionZero = true;
  const files = buildMarkdownReports(output);
  const web = files.find((f) => f.name === 'web.md').content;
  assert.match(
    web,
    /- Blindness: no JOINABLE io surface was extracted from this tree \(0 provides, 0 keyed consumes across 2 files — unresolved consumes cannot join\), so it is invisible to the cross-layer join — discount any "unconsumed"\/"unprovided" verdict that references it\. If this tree does call an API, the calls flow through a client the extractor cannot see; project them with a Mode B adapter and attach it via the `overlays: \["\.\/my-adapter\/envelope\.json"\]` config key to restore visibility\./
  );
});

test('buildMarkdownReports: per-tree Coverage section renders even with no coverage field at all (defensive defaults)', () => {
  const output = {
    fileCount: 1,
    findings: [],
  };
  const files = buildMarkdownReports(output, { sourceId: 'bare', root: '.' });
  // No census present -> Files falls back to the tree's own fileCount (1), never a self-contradictory 0.
  assert.match(files[0].content, /## Coverage\n- Files: 1 {3}Symbols: 0 {3}Import edges: 0/);
  assert.match(files[0].content, /- IO: 0 provides, 0 consumes keyed, 0 unresolved {3}Degraded files: 0/);
  assert.doesNotMatch(files[0].content, /Blindness:/);
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

test('buildMarkdownReports: no coverage-rule bullets when no self-report findings present (census table still carries the section)', () => {
  const output = multiTreeOutput();
  output.crossLayerFindings = output.crossLayerFindings.filter(
    (f) => f.ruleId !== 'cross-layer/sdk-import-no-visible-consume'
  );
  const [crossRepo] = buildMarkdownReports(output);
  // The old standalone "No coverage gaps detected..." string is gone; the census table + blind
  // assertions now carry the section, so no coverage-rule bullet (`**tag** — ...`) is printed.
  assert.doesNotMatch(crossRepo.content, /No coverage gaps detected/);
  assert.doesNotMatch(crossRepo.content, /\*\*web\*\* — source `web` imports the client\/SDK package/);
  assert.match(crossRepo.content, /- `api`: 3 files, 2 provides, 0 consumes \(0 keyed \/ 0 unresolved\), 0 degraded/);
});

test('buildMarkdownReports: cross-repo.md renders run-level warnings (parallel-impl tripwire class) when present, omits the section when absent', () => {
  const output = multiTreeOutput();
  output.warnings = [
    'this join produced 0 cross-source edges but 7 duplicate-route/ambiguous-consume findings — the trees may be parallel implementations',
  ];
  const [crossRepo] = buildMarkdownReports(output);
  assert.match(crossRepo.content, /## Run warnings/);
  assert.match(crossRepo.content, /parallel implementations/);

  const clean = multiTreeOutput();
  const [cleanCrossRepo] = buildMarkdownReports(clean);
  assert.doesNotMatch(cleanCrossRepo.content, /## Run warnings/);
});

test('buildMarkdownReports: BLIND assertion for a joinContributionZero tree, placed before Cross-repo edges', () => {
  const output = multiTreeOutput();
  output.trees[1].output.coverage.joinContributionZero = true;
  const [crossRepo] = buildMarkdownReports(output);
  const content = crossRepo.content;

  const blindIdx = content.indexOf(
    '- BLIND: `web` contributed no JOINABLE io to the join (0 provides, 0 keyed consumes across 2 files) — join findings that reference it are structurally weak; see its per-tree report for guidance.'
  );
  const edgesIdx = content.indexOf('## Cross-repo edges');
  assert.ok(blindIdx > -1 && edgesIdx > blindIdx);
  assert.doesNotMatch(content, /No fully IO-blind trees detected\./);
});

test('buildReports: format "md" flows through the generic build() and returns the multi-file set', () => {
  const files = buildReports(multiTreeOutput(), { formats: ['md'] });
  assert.deepEqual(files.map((f) => f.name), ['cross-repo.md', 'api.md', 'web.md']);
});

test('buildMarkdownReports: cross-repo.md footer renders the disclosure registry once (asserted count + not-yet-detected + partial)', () => {
  const files = buildMarkdownReports(multiTreeOutput());
  const crossRepo = files.find((f) => f.name === 'cross-repo.md').content;

  assert.match(crossRepo, /## Disclosure coverage/);
  assert.match(crossRepo, /zzop actively asserts 1 of 3 known silent-failure classes/);
  // Not-yet-detected spelled out with its summary (the actionable "do not assume I caught this").
  assert.match(crossRepo, /Not yet detected:\n- `provide-side-unextracted`: provider under-extracted\./);
  // Partial listed by id.
  assert.match(crossRepo, /Partial \(detected in common cases, may miss members\): `language-unparsed`\./);
  // The registry is a footer — it comes after the cross-layer findings section.
  assert.ok(crossRepo.indexOf('## Cross-layer findings') < crossRepo.indexOf('## Disclosure coverage'));

  // Per-tree files never repeat the registry (it lives once on cross-repo.md).
  const api = files.find((f) => f.name === 'api.md').content;
  const web = files.find((f) => f.name === 'web.md').content;
  assert.doesNotMatch(api, /## Disclosure coverage/);
  assert.doesNotMatch(web, /## Disclosure coverage/);
});

test('buildMarkdownReports: single-tree file renders the disclosure registry when present at the root', () => {
  const output = {
    fileCount: 1,
    findings: [],
    disclosure: [
      { id: 'consume-side-unextracted', group: 'extraction-blind', summary: 'ok.', status: 'asserted' },
      { id: 'input-scope-error', group: 'input-config', summary: 'typo root reads as empty repo.', status: 'notYetDetected' },
    ],
  };
  const files = buildMarkdownReports(output, { sourceId: 'solo', root: '.' });
  assert.deepEqual(files.map((f) => f.name), ['solo.md']);
  assert.match(files[0].content, /## Disclosure coverage/);
  assert.match(files[0].content, /zzop actively asserts 1 of 2 known silent-failure classes/);
  assert.match(files[0].content, /- `input-scope-error`: typo root reads as empty repo\./);
});

test('buildMarkdownReports: no Disclosure coverage section when the registry is absent (defensive)', () => {
  const files = buildMarkdownReports({ fileCount: 1, findings: [] }, { sourceId: 'bare', root: '.' });
  assert.doesNotMatch(files[0].content, /## Disclosure coverage/);
});

// --- one-sided IO guidance --------------------------------------------------------------------------

function singleTreeConsumeOnlyOutput() {
  return {
    fileCount: 4,
    findings: [],
    warnings: [],
    coverage: {
      files: 4,
      symbols: 9,
      importEdges: 3,
      ioProvides: 0,
      ioConsumesKeyed: 3,
      ioConsumesUnresolved: 1,
      degraded: 0,
      joinContributionZero: false,
    },
  };
}

test('buildMarkdownReports: single-tree file renders the one-sided IO guidance when consumes exist but provides are 0', () => {
  const files = buildMarkdownReports(singleTreeConsumeOnlyOutput(), { sourceId: 'app', root: '.' });
  const content = files[0].content;
  assert.match(
    content,
    /- One-sided IO \(no provides\): 4 consumes were extracted but 0 provides, so every consume in this run is structurally guaranteed to look unprovided/
  );
  // All three fix directions, in order: attach the provider repo, Mode B injection, external-is-expected.
  const attachIdx = content.indexOf('attach that checkout as another tree and re-run so both sides are reviewed together — `"trees": [{ "root": ".", "sourceId": "consumer" }, { "root": "../provider-repo", "sourceId": "provider" }]`');
  const modeBIdx = content.indexOf('The serving code is inside this tree but its framework is not natively extracted: project its routes with a Mode B overlay adapter');
  const externalIdx = content.indexOf('The consumes only target third-party/external APIs');
  assert.ok(attachIdx > -1 && modeBIdx > attachIdx && externalIdx > modeBIdx);
  // FE/BE-neutral wording lock: the guidance must name the non-backend serving shapes too.
  assert.match(content, /a backend, a peer service, a module-federation remote/);
});

test('buildMarkdownReports: single-tree one-sided IO guidance absent when provides > 0 or when fully IO-blind (0/0)', () => {
  const withProvides = singleTreeConsumeOnlyOutput();
  withProvides.coverage.ioProvides = 2;
  const [healthy] = buildMarkdownReports(withProvides, { sourceId: 'app', root: '.' });
  assert.doesNotMatch(healthy.content, /One-sided IO/);

  // 0 provides AND 0 keyed consumes is the joinContributionZero case — the Blindness bullet owns it;
  // the one-sided guidance must not double-fire.
  const blind = singleTreeConsumeOnlyOutput();
  blind.coverage.ioConsumesKeyed = 0;
  blind.coverage.ioConsumesUnresolved = 0;
  blind.coverage.joinContributionZero = true;
  const [blindFile] = buildMarkdownReports(blind, { sourceId: 'app', root: '.' });
  assert.match(blindFile.content, /- Blindness:/);
  assert.doesNotMatch(blindFile.content, /One-sided IO/);

  // Unresolved-only tree (0 provides, 0 keyed, N unresolved): joinContributionZero is TRUE under the
  // engine's semantics (unresolved consumes cannot join) — the Blindness bullet fires alone and its
  // text must not claim "0 consumes" while the Coverage line above shows unresolved ones.
  const unresolvedOnly = singleTreeConsumeOnlyOutput();
  unresolvedOnly.coverage.ioConsumesKeyed = 0;
  unresolvedOnly.coverage.ioConsumesUnresolved = 3;
  unresolvedOnly.coverage.joinContributionZero = true;
  const [unresolvedFile] = buildMarkdownReports(unresolvedOnly, { sourceId: 'app', root: '.' });
  assert.match(unresolvedFile.content, /- Blindness: no JOINABLE io surface/);
  assert.match(unresolvedFile.content, /0 keyed consumes/);
  assert.match(unresolvedFile.content, /unresolved consumes cannot join/);
  assert.doesNotMatch(unresolvedFile.content, /One-sided IO/);
});

test('buildMarkdownReports: per-tree file in a multi-tree run never renders the one-sided IO guidance (consume-only FE tree is healthy there)', () => {
  const files = buildMarkdownReports(multiTreeOutput());
  const web = files.find((f) => f.name === 'web.md').content;
  // web is 0 provides / 2 consumes — one-sided as a tree, but the api tree provides; run-level check
  // lives on cross-repo.md, so the per-tree file stays quiet.
  assert.doesNotMatch(web, /One-sided IO/);
});

test('buildMarkdownReports: cross-repo.md renders the run-level one-sided IO guidance only when NO tree provides anything', () => {
  // Default fixture: api has 2 provides -> no run-level one-sided guidance.
  const [withProvider] = buildMarkdownReports(multiTreeOutput());
  assert.doesNotMatch(withProvider.content, /One-sided IO/);

  const output = multiTreeOutput();
  output.trees[0].output.coverage.ioProvides = 0;
  const [crossRepo] = buildMarkdownReports(output);
  const content = crossRepo.content;
  assert.match(
    content,
    /- One-sided IO \(no provides in any tree\): 2 consumes were extracted across all trees but 0 provides/
  );
  assert.match(content, /The serving code is inside an attached tree but its framework is not natively extracted/);
  // Placed inside Coverage & blindness, before the edges section.
  const oneSidedIdx = content.indexOf('One-sided IO');
  const edgesIdx = content.indexOf('## Cross-repo edges');
  assert.ok(oneSidedIdx > content.indexOf('## Coverage & blindness') && oneSidedIdx < edgesIdx);
});

test('buildMarkdownReports: unprovided consumes section carries the cause-taxonomy line when nonempty, not when empty', () => {
  const [crossRepo] = buildMarkdownReports(multiTreeOutput());
  assert.match(
    crossRepo.content,
    /## Unprovided consumes \(1\)\nNo attached tree provides these keys\. Three causes: \(a\) the repository serving these endpoints is not part of this run — attach its checkout as another tree so both sides are reviewed together; \(b\) the serving code is in an attached tree but its routes were not extracted — project them with a Mode B overlay adapter and attach it via the `overlays: \["\.\/my-adapter\/envelope\.json"\]` config key; \(c\) real spec drift\. A cluster sharing one path prefix usually means \(a\)\./
  );

  const output = multiTreeOutput();
  output.crossLayer.unprovidedConsumes = [];
  const [empty] = buildMarkdownReports(output);
  assert.match(empty.content, /## Unprovided consumes \(0\)\nNone\./);
  assert.doesNotMatch(empty.content, /No attached tree provides these keys/);
});

test('buildMarkdownReports: single-tree summary carries the Rule packs loaded bullet when present', () => {
  const output = {
    fileCount: 1,
    findings: [],
    warnings: [],
    packsLoaded: [
      { id: 'be-security', rules: 3, source: 'dir' },
      { id: 'custom', rules: 1, source: 'inline' },
    ],
  };
  const [file] = buildMarkdownReports(output, { sourceId: 'app', root: '/repo' });
  assert.ok(
    file.content.includes('- Rule packs loaded: 2 (4 rules) — `be-security`, `custom`'),
    `expected the pack-load bullet, got:\n${file.content}`
  );
});

test('buildMarkdownReports: no Rule packs bullet for an older output without packsLoaded', () => {
  const output = { fileCount: 1, findings: [], warnings: [] };
  const [file] = buildMarkdownReports(output, { sourceId: 'app', root: '/repo' });
  assert.ok(!file.content.includes('Rule packs loaded'), `got:\n${file.content}`);
});

test('buildMarkdownReports: "## Architecture / churn signals" renders health/recommendations/hotspots when present', () => {
  const output = {
    fileCount: 3,
    findings: [],
    health: {
      pain: 13.4,
      contributors: [
        { metric: 'fsd', weight: 2.5, gap: 0.328, contribution: 8.2 },
        { metric: 'godFile', weight: 1.5, gap: 0.2, contribution: 3.0 },
        { metric: 'circular', weight: 3.0, gap: 1.0, contribution: 30 }, // 4th place after slice(0,3) below
      ],
    },
    recommendations: [
      {
        id: 'bug-prone',
        severity: 'critical',
        items: [{ path: 'src/checkout.ts', note: 'FIX 8 · risk 120', estimatedReduction: 12, estimatedCost: 10, roi: 1.2, actionHintKey: 'bug-prone-shared', fanIn: 4 }],
      },
      {
        id: 'hot-churn',
        severity: 'warning',
        items: [{ path: 'src/cart.ts', estimatedReduction: 5, estimatedCost: 10, roi: 0.5, actionHintKey: 'hot-churn-core', fanIn: 2 }],
      },
    ],
    nodes: [
      { path: 'src/checkout.ts', changeCount: 40, hotspotScore: 40000 },
      { path: 'src/cart.ts', changeCount: 20, hotspotScore: 12000 },
      { path: 'src/quiet.ts', changeCount: 1, hotspotScore: 0 },
    ],
  };
  const [file] = buildMarkdownReports(output, { sourceId: 'app', root: '.' });
  const content = file.content;
  assert.match(content, /## Architecture \/ churn signals/);
  assert.match(content, /- Health pain: 13\.4/);
  assert.match(content, /- fsd: 8\.2/);
  assert.match(content, /- godFile: 3/);
  assert.match(content, /- Top recommendations:/);
  assert.match(content, /\*\*critical\*\* `bug-prone` — src\/checkout\.ts \(FIX 8 · risk 120\)/);
  assert.match(content, /\*\*warning\*\* `hot-churn` — src\/cart\.ts/);
  assert.match(content, /\| src\/checkout\.ts \| 40 \| 40000 \|/);
  assert.match(content, /\| src\/cart\.ts \| 20 \| 12000 \|/);
  // zero-hotspot node excluded from the top-hotspots table.
  assert.doesNotMatch(content, /src\/quiet\.ts/);
  assert.match(content, /Full detail \(all metrics, every recommendation item, every node\) in report\.json\/--json\./);
});

test('buildMarkdownReports: "## Architecture / churn signals" absent when health/recommendations/nodes are all absent', () => {
  const output = { fileCount: 1, findings: [], warnings: [] };
  const [file] = buildMarkdownReports(output, { sourceId: 'app', root: '/repo' });
  assert.ok(!file.content.includes('Architecture / churn signals'), `got:\n${file.content}`);
});

test('buildMarkdownReports: "## Architecture / churn signals" degrades one part at a time (only recommendations present)', () => {
  const output = {
    fileCount: 1,
    findings: [],
    recommendations: [{ id: 'fat-fanout', severity: 'warning', items: [{ path: 'src/x.ts' }] }],
  };
  const [file] = buildMarkdownReports(output, { sourceId: 'app', root: '.' });
  assert.match(file.content, /## Architecture \/ churn signals/);
  assert.match(file.content, /\*\*warning\*\* `fat-fanout` — src\/x\.ts/);
  assert.doesNotMatch(file.content, /Health pain/);
  assert.doesNotMatch(file.content, /Top hotspots/);
});
