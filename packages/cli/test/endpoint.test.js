'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const { parseArgs } = require('../bin/zzop.js');
const { ConfigError } = require('../lib/mapper');
const { renderEndpointReport } = require('../lib/endpoint');

const BIN = path.join(__dirname, '..', 'bin', 'zzop.js');

// A fake `queryIo` result in the shared facade query core's pinned shape — the renderer is pure,
// so no native call is needed (same mocking stance as the other renderer tests here).
function fakeQueryResult(overrides) {
  return Object.assign(
    {
      pattern: 'users',
      verdict: 'linked',
      counts: {
        edges: 1,
        unconsumedProvides: 0,
        unprovidedConsumes: 0,
        unresolvedConsumes: 0,
        externalConsumes: 0,
        ambiguousConsumes: 0,
      },
      matches: {
        edges: [
          {
            kind: 'http',
            key: 'GET /api/users',
            from: { source: 'web', file: 'src/api.ts', line: 3 },
            to: { source: 'api', file: 'src/users.controller.ts', line: 7 },
            crossSource: true,
          },
        ],
        unconsumedProvides: [],
        unprovidedConsumes: [],
        unresolvedConsumes: [],
        externalConsumes: [],
        ambiguousConsumes: [],
      },
      relatedFindings: [],
      disclosure: [],
    },
    overrides
  );
}

// --- Renderer -----------------------------------------------------------------------------------

test('renders the verdict headline and edge lines as file:line (source)', () => {
  const out = renderEndpointReport(fakeQueryResult({}));
  assert.match(out, /^endpoint "users": linked/);
  assert.match(out, /edges \(1 matched\):/);
  assert.match(out, /^ {2}src\/api\.ts:3 \(web\) GET \/api\/users -> src\/users\.controller\.ts:7 \(api\)$/m);
  // Empty buckets render no section at all.
  assert.doesNotMatch(out, /unprovidedConsumes/);
});

test('renders a tagged bucket line with the raw fallback for an unresolved consume', () => {
  const out = renderEndpointReport(
    fakeQueryResult({
      verdict: 'unresolved-only',
      counts: { edges: 0, unconsumedProvides: 0, unprovidedConsumes: 0, unresolvedConsumes: 1, externalConsumes: 0, ambiguousConsumes: 0 },
      matches: {
        edges: [], unconsumedProvides: [], unprovidedConsumes: [], externalConsumes: [], ambiguousConsumes: [],
        unresolvedConsumes: [
          { source: 'web', kind: 'http', key: null, raw: 'buildUrl(base)', method: 'GET', file: 'src/x.ts', line: 12 },
        ],
      },
    })
  );
  assert.match(out, /unresolvedConsumes \(1 matched\):/);
  assert.match(out, /^ {2}src\/x\.ts:12 \(web\) buildUrl\(base\) \[GET\]$/m);
});

test('discloses a capped bucket and renders related findings', () => {
  const out = renderEndpointReport(
    fakeQueryResult({
      counts: { edges: 25, unconsumedProvides: 0, unprovidedConsumes: 0, unresolvedConsumes: 0, externalConsumes: 0, ambiguousConsumes: 0 },
      truncated: { edges: 5 },
      relatedFindings: [
        { ruleId: 'cross-layer/route-near-miss', severity: 'info', file: 'a.ts', line: 4, message: 'near miss' },
      ],
    })
  );
  assert.match(out, /edges \(25 matched\):/);
  assert.match(out, /\.\.\. 5 more matched/);
  assert.match(out, /related findings \(1\):/);
  assert.match(out, /a\.ts:4 \[cross-layer\/route-near-miss\] near miss/);
});

test('renders ? (never "null") for a related finding without a line', () => {
  const out = renderEndpointReport(
    fakeQueryResult({
      relatedFindings: [
        { ruleId: 'cross-layer/route-drift', severity: 'warning', file: 'b.ts', line: null, message: 'drift' },
      ],
    })
  );
  assert.match(out, /^ {2}b\.ts:\? \[cross-layer\/route-drift\] drift$/m);
  assert.doesNotMatch(out, /null/);
});

