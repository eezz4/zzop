// fixture-anchor-coverage.mjs — every fixture that names a shipped rule must be SCORED by the gate,
// not merely walked by it.
//
// ## The defect this exists for
//
// `cases/trees/api-be/services/be-security.timing-unsafe-compare.ts` labelled
// `timingSafeEqual(Buffer.from(apiKey), Buffer.from(provided))` as the GOOD half of its rule, while
// that rule's own message had begun warning that `crypto.timingSafeEqual` throws
// `RangeError [ERR_CRYPTO_TIMING_SAFE_EQUAL_LENGTH]` on exactly that input shape — a stored secret
// against a request-supplied value. The repository's benchmark was teaching the shape its rule had
// just stopped teaching, and the detection gate scored 245/245 through it, because a `good` half is
// judged only by its SILENCE and silence is what a correct good half and a harmful one both produce.
// (Both halves were repaired in `4c504978`, v0.34.0. The class is what remains.)
//
// ## What is checkable here, and what is measured NOT to be
//
// The tempting check is the direct one: derive the remedy the rule's message spells out and require
// the fixture's `good` half to use it. That was BUILT AND MEASURED before this file was written, in
// two strengths, and it does not carry:
//
//   * any backticked span in the message that contains a call  ->  28 fixtures in scope, 4 flagged,
//     3 of the 4 false (the extractor reads `shapes(`, `sits(`, `info(` out of prose).
//   * only spans that are a single well-formed call expression ->  28 in scope, 5 flagged, at least 2
//     of them false for the same reason, and the rest legitimate (a message may name an alternative
//     API that a correct good half has no reason to call).
//
// A guard that is right one time in three does not get obeyed; it gets an escape hatch bolted on. So
// the remedy-text axis is left OPEN and written down rather than shipped weak — see the backlog entry
// this file's commit closes.
//
// What IS derivable with no judgment call is the weaker sibling, and it is the one that lets a drift
// score as success in the first place: A FIXTURE THAT NAMES A RULE AND IS NOT ANCHORED ANYWHERE IN
// `cases/EXPECTED.jsonc` IS SCORED BY SILENCE ALONE. Nothing in the corpus distinguishes "the rule
// works on this file" from "the rule no longer sees this file at all", so any drift inside it — good
// half or bad half — is scored as success by construction. That is this file's subject.
//
// ## Every input is derived; there is no list to fall out of date
//   * rule ids          <- rules/dsl/<pack>/*.json                (the shipped packs)
//   * sourceId -> tree  <- cases/zzop.config.jsonc `trees[]`       (the same map the run uses)
//   * anchors + benign  <- cases/EXPECTED.jsonc                    (the answer key itself)
//   * roster            <- the fixture filenames under cases/trees/
// A fixture earns its place in the roster by being NAMED after a rule (`<pack>.<rule-id>.<ext>`),
// which is this corpus's own convention and is what makes it a claim about that rule.
//
// ## The floors, and why there are two
// An empty roster or an empty anchor set means the parse broke, not that the tree is clean. Both are
// reported as MEASUREMENT FAILURE with their own text, because "0 violations" and "I could not
// measure" are the same exit code otherwise — the shape this repository has a guard fleet for.
//
// usage: node scripts/measure/fixture-anchor-coverage.mjs
//        (invoked by scripts/measure/detection-gate.sh, before the release build)
import fs from 'fs';
import path from 'path';
// For the rule-side census below: the shipped pack list comes from git, not from a glob walk, so a
// pack that is present-but-untracked cannot inflate the denominator.
import { execFileSync } from 'child_process';

const CASES = 'cases';
const TREES = path.join(CASES, 'trees');

function die(msg, detail) {
  console.error(`fixture-anchor-coverage: ${msg}`);
  for (const d of detail || []) console.error(`  ${d}`);
  process.exit(1);
}

// JSONC -> JSON. Line comments only; this tree's two files use nothing else, and a general stripper
// would have to understand strings containing `//`, which both files do carry (URLs, path keys).
function readJsonc(file) {
  const text = fs.readFileSync(file, 'utf8');
  const out = [];
  for (const line of text.split('\n')) {
    let inStr = false;
    let esc = false;
    let cut = line.length;
    for (let i = 0; i < line.length; i++) {
      const c = line[i];
      if (esc) { esc = false; continue; }
      if (c === '\\') { esc = true; continue; }
      if (c === '"') { inStr = !inStr; continue; }
      if (!inStr && c === '/' && line[i + 1] === '/') { cut = i; break; }
    }
    out.push(line.slice(0, cut));
  }
  // Trailing commas are legal in this tree's .jsonc and not in JSON.
  return JSON.parse(out.join('\n').replace(/,(\s*[}\]])/g, '$1'));
}

