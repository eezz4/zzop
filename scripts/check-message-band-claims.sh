#!/usr/bin/env bash
# check-message-band-claims -- a rule message that names a severity must name one that is actually
# shipped: its OWN band when it talks about itself, and the named rule's band when it points at a sibling.
#
# ## The defect this exists for (2026-09-03, batch U160)
#
# 27 rules moved `warning` -> `info`. Nine of them carried a sentence explaining why they were `warning`
# ("That is why it is `warning` and not `critical`", "Kept at `warning` because ..."), and that sentence
# shipped UNCHANGED next to the new band. The message a user receives then stated two different bands
# for the same finding -- in a change whose entire argument was that the band is the trustworthy part.
# Two more pointed at a sibling's band that a DIFFERENT batch had already moved: `security/cmd-injection`
# said the TypeScript sibling `security/shell-exec-interpolation` "is `critical`" for a full day after
# that rule became `warning`.
#
# The sibling half is the older leak. These messages cross-reference each other constantly -- it is how
# the packs explain why two rules that look alike sit at different bands -- and every one of those
# sentences is a copy of a value whose owner is the other pack file. Nothing compared them.
#
# ## Two surfaces, one rule
#
# The same sentence lives twice: in the pack's `message` and, rewritten by hand, in the `Detects`
# column of docs/rules/catalog.md. Both were wrong after U160 -- the catalog said "Capped at `warning`,
# not `critical`" for a rule shipping `info`, and check-rule-desc-tokens.sh only noticed because the
# word `warning` had also left the message, which is luck rather than coverage. So both surfaces are
# judged here, against the same shipped band.
#
# `check-catalog-severity-sync.sh` (its sibling, added the same day) ties the PUBLISHED severity to the
# shipped one. This ties the severity a message SPEAKS to the shipped one. They are the same class of
# defect one layer apart, which is why neither one covers the other.
#
# ## What is judged, and what is deliberately allowed
#
# Only PRESENT-TENSE claims. These messages carry real history ("this rule sat at `critical` until
# 2026-09-02", "it was `warning` before this band moved again"), and that history is worth keeping --
# a message that silently drops the band it used to have teaches the new value and hides that the old
# one was ever believed. So a claim whose immediate context carries a past marker (`was`, `were`,
# `sat at`, `until`, `before this band`, `had been`) is skipped, and writing history in the past tense
# is how an author tells this guard the difference.
#
# SELF claims are matched by verb, so only present forms match at all: "this rule is `X`", "why it is
# `X`", "it is `X` and not/rather than", "Kept at `X`", "stays `X`", "this rule (now) says/reports `X`",
# and the §29 sentence "the `X` SEVERITY IS THAT ABSENCE ...".
#
# SIBLING claims are judged one sentence at a time, and only where the sentence names EXACTLY ONE other
# shipped rule id -- with two ids in a sentence there is no way to tell which the band belongs to, and a
# guard that guesses is worse than one that says what it does not cover. If such a sentence names any
# band at all, one of the bands it names must be the band that rule ships.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

command -v node > /dev/null || { echo "check-message-band-claims: node is required."; exit 1; }

node - <<'NODE'
const fs = require('fs');
const path = require('path');

const DSL = 'rules/dsl';
const sev = new Map();
const rules = [];
for (const d of fs.readdirSync(DSL, { withFileTypes: true })) {
  if (!d.isDirectory()) continue;
  const p = path.join(DSL, d.name, `${d.name}.json`);
  if (!fs.existsSync(p)) continue;
  const pack = JSON.parse(fs.readFileSync(p, 'utf8'));
  for (const r of pack.rules) {
    sev.set(`${pack.id}/${r.id}`, r.severity);
    rules.push({ key: `${pack.id}/${r.id}`, msg: r.message ?? '' });
  }
}
if (rules.length < 100) {
  console.error(`check-message-band-claims: only ${rules.length} rule(s) read from ${DSL}/.`);
  console.error('  Every check below would pass vacuously; a small number here is a broken scan.');
  process.exit(1);
}

const LEVEL = '(critical|warning|info)';
const SELF = [
  new RegExp(`\\bthis rule is \`${LEVEL}\``, 'g'),
  new RegExp(`\\bwhy (?:this rule is|it is) \`${LEVEL}\``, 'g'),
  new RegExp(`\\bit is \`${LEVEL}\` (?:and not|rather than)`, 'g'),
  new RegExp(`\\bKept at \`${LEVEL}\``, 'g'),
  new RegExp(`\\bstays (?:at )?\`${LEVEL}\``, 'g'),
  new RegExp(`\\bthis rule (?:now )?(?:says|reports) \`${LEVEL}\``, 'g'),
  new RegExp(`\\bthe \`${LEVEL}\` SEVERITY IS THAT ABSENCE`, 'g'),
];
const PAST = /\b(was|were|sat at|until|before this band|had been)\b/i;

const selfBad = [];
const sibBad = [];
let selfSeen = 0;
let sibSeen = 0;

