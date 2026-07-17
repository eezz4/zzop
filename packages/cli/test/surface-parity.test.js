'use strict';

// JS-side half of the surface-parity contract (see docs/contracts/surface-parity.json's own `_doc` for
// the full rationale: three historical drift incidents where an engine/facade output field existed but a
// delivery surface silently failed to carry it — most concretely, `configWarnings` was computed by the
// engine but this CLI's `lib/format.js` never read the string, so a config typo produced zero terminal
// feedback). The Rust half (crates/engine/tests/rule_contracts/surface_parity.rs) checks the registry's
// completeness against the facade's own pinned output key sets, note discipline, and the MCP lane's
// truthfulness; this file is deliberately a CHEAP, grep-level check over the two JS renderers this
// package owns: `lib/format.js` (the pretty terminal render) and `lib/report.js` (the markdown report
// builder). It is exactly the kind of check that would have caught the `configWarnings` incident — the
// field name string was simply absent from `lib/format.js`.

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const REGISTRY_PATH = path.join(__dirname, '..', '..', '..', 'docs', 'contracts', 'surface-parity.json');
const registry = JSON.parse(fs.readFileSync(REGISTRY_PATH, 'utf8'));

const FORMAT_JS_PATH = path.join(__dirname, '..', 'lib', 'format.js');
const REPORT_JS_PATH = path.join(__dirname, '..', 'lib', 'report.js');
const formatJsSource = fs.readFileSync(FORMAT_JS_PATH, 'utf8');
const reportJsSource = fs.readFileSync(REPORT_JS_PATH, 'utf8');

const KNOWN_STATUSES = new Set(['carry', 'carry-conditional', 'omit']);
const ROOTS = ['analyzeOutputView', 'multiAnalyzeOutputView'];

/**
 * Whether `field` appears in `source` as a whole identifier token (`\bfield\b`), not merely as a bare
 * substring. A bare-substring check would false-positive on a short field name like `ir` matching inside
 * unrelated identifiers/prose that merely CONTAIN those two letters (`circular`, `firstSentence`'s "ir"? —
 * no, but e.g. a doc-comment mention of `CommonIr`, or the word `direction`) — the same false-positive
 * class the Rust-side meta-test's own doc calls out for its `"ir":` JSON-key matcher. Word-boundary
 * matching is the right level of precision for these camelCase JS property-access sites
 * (`treeOutput.ir`, `output.coverage`, ...): `\b` only fires at a transition between a word character and
 * a non-word one, so `ir` inside `circular` (no such transition around those two letters) never matches,
 * while `treeOutput.ir` (a `.` immediately before `ir`, then a `.`/`)`/newline after) does.
 * @param {string} source
 * @param {string} field
 * @returns {boolean}
 */
function sourceMentionsField(source, field) {
  const escaped = field.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  return new RegExp(`\\b${escaped}\\b`).test(source);
}

function flattenRows() {
  const rows = [];
  for (const root of ROOTS) {
    const fields = registry[root];
    for (const [field, row] of Object.entries(fields)) {
      rows.push({ root, field, row });
    }
  }
  return rows;
}

test('surface-parity registry is well-formed', () => {
  assert.equal(typeof registry._doc, 'string');
  assert.ok(registry._doc.trim().length > 0, '_doc must be non-empty');
  for (const root of ROOTS) {
    assert.ok(
      registry[root] && typeof registry[root] === 'object' && !Array.isArray(registry[root]),
      `registry.${root} must be an object`
    );
    assert.ok(
      Object.keys(registry[root]).length > 0,
      `registry.${root} must have at least one field row`
    );
  }
  for (const { root, field, row } of flattenRows()) {
    for (const surface of ['mcpAnalyzeReply', 'jsCliRender', 'mdReport']) {
      assert.ok(
        KNOWN_STATUSES.has(row[surface]),
        `${root}.${field}.${surface} must be one of ${[...KNOWN_STATUSES].join('/')}, got: ${row[surface]}`
      );
    }
    assert.equal(
      typeof row.note,
      'string',
      `${root}.${field}.note must be a string (possibly empty for an all-carry row)`
    );
  }
});

test('every omit/carry-conditional row carries a non-empty note', () => {
  for (const { root, field, row } of flattenRows()) {
    const needsNote = ['mcpAnalyzeReply', 'jsCliRender', 'mdReport'].some(
      (surface) => row[surface] === 'omit' || row[surface] === 'carry-conditional'
    );
    if (!needsNote) continue;
    assert.ok(
      row.note && row.note.trim().length > 0,
      `${root}.${field} has an omit/carry-conditional status but an empty note — every omit/conditional ` +
        'row must explain why, and where the data IS available'
    );
  }
});

test('every field marked carry/carry-conditional for jsCliRender is mentioned in lib/format.js', () => {
  const offenders = [];
  for (const { root, field, row } of flattenRows()) {
    if (row.jsCliRender === 'omit') continue;
    if (!sourceMentionsField(formatJsSource, field)) {
      offenders.push(`${root}.${field} (jsCliRender: ${row.jsCliRender})`);
    }
  }
  assert.deepEqual(
    offenders,
    [],
    `these registry rows claim lib/format.js carries the field, but the field name string is absent from ` +
      `lib/format.js — either the registry is stale or the renderer silently stopped reading the field ` +
      `(the exact defect class this contract exists to catch): ${JSON.stringify(offenders)}`
  );
});

test('every field marked carry/carry-conditional for mdReport is mentioned in lib/report.js', () => {
  const offenders = [];
  for (const { root, field, row } of flattenRows()) {
    if (row.mdReport === 'omit') continue;
    if (!sourceMentionsField(reportJsSource, field)) {
      offenders.push(`${root}.${field} (mdReport: ${row.mdReport})`);
    }
  }
  assert.deepEqual(
    offenders,
    [],
    `these registry rows claim lib/report.js carries the field, but the field name string is absent from ` +
      `lib/report.js — either the registry is stale or the report builder silently stopped reading the ` +
      `field (the exact defect class this contract exists to catch): ${JSON.stringify(offenders)}`
  );
});
