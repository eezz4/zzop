'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

const { buildReports, DEFAULT_FORMATS } = require('../lib/report');

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
