#!/usr/bin/env bash
# check-fragment-fixture-pairing — a rule's `message` and `severity` must read the same in the shipped
# pack and in that pack's pre-migration fixture. Those fixtures are byte-identity snapshots, so a pack
# edit that does not reach them is a defect; and the copy that does not get updated is user-facing text.
#
# ## The defect this exists for -- three occurrences, and the convention never stopped one
#
# `crates/core/src/dsl/tests_fixtures/{http,redis}_pre_migration.json` prove that fragment expansion
# does not change meaning, so an INTENDED message change in `rules/dsl/<pack>/<pack>.json` has to be
# mirrored into them or `byte_identity::*_pack_debug_output_is_unchanged_by_the_fragment_migration`
# goes red. That convention has been written down since 2026-07-26 and was violated three times:
#
#   1. 2026-07-26 -- a redis ordering gate put the matcher field in the fixture and forgot the message.
#   2. 2026-07-26 -- an http Mode A prescription correction never reached the fixture at all.
#   3. 2026-08-27 -- `e6e73f9` removed a DANGEROUS prescription from `redis/lock-no-ttl`, the fixture
#      kept it, and HEAD stayed red for two days. Two things made that one worse than a broken test.
#      The removed sentence survived in the fixture as a THIRD copy, and the batch that deleted it from
#      the public docs grepped `docs/` and the site only, so nothing pointed here. And the commit
#      passed, because `.githooks/pre-commit` runs guards and no tests: a message edit breaks no guard,
#      and the test that would catch it is not on the commit path.
#
# The convention lived in a comment. This is the machine half, the way `check-vendor-token-literals.sh`
# is the machine half of the secret-fixture convention next to it in that same document.
#
# ## Why it compares text and not a hash
#
# The recorded prescription was to pin the source pack's HASH beside the fixture. A hash pin says "the
# source moved" -- which is true whenever any field moves, including the matcher fields that are
# SUPPOSED to differ here (the pack spells `${NAME}` fragment references, the fixture spells the
# expanded regex). It would fire on edits that are correct, and a pin that cries wolf gets bumped
# rather than obeyed -- and a bumped hash is a defeated guard that still looks armed.
#
# Messages and severities are copied VERBATIM in both directions -- measured: 6 of 6 redis rules and
# every http rule agree byte for byte today -- so they can be compared directly. A direct comparison
# names the rule and the field, cannot be satisfied by editing a number, and is exactly the half that
# went wrong all three times ("the matcher was remembered and the message was forgotten").
#
# ## What is judged
#
# For every `*_pre_migration.json` (that directory listing is the roster -- there is no list here to
# fall out of date): the rule ID SETS must match, and for every shared id the `message` and `severity`
# must be byte-identical. Matcher fields are deliberately NOT compared; proving those equivalent after
# expansion is the Rust byte-identity test's job, and this guard exists because that test is not on the
# commit path.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

command -v node > /dev/null || { echo "check-fragment-fixture-pairing: node is required."; exit 1; }

node - <<'NODE'
const fs = require('fs');
const path = require('path');

const FIXDIR = 'crates/core/src/dsl/tests_fixtures';
const fixtures = fs.readdirSync(FIXDIR).filter((f) => f.endsWith('_pre_migration.json')).sort();

// A vacuously green guard is the failure this one exists to prevent, so the roster has a floor.
if (fixtures.length === 0) {
  console.error(`check-fragment-fixture-pairing: found NO *_pre_migration.json under ${FIXDIR}.`);
  console.error('  Either the fixtures moved or this glob stopped matching. Both mean this guard is');
  console.error('  checking nothing, which is worse than the defect it was written for.');
  process.exit(1);
}

const problems = [];
let comparedRules = 0;

for (const fixture of fixtures) {
  const pack = fixture.replace(/_pre_migration\.json$/, '');
  const packPath = path.posix.join('rules/dsl', pack, `${pack}.json`);
  if (!fs.existsSync(packPath)) {
    problems.push(`${fixture}: no shipped pack at ${packPath} -- the fixture outlived its pack, or the naming convention changed.`);
    continue;
  }
  const src = JSON.parse(fs.readFileSync(packPath, 'utf8'));
  const fix = JSON.parse(fs.readFileSync(path.posix.join(FIXDIR, fixture), 'utf8'));
  const byId = (p) => new Map((p.rules ?? []).map((r) => [r.id, r]));
  const S = byId(src), F = byId(fix);

  for (const id of [...S.keys()].filter((id) => !F.has(id)).sort())
    problems.push(`${packPath} has rule "${id}" and ${fixture} does not.`);
  for (const id of [...F.keys()].filter((id) => !S.has(id)).sort())
    problems.push(`${fixture} has rule "${id}" and ${packPath} does not -- a rule deleted from the pack is still carried here.`);

  for (const id of [...S.keys()].filter((id) => F.has(id)).sort()) {
    comparedRules++;
    for (const field of ['message', 'severity']) {
      const a = S.get(id)[field], b = F.get(id)[field];
      if (a === b) continue;
      problems.push(
        `${pack}/${id}: \`${field}\` differs between the pack and its fixture.\n` +
        `      pack    (${packPath}): ${JSON.stringify(a).slice(0, 160)}\n` +
        `      fixture (${FIXDIR}/${fixture}): ${JSON.stringify(b).slice(0, 160)}`
      );
    }
  }
}

if (problems.length === 0) {
  // COMPARED-QUANTITY FLOOR (2026-09-13, review ledger V186). The roster floor above proves fixtures
  // EXIST; it does not prove any rule was compared. Fixtures whose `rules` array is empty satisfy the
  // roster and compare nothing, and this line would print "0 rules compared" as an OK.
  if (comparedRules === 0) {
    console.error('check-fragment-fixture-pairing: fixtures exist but ZERO rules were compared.');
    console.error('  Every fixture pairs with a pack that shares no rule id, or the arrays are empty.');
    console.error('  Existing is not the same as being checked, and only one of those was asserted.');
    process.exit(1);
  }
  console.log(`check-fragment-fixture-pairing: OK (${fixtures.length} fixture(s), ${comparedRules} rules compared on message + severity).`);
  process.exit(0);
}

console.error('check-fragment-fixture-pairing: a pack and its pre-migration fixture disagree.');
console.error('');
for (const p of problems) console.error(`  - ${p}`);
console.error('');
console.error('  These fixtures are byte-identity snapshots: an INTENDED message change in the pack has');
console.error('  to be mirrored into the fixture, or the Rust byte-identity test goes red -- and that');
console.error('  test is not on the commit path, which is how a two-day red HEAD happened in August.');
console.error('  Copy the new text into the fixture. If you REMOVED a prescription from the pack, note');
console.error('  that the fixture is a second copy of it: deleting it in one place is not deleting it.');
process.exit(1);
NODE
