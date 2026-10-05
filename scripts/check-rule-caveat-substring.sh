#!/usr/bin/env bash
# Rule-caveat substring guard — a rule's `caveat` must be a VERBATIM substring of its own `message`.
#
# ## What the field is
# `caveat` names the span of a rule's message that answers "what does a reader need to know before
# acting on this" — the shape of its common false positive, what the matcher cannot see, or what
# breaks if the fix is applied blind. The analyze reply ships that span inline, once per distinct rule
# on the screen (`ruleCaveats`), while the message itself rides a `messageBy: ruleId` pointer and is
# not on the wire at all.
#
# ## Why it is a copy, and why that is safe
# Two copies of a sentence are normally this repo's most-repeated defect — one of them always goes
# stale. The alternative here was worse and was measured rather than assumed: splitting the caveat OUT
# of `message` would have meant rewriting the prose of most of the rules that carry one, because only
# a handful have it at the front, and at least one message points BACKWARDS at its own earlier
# sentence, which a cut breaks silently. (The counts are deliberately not written here — this script
# prints them on every run, and a number in a comment beside a script that derives it is the drift
# this repo has paid for repeatedly. The three it used to spell were each wrong by the time the batch
# that wrote them landed.)
#
# So the copy stays and the drift is made IMPOSSIBLE instead of merely discouraged: this guard fails
# unless `message.contains(caveat)`, byte for byte. Two copies that cannot differ are one fact. Edit
# the message's caveat span without updating the field and the build goes red here, naming the rule.
#
# ## What it does NOT check
# That a rule WHICH SHOULD have a caveat has one. There is no mechanical test for "this rule has a
# known false-positive shape" — a rule with none must not invent one, and a missing caveat is silence,
# not a false claim. The population it guards is the rules that declare the field.
#
# ## Why the walk is RECURSIVE and the floor exists
# Both were findings against this guard's first version, on the day it was written.
#   * The needle was `rules/dsl/<d>/<d>.json` — one pack per directory, stem matching the directory.
#     `crates/config/build.rs` embeds `rules/dsl/**/*.json` RECURSIVELY with no name constraint, so a
#     pack at a nested path or under a differently-stemmed file would ship a caveat this guard never
#     read. `check-marker-claims.sh` already walks recursively and shape-tests each file; that is the
#     house precedent and this now copies it.
#   * There was no non-empty floor. Move the packs and the guard printed `clean (0 caveat(s) across 0
#     pack(s))` and exited 0 — vouching for invariants it never read. Three sibling guards carry a
#     floor for exactly this reason (`check-catalog-severity-sync.sh`, `check-message-band-claims.sh`,
#     `check-marker-claims.sh`); this one did not.
# `examples/packs/` is scanned too, for the reason `check-marker-claims.sh` scans it: those files are
# the published "write your own pack" set and ship as MCP contract resources, so a caveat that drifts
# there teaches the drift.
set -euo pipefail

cd "$(dirname "$0")/.."

node - <<'NODE'
const fs = require('node:fs');
const path = require('node:path');

// A pack is any JSON under these roots whose SHAPE is a pack — the same test `check-marker-claims.sh`
// applies, and the same recursion, so neither a nested directory nor a differently-stemmed file can
// hide from this guard while `build.rs` still embeds it.
const ROOTS = ['rules/dsl', 'examples/packs'];
const RULES_DSL_RULE_FLOOR = 100; // see the header: a subject set that collapses must fail, not pass
const PACK_FLOOR = 8;

function walk(dir, out) {
  if (!fs.existsSync(dir)) return out;
  for (const entry of fs.readdirSync(dir, { withFileTypes: true }).sort((a, b) => a.name < b.name ? -1 : 1)) {
    const p = path.join(dir, entry.name);
    if (entry.isDirectory()) walk(p, out);
    else if (entry.isFile() && entry.name.endsWith('.json')) out.push(p);
  }
  return out;
}

let checked = 0;
let packs = 0;
let dslRules = 0;
const bad = [];

for (const root of ROOTS) {
  for (const p of walk(root, [])) {
    let doc;
    try {
      doc = JSON.parse(fs.readFileSync(p, 'utf8'));
    } catch (e) {
      bad.push(`${p}: not parseable as JSON (${e.message})`);
      continue;
    }
    if (!doc || typeof doc !== 'object' || !Array.isArray(doc.rules)) continue; // not a pack
    packs += 1;
    if (root === 'rules/dsl') dslRules += doc.rules.length;

    for (const r of doc.rules) {
      // `null` is ABSENCE, not a malformed value: the schema publishes `["string", "null"]` and the
      // Rust field is `Option<String>` with `#[serde(default)]`, so a generator emitting an explicit
      // null for an absent optional is writing legal input. Rejecting it would have made this guard
      // red on a pack the loader accepts — which is the guard disagreeing with the contract it guards.
      if (r.caveat === undefined || r.caveat === null) continue;
      checked += 1;
      const id = `${doc.id ?? path.basename(p, '.json')}/${r.id}`;
      if (typeof r.caveat !== 'string' || r.caveat.length === 0) {
        bad.push(`${id} (${p}): caveat is neither absent nor a non-empty string`);
        continue;
      }
      if (typeof r.message !== 'string') {
        // Named rather than thrown: an uncaught TypeError here is still red, but it reads as a broken
        // guard instead of a broken pack, and the next reader debugs the wrong file.
        bad.push(`${id} (${p}): declares a caveat but has no string \`message\` to be a substring of`);
        continue;
      }
      if (!r.message.includes(r.caveat)) {
        bad.push(
          `${id} (${p}): caveat is NOT a verbatim substring of message — ` +
            `the two copies have drifted, which is the one thing this field's shape forbids. ` +
            `caveat opens: ${JSON.stringify(r.caveat.slice(0, 60))}`
        );
      }
    }
  }
}

// The floor, before any verdict. A guard whose subject set collapsed reports what it read, never
// success — `running 0 tests` with exit 0 is the shape this repo has been burned by.
if (packs < PACK_FLOOR || dslRules < RULES_DSL_RULE_FLOOR) {
  console.error('check-rule-caveat-substring: REFUSING TO JUDGE — the subject set collapsed.');
  console.error(
    `  found ${packs} pack(s) (floor ${PACK_FLOOR}) and ${dslRules} rule(s) under rules/dsl ` +
      `(floor ${RULES_DSL_RULE_FLOOR}).`
  );
  console.error('  Either the packs moved or this walk stopped matching them. A clean report here would');
  console.error('  vouch for invariants nothing read.');
  process.exit(1);
}

if (bad.length) {
  console.error('check-rule-caveat-substring: FAILED');
  for (const b of bad) console.error(`  ${b}`);
  console.error(
    '\nA `caveat` is a span OF the message, quoted verbatim so the reply can ship it alone. ' +
      'Fix by copying the span again from `message`, or by widening/narrowing the span — never by ' +
      'rewording one side.'
  );
  process.exit(1);
}
console.log(
  `check-rule-caveat-substring: clean (${checked} caveat(s) across ${packs} pack(s), ` +
    `${dslRules} rule(s) under rules/dsl; each caveat a verbatim substring of its own message).`
);
NODE