test('not-found renders suggestions (or says none were found)', () => {
  const base = {
    verdict: 'not-found',
    counts: { edges: 0, unconsumedProvides: 0, unprovidedConsumes: 0, unresolvedConsumes: 0, externalConsumes: 0, ambiguousConsumes: 0 },
    matches: { edges: [], unconsumedProvides: [], unprovidedConsumes: [], unresolvedConsumes: [], externalConsumes: [], ambiguousConsumes: [] },
  };
  const withSuggestions = renderEndpointReport(
    fakeQueryResult(Object.assign({ suggestions: ['GET /api/v2/users'] }, base))
  );
  assert.match(withSuggestions, /did you mean:/);
  assert.match(withSuggestions, /GET \/api\/v2\/users/);

  const without = renderEndpointReport(fakeQueryResult(Object.assign({ suggestions: [] }, base)));
  assert.match(without, /no similar keys found/);
});

test('renderer is deterministic: same input, same output', () => {
  const result = fakeQueryResult({});
  assert.equal(renderEndpointReport(result), renderEndpointReport(result));
});

// --- Arg parsing --------------------------------------------------------------------------------

test('endpoint parses its pattern positional', () => {
  const opts = parseArgs(['endpoint', 'users']);
  assert.equal(opts.command, 'endpoint');
  assert.equal(opts.pattern, 'users');
});

test('endpoint requires a pattern and rejects a trailing extra argument', () => {
  assert.throws(
    () => parseArgs(['endpoint']),
    (e) => e instanceof ConfigError && /requires a <pattern> argument/.test(e.message)
  );
  assert.throws(
    () => parseArgs(['endpoint', 'users', 'extra']),
    (e) => e instanceof ConfigError && /Unexpected argument "extra"/.test(e.message)
  );
});

test('endpoint accepts --config and --json; run still accepts them too', () => {
  const opts = parseArgs(['endpoint', 'users', '--config', 'z.jsonc', '--json']);
  assert.equal(opts.config, 'z.jsonc');
  assert.equal(opts.format, 'json');
  assert.equal(parseArgs(['run', '--json']).format, 'json');
  assert.equal(parseArgs(['--config', 'z.jsonc']).config, 'z.jsonc');
});

test('endpoint rejects flags scoped to other commands', () => {
  for (const flags of [['--force'], ['--all'], ['--out', 'x'], ['--severity', 'info'], ['--debug-io'], ['--format', 'json']]) {
    assert.throws(
      () => parseArgs(['endpoint', 'users', ...flags]),
      (e) => e instanceof ConfigError && /not valid for the `endpoint` command/.test(e.message),
      `expected rejection for ${flags[0]}`
    );
  }
});

test('endpoint-shared flags stay rejected on init', () => {
  assert.throws(
    () => parseArgs(['init', '--config', 'x']),
    (e) => e instanceof ConfigError && /not valid for the `init`/.test(e.message)
  );
});

test('--help bypasses endpoint pattern validation', () => {
  const opts = parseArgs(['endpoint', '--help']);
  assert.equal(opts.help, true);
  assert.equal(opts.command, 'endpoint');
});

// --- End-to-end: focused help ------------------------------------------------------------------

test('zzop endpoint --help prints the endpoint usage entry and exits 0', () => {
  const res = spawnSync(process.execPath, [BIN, 'endpoint', '--help'], { encoding: 'utf8' });
  assert.equal(res.status, 0);
  assert.match(res.stdout, /zzop endpoint <pattern>/);
  assert.match(res.stdout, /provided-only \| consumed-unprovided/);
  assert.doesNotMatch(res.stdout, /Options \(run\):/);
});

test('zzop endpoint without a pattern exits 2 with the named error', () => {
  const res = spawnSync(process.execPath, [BIN, 'endpoint'], { encoding: 'utf8' });
  assert.equal(res.status, 2);
  assert.match(res.stderr, /requires a <pattern> argument/);
});