for (const r of rules) {
  for (const re of SELF) {
    re.lastIndex = 0;
    let m;
    while ((m = re.exec(r.msg))) {
      if (PAST.test(r.msg.slice(Math.max(0, m.index - 60), m.index))) continue;
      selfSeen++;
      if (m[1] !== sev.get(r.key)) {
        selfBad.push(`${r.key.padEnd(46)} ships \`${sev.get(r.key)}\`, message says "${m[0]}"`);
      }
    }
  }
  for (const s of r.msg.split(/(?<=[.!?])\s+/)) {
    if (PAST.test(s)) continue;
    const ids = [...s.matchAll(/`([a-z][a-z0-9-]*\/[a-z0-9-]+)`/g)]
      .map((x) => x[1]).filter((k) => sev.has(k) && k !== r.key);
    if (new Set(ids).size !== 1) continue;
    const bands = [...s.matchAll(new RegExp(`\`${LEVEL}\``, 'g'))].map((x) => x[1]);
    if (!bands.length) continue;
    sibSeen++;
    const want = sev.get(ids[0]);
    if (!bands.includes(want)) {
      sibBad.push(`${r.key} says of ${ids[0]} (ships \`${want}\`): ${s.replace(/\s+/g, ' ').slice(0, 170)}`);
    }
  }
}

// SURFACE 2: the catalog's hand-written Detects prose, judged against that row's own severity cell.
// The cell itself is check-catalog-severity-sync.sh's subject; here it is the yardstick.
const CATALOG = "docs/rules/catalog.md";
const catBad = [];
let catSeen = 0;
if (fs.existsSync(CATALOG)) {
  const lines = fs.readFileSync(CATALOG, 'utf8').split(/\r?\n/);
  const HEADING = /^### `([a-z-]+)` \(\d+ rules?\)$/;
  const CATROW = /^\| `([a-z0-9-]+)` \| ([a-z][a-z\/]*)(?:\s[^|]*)?\| (.*)$/;
  let pack = null;
  for (let i = 0; i < lines.length; i++) {
    const head = lines[i].match(HEADING);
    if (head) { pack = head[1]; continue; }
    if (lines[i].startsWith('#')) { pack = null; continue; }
    if (!pack || !lines[i].startsWith('| `')) continue;
    const row = lines[i].match(CATROW);
    if (!row) continue;   // row SHAPE is check-catalog-severity-sync.sh's subject, not this guard's
    for (const re of SELF) {
      re.lastIndex = 0;
      let m;
      while ((m = re.exec(row[3]))) {
        if (PAST.test(row[3].slice(Math.max(0, m.index - 60), m.index))) continue;
        catSeen++;
        if (m[1] !== row[2]) {
          catBad.push(CATALOG + ':' + (i + 1) + '  ' + pack + '/' + row[1]
            + ' row carries ' + row[2] + ', prose says "' + m[0] + '"');
        }
      }
    }
  }
}

// PER-SURFACE FLOOR (2026-09-13, review ledger V186). This guard reads THREE surfaces and used to
// report a total. Emptying `docs/rules/catalog.md` took `catSeen` from 7 to 0 while the other two
// kept the line non-zero, so the guard printed OK over a surface that had silently stopped existing.
// A total that is still non-zero HIDES one surface collapsing -- the same lesson the extension-closure
// guard learned per-root (V166), and the reason that floor is per-root rather than per-total.
for (const [name, seen, subject] of [
  ["message self-claims", selfSeen, "the shipped rule messages"],
  ["sibling claims", sibSeen, "the shipped rule messages"],
  ["catalog prose claims", catSeen, "docs/rules/catalog.md"],
]) {
  if (seen === 0) {
    console.error(`check-message-band-claims: read ZERO ${name} from ${subject}.`);
    console.error("  A surface this guard judges produced nothing. That is a moved anchor or a changed");
    console.error("  shape, never a repo that stopped making claims -- and reporting it inside a");
    console.error("  non-zero total is how it stays invisible.");
    process.exit(1);
  }
}

if (!selfBad.length && !sibBad.length && !catBad.length) {
  console.log(`check-message-band-claims: OK (${selfSeen} message self-claim(s), ${sibSeen} sibling-claim(s), ${catSeen} catalog-prose claim(s) checked against the shipped bands).`);
  process.exit(0);
}
if (selfBad.length) {
  console.error('check-message-band-claims: a message states its OWN band as something it does not ship:');
  for (const b of selfBad) console.error(`  ${b}`);
}
if (catBad.length) {
  console.error('check-message-band-claims: catalog prose states a band its own row does not carry:');
  for (const b of catBad) console.error(`  ${b}`);
}
if (sibBad.length) {
  console.error('check-message-band-claims: a message states a SIBLING\'s band as something that rule does not ship:');
  for (const b of sibBad) console.error(`  ${b}`);
}
console.error('');
console.error('  The pack owns the band; the sentence is a copy. Fix the sentence -- and if the old value is');
console.error('  worth keeping, write it in the PAST tense ("sat at `critical` until ...") rather than');
console.error('  deleting it: that is both honest and how this guard is told the claim is history.');
process.exit(1);
NODE
