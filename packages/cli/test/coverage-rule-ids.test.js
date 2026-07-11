'use strict';

// Guards that packages/cli/lib/report.js's COVERAGE_RULE_IDS set — the rule ids report.js routes into
// the "Coverage & blindness" section instead of the generic cross-layer findings bucket — stays a
// subset of the rule ids docs/rules/catalog.md actually documents. report.js is read as SOURCE TEXT
// (not imported) so this test cannot itself cause the section-routing bug it's guarding: a parallel
// agent owns report.js, and importing it here would couple this guard to its module shape instead of
// just its literal contract with the catalog.
//
// Failure means a cross-layer rule id was renamed or removed in the catalog but report.js still
// references the old id — section routing for coverage findings would silently break (the finding
// would fall through to the generic bucket, or match nothing at all).

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const reportSource = fs.readFileSync(path.join(__dirname, '../lib/report.js'), 'utf8');
const catalogSource = fs.readFileSync(path.join(__dirname, '../../../docs/rules/catalog.md'), 'utf8');

function extractCoverageRuleIds(source) {
  const match = source.match(/COVERAGE_RULE_IDS\s*=\s*new Set\(\[([\s\S]*?)\]\)/);
  assert.ok(match, 'expected to find `const COVERAGE_RULE_IDS = new Set([...])` in report.js source');
  const body = match[1];
  const ids = [];
  const idPattern = /'([^']+)'|"([^"]+)"/g;
  let m;
  while ((m = idPattern.exec(body)) !== null) {
    ids.push(m[1] !== undefined ? m[1] : m[2]);
  }
  return ids;
}

function extractCatalogRuleIds(source) {
  return new Set(source.match(/cross-layer\/[a-z0-9-]+/g) || []);
}

test('COVERAGE_RULE_IDS in report.js is nonempty and every entry is a cross-layer id', () => {
  const ids = extractCoverageRuleIds(reportSource);
  assert.ok(ids.length > 0, 'COVERAGE_RULE_IDS parsed as empty — extraction regex likely stale');
  for (const id of ids) {
    assert.match(id, /^cross-layer\//, `COVERAGE_RULE_IDS entry '${id}' is not a cross-layer/ rule id`);
  }
});

test('every COVERAGE_RULE_IDS entry in report.js is present in docs/rules/catalog.md', () => {
  const coverageIds = extractCoverageRuleIds(reportSource);
  const catalogIds = extractCatalogRuleIds(catalogSource);
  for (const id of coverageIds) {
    assert.ok(
      catalogIds.has(id),
      `report.js references a cross-layer rule id absent from catalog.md (renamed?) — section routing for coverage findings will silently break: '${id}'`
    );
  }
});