// --- shipped rule ids --------------------------------------------------------------------------
const ruleIds = new Set();
for (const pack of fs.readdirSync('rules/dsl')) {
  const dir = path.join('rules/dsl', pack);
  if (!fs.statSync(dir).isDirectory()) continue;
  for (const f of fs.readdirSync(dir)) {
    if (!f.endsWith('.json')) continue;
    let json;
    try { json = JSON.parse(fs.readFileSync(path.join(dir, f), 'utf8')); } catch { continue; }
    for (const r of Object.values(json.rules || {})) if (r && r.id) ruleIds.add(r.id);
  }
}
if (ruleIds.size === 0) die('read ZERO rule ids out of rules/dsl/ — MEASUREMENT FAILURE, not a clean tree.');

// --- sourceId -> tree root ----------------------------------------------------------------------
const config = readJsonc(path.join(CASES, 'zzop.config.jsonc'));
const rootBySource = new Map();
for (const s of config.trees || []) {
  if (s && s.sourceId && s.root) rootBySource.set(s.sourceId, s.root.replace(/^\.\//, ''));
}
if (rootBySource.size === 0) die('cases/zzop.config.jsonc named ZERO trees — MEASUREMENT FAILURE.');

// --- what the key scores ------------------------------------------------------------------------
const expected = readJsonc(path.join(CASES, 'EXPECTED.jsonc'));
const ARRAY_KEYS = new Set(['benign', 'untracked']);

// `<sourceId>/<tree-relative path>` -> `cases/trees/<root>/<path>`. A prefix that is not a declared
// sourceId is returned unresolved rather than guessed at, and shows up as an unmatched roster entry
// instead of silently vouching for one.
function resolve(key) {
  const cut = key.indexOf('/');
  if (cut < 0) return null;
  const root = rootBySource.get(key.slice(0, cut));
  if (!root) return null;
  return path.join(CASES, root, key.slice(cut + 1));
}

const anchored = new Set();
for (const [key, value] of Object.entries(expected)) {
  if (ARRAY_KEYS.has(key) || !Array.isArray(value)) continue;
  const file = resolve(key.replace(/:\d+$/, ''));
  if (file) anchored.add(file);
}
if (anchored.size === 0) die('resolved ZERO anchors out of cases/EXPECTED.jsonc — MEASUREMENT FAILURE.');

// Deliberate silence, declared in the key itself: decoys that must NOT fire, and the test halves of
// the test-convention pairs. Their whole claim is the absence of a finding, so requiring an anchor
// would be requiring the opposite of what they assert.
const declaredSilent = new Set();
for (const listKey of ARRAY_KEYS) {
  for (const entry of expected[listKey] || []) {
    const file = resolve(String(entry).replace(/:\d+$/, ''));
    if (file) declaredSilent.add(file);
  }
}

// --- the roster ---------------------------------------------------------------------------------
const files = [];
(function walk(dir) {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p); else files.push(p);
  }
})(TREES);

const roster = [];
for (const f of files) {
  // `<anything>.<rule-id>.<ext>` — every dotted segment is offered, so `security.no-where.decoy.ts`
  // and `be-db.update-delete-no-where.ts` are both read without a positional assumption.
  const segments = path.basename(f).split('.');
  const named = segments.filter((s) => ruleIds.has(s));
  if (named.length > 0) roster.push({ file: f, rules: named });
}
if (roster.length === 0) {
  die('ZERO fixtures under cases/trees/ are named after a shipped rule id — MEASUREMENT FAILURE.', [
    'Either the `<pack>.<rule-id>.<ext>` naming convention changed or the pack read above is wrong.',
    'This is not "no violations": with an empty roster this check asserts nothing at all.',
  ]);
}

const silent = roster.filter((r) => !anchored.has(r.file) && !declaredSilent.has(r.file));

