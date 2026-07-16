'use strict';

// JS<->Rust config-front-end parity harness (JS side) — replays every case of the committed
// docs/contracts/config-parity.fixture.json through mapper.js's configToRequest and deep-equals the
// emitted {method, request} against the fixture's expected JSON. The Rust side
// (crates/config/tests/config_parity.rs) asserts zzop_config's mapper against the SAME file (after
// reversing its two documented deltas — config-dir path resolution and the withDefaults fold-in;
// see the fixture's _docs.normalization), so the two front-ends can only drift by failing one of
// the two tests. Same committed-fixture pattern as docs/adapters/key-normalization.fixture.json.
//
// The expected shapes ARE this mapper's raw output (paths literal, no packDefs/git defaults — those
// belong to @zzop/native's withDefaults layer, applied after this mapper), so no normalization is
// needed on this side: a plain deep-equal per case.

const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const path = require('node:path');

const { configToRequest } = require('../lib/mapper');

const FIXTURE_PATH = path.join(
  __dirname,
  '..',
  '..',
  '..',
  'docs',
  'contracts',
  'config-parity.fixture.json'
);
const fixture = JSON.parse(fs.readFileSync(FIXTURE_PATH, 'utf8'));

test('config-parity fixture is well-formed and non-empty', () => {
  assert.ok(Array.isArray(fixture.cases) && fixture.cases.length > 0);
  for (const c of fixture.cases) {
    assert.equal(typeof c.name, 'string');
    assert.ok(c.config && typeof c.config === 'object');
    assert.ok(['analyze', 'analyzeTrees'].includes(c.expected.method));
    assert.ok(c.expected.request && typeof c.expected.request === 'object');
  }
});

test('JS mapper emits the committed request JSON for every fixture case', async (t) => {
  for (const c of fixture.cases) {
    await t.test(c.name, () => {
      const { method, request } = configToRequest(structuredClone(c.config));
      assert.deepStrictEqual(
        { method, request },
        c.expected,
        `case ${JSON.stringify(c.name)}: the JS mapper's request JSON drifted from the committed ` +
          'parity fixture (docs/contracts/config-parity.fixture.json) — if the change is ' +
          'intentional, update the fixture AND confirm crates/config/tests/config_parity.rs still ' +
          'passes (the Rust mapper must emit the same shape)'
      );
    });
  }
});
