'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

const {
  collectFindings,
  groupByFile,
  countBySeverity,
  formatPretty,
  formatJson,
  computeExitCode,
} = require('../lib/format');

const singleOutput = {
  fileCount: 3,
  findings: [
    { ruleId: 'b-rule', severity: 'warning', file: 'src/z.ts', line: 10, message: 'zzz' },
    { ruleId: 'a-rule', severity: 'critical', file: 'src/a.ts', line: 5, message: 'boom' },
    { ruleId: 'c-rule', severity: 'info', file: 'src/a.ts', line: 2, message: 'note' },
  ],
};

const treesOutput = {
  trees: [
    {
      root: './api',
      sourceId: 'api',
      output: {
        fileCount: 2,
        findings: [{ ruleId: 'x', severity: 'warning', file: 'api/h.ts', line: 1, message: 'm' }],
      },
    },
    {
      root: './web',
      sourceId: 'web',
      output: {
        fileCount: 4,
        findings: [{ ruleId: 'y', severity: 'critical', file: 'web/i.ts', line: 9, message: 'n' }],
      },
    },
  ],
  crossLayer: {},
};

test('collectFindings: single-tree shape', () => {
  const { findings, fileCount } = collectFindings(singleOutput);
  assert.equal(findings.length, 3);
  assert.equal(fileCount, 3);
});

test('collectFindings: multi-tree shape flattens + sums fileCount + tags source', () => {
  const { findings, fileCount } = collectFindings(treesOutput);
  assert.equal(findings.length, 2);
  assert.equal(fileCount, 6);
  assert.equal(findings[0].sourceId, 'api');
  assert.equal(findings[1].root, './web');
});

test('collectFindings: tolerates empty/garbage', () => {
  assert.deepEqual(collectFindings(null), { findings: [], fileCount: 0 });
  assert.deepEqual(collectFindings({}), { findings: [], fileCount: 0 });
});

test('groupByFile sorts files and orders within a file by line then ruleId', () => {
  const { findings } = collectFindings(singleOutput);
  const groups = groupByFile(findings);
  assert.deepEqual([...groups.keys()], ['src/a.ts', 'src/z.ts']);
  const aFile = groups.get('src/a.ts');
  assert.deepEqual(aFile.map((f) => f.line), [2, 5]);
});

test('countBySeverity tallies', () => {
  const { findings } = collectFindings(singleOutput);
  assert.deepEqual(countBySeverity(findings), { critical: 1, warning: 1, info: 1, other: 0 });
});

test('formatPretty (no color) groups by file and prints a footer', () => {
  const out = formatPretty(singleOutput, { color: false });
  assert.match(out, /src\/a\.ts/);
  assert.match(out, /src\/z\.ts/);
  assert.match(out, /3 findings in 3 files/);
  assert.match(out, /1 critical, 1 warning, 1 info/);
  // no ANSI escapes when color disabled
  assert.doesNotMatch(out, /\[/);
});

test('formatPretty color mode emits ANSI', () => {
  const out = formatPretty(singleOutput, { color: true });
  assert.match(out, /\[/);
});

test('formatPretty on empty findings says so', () => {
  const out = formatPretty({ fileCount: 1, findings: [] }, { color: false });
  assert.match(out, /No findings/);
  assert.match(out, /0 findings in 1 file/);
});

test('formatJson pretty-prints the raw output', () => {
  const out = formatJson(singleOutput);
  assert.equal(out, JSON.stringify(singleOutput, null, 2));
});

test('computeExitCode: failOn warn trips on warning and critical, not info', () => {
  assert.equal(computeExitCode([{ severity: 'info' }], 'warn'), 0);
  assert.equal(computeExitCode([{ severity: 'warning' }], 'warn'), 1);
  assert.equal(computeExitCode([{ severity: 'critical' }], 'warn'), 1);
});

test('computeExitCode: failOn critical only trips on critical', () => {
  assert.equal(computeExitCode([{ severity: 'warning' }], 'critical'), 0);
  assert.equal(computeExitCode([{ severity: 'critical' }], 'critical'), 1);
});

test('computeExitCode: failOn off always 0', () => {
  assert.equal(computeExitCode([{ severity: 'critical' }], 'off'), 0);
});

test('computeExitCode: empty findings -> 0', () => {
  assert.equal(computeExitCode([], 'warn'), 0);
});
