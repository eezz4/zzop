#!/usr/bin/env bash
# check-catalog-severity-sync -- every severity `docs/rules/catalog.md` publishes for a DSL rule must be
# the severity that rule actually ships with in `rules/dsl/<pack>/<pack>.json`, and every shipped DSL
# rule must have a row there.
#
# ## The defect this exists for (2026-09-03, batch U160)
#
# 27 rules were moved `warning` -> `info` in the packs. The catalog kept saying `warning` for all 27, the
# site inherited the same 27 wrong cells from it, and EVERY guard stayed green -- 40 of them, plus the
# whole workspace test suite. Nothing in this repo tied a published severity back to the shipped one.
#
# What DID exist reads as if it covers this and does not:
#   - check-rules-catalog-sync.sh compares site/rules.html to the CATALOG. Both wrong together is clean.
#   - check-docs-rule-ids.sh builds its id set FROM the catalog, so the catalog is its authority.
#   - crates/engine/tests/rule_contracts/ pins catalog TOTALS (pack count, rule count), not cells.
# A severity is the one thing in that table a user acts on -- it decides whether `--fail-on warning`
# breaks their build -- and it was the one column with no owner.
#
# ## Why there is NO "skip a row I could not parse" branch
#
# The first version of this guard matched rows as `| `id` | <bare level> |` and silently skipped
# anything else, failing only if fewer than 100 rows were compared -- 18 rows of slack against 118
# rules. An external reviewer defeated it the day it landed: `scripts/gen-site-rules.mjs`'s
# `severityCell` deliberately accepts a level PLUS a hand-written qualifier (`warning (info when the
# two sites straddle a deployment manifest)`), so writing a DSL cell that way made the row invisible
# here while the site regenerated happily from it. Both surfaces then published a severity the engine
# does not ship, which is precisely the defect above.
#
# So the accounting is now TOTAL, in both directions:
#   - every line under a bundled-pack heading that begins `| `` is a row this guard MUST parse; one it
#     cannot parse is an error naming the line, never a skip;
#   - the set of ids it compared must EQUAL the set of ids the packs ship -- a deleted row cannot hide,
#     which the row-count floor also could not see.
# The level is compared; a trailing qualifier is prose and is not.
#
# ## What is out of scope, and why that is not a hole
#
# Native-analysis rows: their severities are registered in Rust, not in a pack file. Exported packs
# (`examples/packs/`): not loaded by a default run, and the catalog says so under its own heading --
# both are recognised by their heading shape, and a heading whose shape this guard does not recognise
# ends the bundled section rather than being read as part of it.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

command -v node > /dev/null || { echo "check-catalog-severity-sync: node is required."; exit 1; }

node - <<'NODE'
const fs = require('fs');
const path = require('path');

const DSL = 'rules/dsl';
const shipped = new Map();
for (const dir of fs.readdirSync(DSL, { withFileTypes: true })) {
  if (!dir.isDirectory()) continue;
  const p = path.join(DSL, dir.name, `${dir.name}.json`);
  if (!fs.existsSync(p)) continue;
  const pack = JSON.parse(fs.readFileSync(p, 'utf8'));
  for (const r of pack.rules) shipped.set(`${pack.id}/${r.id}`, r.severity);
}
if (shipped.size < 100) {
  console.error(`check-catalog-severity-sync: only ${shipped.size} shipped rule(s) found under ${DSL}/.`);
  console.error('  Every comparison below would pass vacuously, so a small number here is a broken scan,');
  console.error('  never a smaller product.');
  process.exit(1);
}

const CATALOG = 'docs/rules/catalog.md';
const lines = fs.readFileSync(CATALOG, 'utf8').split(/\r?\n/);
// A BUNDLED pack section: `### `pack` (N rules)` and nothing after the paren. The exported packs are
// spelled `### `pack` (N rules) — gloss`, so they end the bundled section instead of joining it.
const BUNDLED_HEADING = /^### `([a-z-]+)` \(\d+ rules?\)$/;
// A rule row: id, then a severity cell that is a bare level optionally followed by prose. The level
// grammar is `gen-site-rules.mjs`'s `severityCell` grammar, deliberately -- if the generator accepts
// a spelling, this guard has to read it.
const ROW = /^\| `([a-z0-9-]+)` \| ([a-z][a-z/]*)((?:\s[^|]*)?)\| /;

let pack = null;
const drift = [];
const unknown = [];
const unparsed = [];
const seen = new Set();
for (let i = 0; i < lines.length; i++) {
  const line = lines[i];
  const head = line.match(BUNDLED_HEADING);
  if (head) { pack = head[1]; continue; }
  if (line.startsWith('#')) { pack = null; continue; }
  if (!pack) continue;
  if (!line.startsWith('| `')) continue;            // header/separator/prose rows are not rule rows
  const row = line.match(ROW);
  if (!row) { unparsed.push(`${CATALOG}:${i + 1}  ${line.slice(0, 110)}`); continue; }
  const key = `${pack}/${row[1]}`;
  const want = shipped.get(key);
  if (want === undefined) { unknown.push(`${key} (${CATALOG}:${i + 1})`); continue; }
  seen.add(key);
  if (want !== row[2]) drift.push(`${key.padEnd(46)} catalog says ${row[2].padEnd(8)} pack ships ${want}`);
}

