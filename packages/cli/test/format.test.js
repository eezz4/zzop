'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

const {
  collectFindings,
  collectWarnings,
  groupByFile,
  countBySeverity,
  filterOutputBySeverity,
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

test('formatPretty folds info findings to a per-rule count block by default', () => {
  const output = {
    fileCount: 2,
    findings: [
      { ruleId: 'b-rule', severity: 'warning', file: 'src/z.ts', line: 10, message: 'zzz' },
      { ruleId: 'dead-candidates', severity: 'info', file: 'src/a.ts', line: 1, message: 'x' },
      { ruleId: 'dead-candidates', severity: 'info', file: 'src/b.ts', line: 1, message: 'y' },
      { ruleId: 'as-cast', severity: 'info', file: 'src/c.ts', line: 1, message: 'z' },
    ],
  };
  const out = formatPretty(output, { color: false });
  // warning is shown inline; info is folded, not listed per-file
  assert.match(out, /src\/z\.ts/);
  assert.doesNotMatch(out, /src\/a\.ts/);
  // folded summary shows per-rule counts + the expand hint
  assert.match(out, /3 findings folded/);
  assert.match(out, /2 {2}dead-candidates/);
  assert.match(out, /1 {2}as-cast/);
  assert.match(out, /pass --all to show/);
});

test('formatPretty showAllInfo expands info findings inline', () => {
  const output = {
    fileCount: 1,
    findings: [{ ruleId: 'dead-candidates', severity: 'info', file: 'src/a.ts', line: 1, message: 'x' }],
  };
  const out = formatPretty(output, { color: false, showAllInfo: true });
  assert.match(out, /src\/a\.ts/);
  assert.doesNotMatch(out, /folded/);
});

test('formatPretty with only info findings says no warnings/errors, then folds', () => {
  const output = {
    fileCount: 1,
    findings: [{ ruleId: 'as-cast', severity: 'info', file: 'src/a.ts', line: 1, message: 'x' }],
  };
  const out = formatPretty(output, { color: false });
  assert.match(out, /No warnings or errors/);
  assert.match(out, /1 finding folded/);
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

test('collectWarnings gathers engine warnings, tagging multi-tree entries by source', () => {
  assert.deepEqual(collectWarnings({ warnings: ['a', 'b'] }), ['a', 'b']);
  assert.deepEqual(
    collectWarnings({
      trees: [{ sourceId: 'api', output: { warnings: ['x'] } }],
      warnings: ['top'],
    }),
    ['[api] x', 'top']
  );
  assert.deepEqual(collectWarnings(null), []);
  assert.deepEqual(collectWarnings({}), []);
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

// --- --severity display filter ------------------------------------------------------------------------

test('formatPretty: minSeverity "warning" hides info entirely (no folded block) and keeps warning/critical', () => {
  const output = {
    fileCount: 3,
    findings: [
      { ruleId: 'b-rule', severity: 'warning', file: 'src/z.ts', line: 10, message: 'zzz' },
      { ruleId: 'a-rule', severity: 'critical', file: 'src/a.ts', line: 5, message: 'boom' },
      { ruleId: 'dead-candidates', severity: 'info', file: 'src/c.ts', line: 1, message: 'note' },
    ],
  };
  const out = formatPretty(output, { color: false, minSeverity: 'warning' });
  assert.match(out, /src\/z\.ts/);
  assert.match(out, /src\/a\.ts/);
  assert.doesNotMatch(out, /src\/c\.ts/);
  assert.doesNotMatch(out, /folded/);
  assert.doesNotMatch(out, /dead-candidates/);
  // footer tallies only the filtered (displayed) findings — info is dropped, not just folded.
  // fileCount is a files-analyzed metric, unaffected by the display filter, so it stays as reported (3).
  assert.match(out, /2 findings in 3 files/);
  assert.match(out, /0 info/);
});

test('formatPretty: minSeverity "critical" shows only critical findings', () => {
  const output = {
    fileCount: 3,
    findings: [
      { ruleId: 'b-rule', severity: 'warning', file: 'src/z.ts', line: 10, message: 'zzz' },
      { ruleId: 'a-rule', severity: 'critical', file: 'src/a.ts', line: 5, message: 'boom' },
      { ruleId: 'dead-candidates', severity: 'info', file: 'src/c.ts', line: 1, message: 'note' },
    ],
  };
  const out = formatPretty(output, { color: false, minSeverity: 'critical' });
  assert.match(out, /src\/a\.ts/);
  assert.doesNotMatch(out, /src\/z\.ts/);
  assert.doesNotMatch(out, /src\/c\.ts/);
  assert.match(out, /1 finding in 3 files/);
});

test('formatPretty: minSeverity omitted / null / "off" is identical to today (regression guard)', () => {
  const base = formatPretty(singleOutput, { color: false });
  assert.equal(formatPretty(singleOutput, { color: false, minSeverity: null }), base);
  assert.equal(formatPretty(singleOutput, { color: false, minSeverity: undefined }), base);
  assert.equal(formatPretty(singleOutput, { color: false, minSeverity: 'off' }), base);
});

test('filterOutputBySeverity: single-tree drops sub-threshold findings, leaves rest of output intact', () => {
  const output = {
    fileCount: 3,
    warnings: ['w'],
    findings: [
      { ruleId: 'a', severity: 'critical', file: 'a.ts', line: 1 },
      { ruleId: 'b', severity: 'warning', file: 'b.ts', line: 2 },
      { ruleId: 'c', severity: 'info', file: 'c.ts', line: 3 },
    ],
  };
  const filtered = filterOutputBySeverity(output, 'warning');
  assert.deepEqual(
    filtered.findings.map((f) => f.ruleId),
    ['a', 'b']
  );
  assert.equal(filtered.fileCount, 3);
  assert.deepEqual(filtered.warnings, ['w']);
});

test('filterOutputBySeverity: multi-tree filters each trees[].output.findings AND top-level crossLayerFindings', () => {
  const output = {
    trees: [
      {
        root: './api',
        sourceId: 'api',
        output: {
          fileCount: 2,
          findings: [
            { ruleId: 'x', severity: 'warning', file: 'api/h.ts', line: 1 },
            { ruleId: 'x2', severity: 'info', file: 'api/i.ts', line: 2 },
          ],
        },
      },
      {
        root: './web',
        sourceId: 'web',
        output: {
          fileCount: 4,
          findings: [{ ruleId: 'y', severity: 'critical', file: 'web/i.ts', line: 9 }],
        },
      },
    ],
    crossLayer: {},
    crossLayerFindings: [
      { ruleId: 'cl-1', severity: 'warning', file: 'x.ts', line: 1 },
      { ruleId: 'cl-2', severity: 'info', file: 'y.ts', line: 2 },
    ],
  };
  const filtered = filterOutputBySeverity(output, 'warning');
  assert.deepEqual(
    filtered.trees[0].output.findings.map((f) => f.ruleId),
    ['x']
  );
  assert.deepEqual(
    filtered.trees[1].output.findings.map((f) => f.ruleId),
    ['y']
  );
  assert.deepEqual(
    filtered.crossLayerFindings.map((f) => f.ruleId),
    ['cl-1']
  );
  // sibling fields untouched
  assert.equal(filtered.trees[0].root, './api');
  assert.equal(filtered.trees[0].output.fileCount, 2);
  assert.deepEqual(filtered.crossLayer, {});
});

test('filterOutputBySeverity: null/undefined/"off"/"info" are no-ops (identical to unfiltered)', () => {
  const output = { fileCount: 1, findings: [{ ruleId: 'a', severity: 'info', file: 'a.ts', line: 1 }] };
  assert.deepEqual(filterOutputBySeverity(output, null), output);
  assert.deepEqual(filterOutputBySeverity(output, undefined), output);
  assert.deepEqual(filterOutputBySeverity(output, 'off'), output);
  assert.deepEqual(filterOutputBySeverity(output, 'info'), output);
});

test('exit code is computed from UNFILTERED findings, not the --severity display filter', () => {
  const output = {
    fileCount: 1,
    findings: [{ ruleId: 'a', severity: 'warning', file: 'a.ts', line: 1 }],
  };
  // Simulate `zzop --severity critical` on a repo that only has warnings: the displayed view is empty...
  const filtered = filterOutputBySeverity(output, 'critical');
  assert.equal(filtered.findings.length, 0);
  // ...but the exit-code path reads collectFindings(output) off the ORIGINAL output, never the filtered one.
  const { findings: unfilteredFindings } = collectFindings(output);
  assert.equal(computeExitCode(unfilteredFindings, 'warn'), 1);
  // Sanity check: computing from the filtered findings would have (wrongly) returned 0 — proving the two
  // paths must read from different sources.
  const { findings: filteredFindings } = collectFindings(filtered);
  assert.equal(computeExitCode(filteredFindings, 'warn'), 0);
});
