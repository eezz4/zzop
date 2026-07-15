'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

const {
  collectFindings,
  collectWarnings,
  groupByFile,
  countBySeverity,
  splitMessage,
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

test('collectFindings: multi-tree appends crossLayerFindings, tagged crossLayer (not sourceId/root)', () => {
  const output = {
    trees: [
      {
        root: './api',
        sourceId: 'api',
        output: { fileCount: 2, findings: [{ ruleId: 'x', severity: 'warning', file: 'api/h.ts', line: 1 }] },
      },
    ],
    crossLayer: {},
    crossLayerFindings: [
      { ruleId: 'cross-layer/duplicate-route', severity: 'warning', file: 'a.ts', line: 3, message: 'dup' },
      { ruleId: 'cross-layer/unconsumed-endpoint', severity: 'info', file: 'b.ts', line: 9, message: 'dead' },
    ],
  };
  const { findings, fileCount } = collectFindings(output);
  assert.equal(findings.length, 3);
  assert.equal(fileCount, 2);
  const crossLayer = findings.filter((f) => f.ruleId.startsWith('cross-layer/'));
  assert.equal(crossLayer.length, 2);
  for (const f of crossLayer) {
    assert.equal(f.crossLayer, true);
    assert.ok(!('sourceId' in f));
    assert.ok(!('root' in f));
  }
  const perTree = findings.find((f) => f.ruleId === 'x');
  assert.equal(perTree.sourceId, 'api');
  assert.ok(!perTree.crossLayer);
});

test('collectFindings: multi-tree output with no crossLayerFindings field is unaffected', () => {
  const { findings } = collectFindings(treesOutput);
  assert.equal(findings.length, 2);
  assert.ok(findings.every((f) => !f.crossLayer));
});

test('collectFindings: single-tree shape never picks up crossLayerFindings (no such field exists there)', () => {
  const { findings } = collectFindings({ ...singleOutput, crossLayerFindings: [{ ruleId: 'bogus' }] });
  assert.equal(findings.length, 3);
  assert.ok(findings.every((f) => !f.crossLayer));
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

test('formatPretty: multi-tree crossLayerFindings render in their own labeled section, not mixed into per-file groups', () => {
  const output = {
    trees: [
      {
        root: './api',
        sourceId: 'api',
        output: { fileCount: 1, findings: [{ ruleId: 'x', severity: 'warning', file: 'shared.ts', line: 1, message: 'per-tree' }] },
      },
    ],
    crossLayer: {},
    crossLayerFindings: [
      { ruleId: 'cross-layer/duplicate-route', severity: 'warning', file: 'shared.ts', line: 3, message: 'dup route' },
    ],
  };
  const out = formatPretty(output, { color: false });
  assert.match(out, /per-tree/);
  assert.match(out, /Cross-layer findings:/);
  assert.match(out, /dup route/);
  // The cross-layer section comes after the per-file groups.
  assert.ok(out.indexOf('per-tree') < out.indexOf('Cross-layer findings:'));
  assert.match(out, /2 findings in 1 file/);
});

test('formatPretty: info-tier crossLayerFindings fold into the same info block as per-tree info findings', () => {
  const output = {
    trees: [{ root: './api', sourceId: 'api', output: { fileCount: 1, findings: [] } }],
    crossLayer: {},
    crossLayerFindings: [
      { ruleId: 'cross-layer/unconsumed-endpoint', severity: 'info', file: 'a.ts', line: 1, message: 'dead route' },
    ],
  };
  const out = formatPretty(output, { color: false });
  assert.match(out, /1 finding folded/);
  assert.doesNotMatch(out, /Cross-layer findings:/);
  assert.doesNotMatch(out, /dead route/);
});

test('formatPretty: no crossLayerFindings section when the field is absent/empty', () => {
  const out = formatPretty(treesOutput, { color: false });
  assert.doesNotMatch(out, /Cross-layer findings:/);
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

test('exit code integration: a warning-tier crossLayerFinding gates the default failOn:warn', () => {
  const output = {
    trees: [{ root: './api', sourceId: 'api', output: { fileCount: 1, findings: [] } }],
    crossLayer: {},
    crossLayerFindings: [
      { ruleId: 'cross-layer/duplicate-route', severity: 'warning', file: 'a.ts', line: 1, message: 'dup' },
    ],
  };
  const { findings } = collectFindings(output);
  assert.equal(computeExitCode(findings, 'warn'), 1);
});

test('exit code integration: an info-tier crossLayerFinding does NOT gate the default failOn:warn', () => {
  const output = {
    trees: [{ root: './api', sourceId: 'api', output: { fileCount: 1, findings: [] } }],
    crossLayer: {},
    crossLayerFindings: [
      { ruleId: 'cross-layer/unconsumed-endpoint', severity: 'info', file: 'a.ts', line: 1, message: 'dead' },
    ],
  };
  const { findings } = collectFindings(output);
  assert.equal(computeExitCode(findings, 'warn'), 0);
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

// ---------------------------------------------------------------------------------------------------
// splitMessage + the terminal one-line-headline fold (the "2-tier message" render).
// ---------------------------------------------------------------------------------------------------

test('splitMessage splits a mini-doc message at its first sentence', () => {
  const { headline, detail } = splitMessage(
    'endpoint `GET /foo` is not called by any source in this analysis. This may be dead code. Disable via config.'
  );
  assert.equal(headline, 'endpoint `GET /foo` is not called by any source in this analysis.');
  assert.equal(detail, 'This may be dead code. Disable via config.');
});

test('splitMessage does not split on a dotted token (no space after the period)', () => {
  const { headline, detail } = splitMessage('call `axios.get(url)` uses a dynamic URL and is unresolved');
  assert.equal(headline, 'call `axios.get(url)` uses a dynamic URL and is unresolved');
  assert.equal(detail, '');
});

test('splitMessage skips e.g./i.e. abbreviations when finding the sentence end', () => {
  const { headline, detail } = splitMessage(
    'this route is provider-only, e.g. a webhook or health probe. Confirm before removing.'
  );
  assert.equal(headline, 'this route is provider-only, e.g. a webhook or health probe.');
  assert.equal(detail, 'Confirm before removing.');
});

test('splitMessage returns no detail for a short single-sentence message', () => {
  const { headline, detail } = splitMessage('duplicate route `POST /x` across two trees');
  assert.equal(headline, 'duplicate route `POST /x` across two trees');
  assert.equal(detail, '');
});

test('splitMessage soft-caps a long period-less message on a word boundary', () => {
  const long = 'word '.repeat(80).trim(); // 400 chars, no periods
  const { headline, detail } = splitMessage(long);
  // format.js's internal HEADLINE_SOFT_CAP is 200; headline is that plus at most a trailing ellipsis.
  assert.ok(headline.length <= 201, 'headline stays within the soft cap (+ellipsis)');
  assert.ok(headline.endsWith('…'));
  assert.ok(detail.length > 0);
});

test('formatPretty folds a finding message to its headline by default, with a trim notice', () => {
  const output = {
    fileCount: 1,
    findings: [
      {
        ruleId: 'r',
        severity: 'warning',
        file: 'a.ts',
        line: 3,
        message: 'headline sentence here. hidden detail follows with the fix.',
      },
    ],
  };
  const out = formatPretty(output, { color: false });
  assert.ok(out.includes('headline sentence here.'), 'headline shown');
  assert.ok(!out.includes('hidden detail follows'), 'detail hidden by default');
  assert.ok(out.includes('pass --all for full guidance'), 'trim notice shown');
});

test('formatPretty showAllInfo prints the full finding message and no trim notice', () => {
  const output = {
    fileCount: 1,
    findings: [
      {
        ruleId: 'r',
        severity: 'warning',
        file: 'a.ts',
        line: 3,
        message: 'headline sentence here. hidden detail follows with the fix.',
      },
    ],
  };
  const out = formatPretty(output, { color: false, showAllInfo: true });
  assert.ok(out.includes('hidden detail follows with the fix.'), 'full message shown under --all');
  assert.ok(!out.includes('pass --all for full guidance'), 'no trim notice when already expanded');
});

test('formatPretty shows no trim notice when no message has foldable detail', () => {
  const output = {
    fileCount: 1,
    findings: [{ ruleId: 'r', severity: 'warning', file: 'a.ts', line: 1, message: 'short single clause' }],
  };
  const out = formatPretty(output, { color: false });
  assert.ok(!out.includes('pass --all for full guidance'));
});