// --- RULE-SIDE COVERAGE (2026-09-15, ledger V260) --------------------------------------------------
//
// Everything above asks about FIXTURES: does every fixture that names a rule get scored? That is the
// right question and it is not the only one. External review round 18 asked the other one — how much
// of the RULE SET does the gate defend? — and nothing here answered it.
//
// It matters because of how the gate's own headline reads. `292/292, precision 100%` is a true
// sentence about the labelled corpus, and a reader takes it as a sentence about the rules. Measured:
// 38 of 118 shipped DSL rules (32%) appear nowhere in cases/EXPECTED.jsonc, and three packs are
// wholly absent — `browser`, `redis` and `reliability` between them hold 19 of those 38. A regression
// in any of them is invisible to the gate, and the gate still prints 100%.
//
// 🔴 This is DECLARED rather than closed, and the reason is the same one the wire census gives for its
// own gaps: closing it means adding fixtures, which moves cases/EXPECTED.jsonc — the gate's ground
// truth — so "fix the coverage" and "edit the answer key" are the same edit. That trade needs to be
// made deliberately per rule, not swept in to make a number go up.
//
// What IS enforced is the ratchet: coverage may grow freely and may not shrink. A rule that loses its
// last anchor fails here, naming itself, instead of quietly joining the silent 38.
const ANCHORED_FLOOR = 80; // of 118 shipped DSL rules, measured 2026-09-15. Recount: this script.

const packRules = [];
for (const file of execFileSync("git", ["ls-files", "rules/dsl/*.json"], { encoding: "utf8" }).trim().split("\n")) {
  const pack = JSON.parse(fs.readFileSync(file, "utf8"));
  for (const rule of pack.rules) packRules.push(`${pack.id}/${rule.id}`);
}
if (packRules.length === 0) {
  console.error("fixture-anchor-coverage: no shipped DSL rule was read — the rule-side census below would vouch for nothing.");
  process.exit(1);
}
const expectedText = fs.readFileSync(path.join(CASES, "EXPECTED.jsonc"), "utf8");
const unanchoredRules = packRules.filter((id) => !expectedText.includes(id));
const anchoredRules = packRules.length - unanchoredRules.length;
const byPack = new Map();
for (const id of packRules) {
  const pack = id.slice(0, id.indexOf("/"));
  const seen = byPack.get(pack) ?? { total: 0, silent: 0 };
  seen.total += 1;
  if (unanchoredRules.includes(id)) seen.silent += 1;
  byPack.set(pack, seen);
}

console.log(`fixture-anchor-coverage: ${roster.length} fixtures name a shipped rule id.`);
console.log(`  shipped DSL rules anchored       : ${anchoredRules} of ${packRules.length} (${unanchoredRules.length} named nowhere in the ground truth)`);
for (const [pack, seen] of [...byPack].sort()) {
  if (seen.silent === 0) continue;
  const whole = seen.silent === seen.total ? "  <- the WHOLE pack" : "";
  console.log(`      ${pack.padEnd(12)} ${seen.total - seen.silent}/${seen.total} anchored${whole}`);
}
console.log(`  ^ the gate's score is 100% of what is LABELLED, never of what ships. See this file's rule-side note.`);
if (anchoredRules < ANCHORED_FLOOR) {
  console.error(`fixture-anchor-coverage: RULE COVERAGE SHRANK -- ${anchoredRules} shipped rules are anchored, below the recorded floor of ${ANCHORED_FLOOR}.`);
  console.error(`  A rule lost its last anchor in cases/EXPECTED.jsonc, so the detection gate no longer defends it and`);
  console.error(`  will keep printing 100%. Restore the anchor, or lower ANCHORED_FLOOR in the same commit and say why.`);
  process.exit(1);
}

console.log(`  anchored in cases/EXPECTED.jsonc : ${roster.filter((r) => anchored.has(r.file)).length}`);
console.log(`  declared silent (decoy/test half): ${roster.filter((r) => declaredSilent.has(r.file)).length}`);
console.log(`  scored by silence alone          : ${silent.length}`);

if (silent.length > 0) {
  console.error('');
  console.error('fixture-anchor-coverage: FAILED — these fixtures name a rule and are scored by nothing.');
  console.error('  Neither an anchor nor a `benign`/`untracked` entry in cases/EXPECTED.jsonc mentions');
  console.error('  them, so the gate cannot tell "the rule works here" from "the rule stopped seeing');
  console.error('  this file". Any drift inside one of these — including a `good` half that teaches the');
  console.error('  shape its own rule warns about — scores as success.');
  for (const s of silent) console.error(`    ${s.file}   (names: ${s.rules.join(', ')})`);
  console.error('  Fix by LABELLING, not by deleting: add the anchor the bad half should produce, or');
  console.error('  list the file under `benign` with the reason its silence is the claim.');
  process.exit(1);
}