// A SECOND invariant, added 2026-09-05 for a defect this file was already standing next to: one
// SPELLING may not sit in two of the catalog's id spaces. `circular` sat in both the native-analysis
// table (`warning`, keys findings) and the recommendation-id table (`critical`, keys none), and the
// row that disambiguated them did it with the word "here" -- a section-scoped pronoun that does not
// survive the grep or the excerpt anyone actually reads. Two independent labelers reading only
// shipped text landed on the wrong row, and so did the engineer who received their agreeing answers:
// three readers, one line, no guard. Severity drift was this file's first subject; a collision is the
// same failure one column over -- a cell that is correct and still reads as false.
//
// Sections, not tables, are the unit: the eight bundled DSL packs each get a heading and share ONE id
// space (a finding spells them `<pack>/<rule>`), so a repeat across two pack headings is not a
// collision. Every other heading here IS its own space. That is why this reads headings rather than
// re-deriving spaces from the registry -- the catalog's own sectioning is what a reader navigates by,
// and it is the thing that failed.
const SECTION = /^#{2,4}\s+(.*)$/;
const ANY_ROW = /^\|\s*`([^`]+)`\s*\|/;
const PACK_SPACE = 'DSL packs (one id space)';
const spaceOf = new Map();
{
  let section = '(top)';
  for (const line of lines) {
    const h = line.match(SECTION);
    if (h) {
      section = BUNDLED_HEADING.test(line) ? PACK_SPACE : h[1].replace(/\s*\(\d+ rules?\)/, '');
      continue;
    }
    const r = line.match(ANY_ROW);
    if (!r) continue;
    if (!spaceOf.has(r[1])) spaceOf.set(r[1], new Map());
    spaceOf.get(r[1]).set(section, line);
  }
}
if (spaceOf.size < 100) {
  console.error(`check-catalog-severity-sync: only ${spaceOf.size} catalog id(s) seen -- the collision`);
  console.error('  scan would pass vacuously. A small number here is a broken scan, never a smaller catalog.');
  process.exit(1);
}
// A collision is not itself the defect -- two spaces MAY legitimately want the same word. The defect
// is a colliding row a reader cannot place without its heading. So the test is on the ROW: each one
// must carry the literal token `id space`, which is this guard's way of making the author write the
// disambiguation down instead of inheriting it from the section. One token, no per-section vocabulary
// table (that list would be the hand-maintained thing this repo keeps getting bitten by), and it fails
// loud the moment a third space adopts a spelling.
const MARKER = 'id space';
const unscoped = [];
for (const [id, rows] of spaceOf) {
  if (rows.size < 2) continue;
  for (const [section, text] of rows) {
    if (!text.includes(MARKER)) unscoped.push({ id, section });
  }
}
if (unscoped.length) {
  console.error('check-catalog-severity-sync: a spelling sits in two catalog id spaces, and a row does');
  console.error(`  not say which one it is in (looking for the literal token "${MARKER}"):`);
  for (const u of unscoped) console.error(`  ${u.id} -- ${u.section}`);
  console.error('');
  console.error('  A reader who greps this file gets one row and no heading, so the two become one id in');
  console.error('  their head -- measured: three independent readers, one line. Either rename, or make');
  console.error('  EVERY colliding row name its own space IN THE ROW, never with a word like "here"');
  console.error('  that needs the heading to mean anything.');
  process.exit(1);
}


const missing = [...shipped.keys()].filter((k) => !seen.has(k));
if (!drift.length && !unknown.length && !unparsed.length && !missing.length) {
  console.log(`check-catalog-severity-sync: OK (${seen.size} rule(s); every shipped DSL rule has a row and every row's level matches).`);
  process.exit(0);
}
if (unparsed.length) {
  console.error('check-catalog-severity-sync: rows under a bundled-pack heading this guard could not read:');
  for (const u of unparsed) console.error(`  ${u}`);
  console.error('  A row it cannot read is a row it cannot check. Reshape the row, or teach this guard the');
  console.error('  new shape -- leaving it unreadable is how 27 wrong cells shipped green once already.');
}
if (unknown.length) {
  console.error('check-catalog-severity-sync: catalog rows name ids no pack ships:');
  for (const u of unknown) console.error(`  ${u}`);
}
if (missing.length) {
  console.error(`check-catalog-severity-sync: ${missing.length} shipped DSL rule(s) have NO row in ${CATALOG}:`);
  for (const m of missing) console.error(`  ${m}`);
  console.error('  A rule with no row is a rule a user cannot look up, and its severity is unpublished.');
}
if (drift.length) {
  console.error(`check-catalog-severity-sync: ${CATALOG} publishes a severity the engine does not ship:`);
  for (const d of drift) console.error(`  ${d}`);
  console.error('');
  console.error('  The PACK owns this value -- it is what the run reports and what `--fail-on` reads.');
  console.error('  Fix the catalog to what the pack ships, then regenerate the site:');
  console.error('    node scripts/gen-site-rules.mjs && node scripts/gen-site.mjs');
}
process.exit(1);
NODE
