#!/usr/bin/env node
// wire-key-census.mjs — enumerate every key path a shipped reply carries, so a PUBLIC WIRE SURFACE
// cannot be born between two 1.0 audits without anyone judging it.
//
// ## The failure this exists to end
// The 1.0 gate table is a SNAPSHOT taken by a six-module review on 2026-08-11. Surfaces kept
// arriving after it. Measured on 2026-09-06: eleven public reply keys had been born since that
// audit and NONE had ever been assigned a bucket -- four of them named by an outside reviewer from
// memory, the other seven found only when this census was run. A snapshot that nobody can
// regenerate degrades into a memory, and a memory of a growing surface is wrong in one direction
// only: it always looks smaller than it is.
//
// ## Why three invocations and not one
// Several of those keys are CONDITIONAL -- they appear only when the reply truncates, only when
// prose folds, or only on the join lane. One reply therefore cannot see them, and a census that
// silently misses the conditional half is worse than none: it reports a clean sweep over the keys
// that were never at risk. The three below were chosen because between them they reach all eleven
// (verified 2026-09-06), and every one runs against TRACKED cases/ fixtures, so this needs no
// corpus and CI could run it if that ever becomes worth the maintenance.
//
// ## What it deliberately does NOT do
// It does not diff against a committed baseline, because two node classes make a stored path list
// churn on every unrelated change: keys built FROM DATA (`findings.byRule.<ruleId>`,
// `ruleMessages.<ruleId>`, `shown[].data.<perRuleField>`, `coverage.declaredImportsByExt.<ext>` --
// nine such nodes measured) and keys that differ legitimately between the analyze and cross lanes.
// Collapsing those needs a hand-maintained declaration of which nodes are maps, which is a real
// cost and is filed as a candidate rather than paid for here. So this prints; a human compares
// against the previous release's print and judges what is new. The release checklist owns that step.
//
// Usage:  node scripts/measure/wire-key-census.mjs [--binary <path>] [--counts]
import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";

const args = process.argv.slice(2);
const binIdx = args.indexOf("--binary");
// The shipped binary has no `.exe` tail off Windows. Hardcoding one made this census refuse to run
// on macOS/Linux with "build it first" while a fresh binary sat in that very directory -- an absence
// and a wrong name look the same from here.
const DEFAULT_BIN = `./target/release/zzop${process.platform === "win32" ? ".exe" : ""}`;
const BIN = binIdx >= 0 ? args[binIdx + 1] : DEFAULT_BIN;
const COUNTS = args.includes("--counts");

if (!existsSync(BIN)) {
  console.error(`wire-key-census: no binary at ${BIN} -- build it first (cargo build --release -p zzop-cli-bin)`);
  console.error("  A census taken with a stale binary describes a surface that is not the one shipping.");
  process.exit(2);
}

// Each entry names WHAT IT IS HERE TO REACH, and `proves` turns that sentence into a CHECK.
//
// ## Why `proves` exists (2026-09-14, external review round 22, ledger V242)
// `reaches` was prose. The first entry claimed "prose folding (ruleMessages/messageRef)" and had
// stopped reaching it: `dfd79382`/`165046e5` put the BY-ID fold ahead of the exact fold
// (crates/summary/src/output/rule_prose/mod.rs), so on the api-be fixture every bundled-pack message
// is claimed by `messageBy` and `ruleMessages` never fires. The census went on printing a clean sweep
// over a surface it had quietly stopped covering — which is this script's own stated fear ("a census
// that silently misses the conditional half is worse than none"), arriving through its own door.
//
// A claim about what an invocation reaches has to fail when it stops being true, or it is a memory.
// Every path in `proves` must appear in THAT invocation's own output; the run exits 1 naming the
// invocation otherwise. Keep `proves` to the CONDITIONAL paths the invocation exists for — an
// unconditional key proves nothing about why this row is in the list.
const INVOCATIONS = [
  { name: "analyze", reaches: "the single-tree reply and its always-present shape",
    proves: ["findings.shown[].messageBy", "findings.messageByIdMeaning", "findings.byDirectory"],
    argv: ["analyze", "--config", "cases/trees/api-be/zzop.config.jsonc"] },
  { name: "analyze --limit 1", reaches: "truncation (truncated.severitiesNotShown and its three children)",
    proves: ["findings.truncated", "findings.truncated.severitiesNotShown"],
    argv: ["analyze", "--config", "cases/trees/api-be/zzop.config.jsonc", "--limit", "1"] },
  { name: "cross", reaches: "the join lane; template folding (ruleMessageTemplates/templateParts)",
    proves: ["crossLayerFindings", "buckets", "sources"],
    argv: ["cross", "--config", "cases/zzop.config.jsonc"] },
  // 2026-09-14. The EXACT prose fold plus the ROI summary's filled shape — both of which the three
  // rows above miss, and both of which are public wire surface. `island` is the one tracked fixture
  // that produces a rule whose message by-id cannot rebuild (`unreachable`, a native analysis), so
  // `ruleMessages`/`messageRef` fire there and nowhere else in cases/; it is also one of two that
  // produce a non-null `topRecommendation`, whose four CHILDREN were invisible while the parent key
  // itself was censused — a null parent shows its key and hides its shape.
  { name: "analyze island", reaches: "the exact prose fold (ruleMessages/messageRef) and a FILLED topRecommendation",
    proves: ["findings.ruleMessages", "findings.ruleMessagesMeaning", "findings.shown[].messageRef",
             "findings.shown[].evidencePaths", "architecture.topRecommendation.idMeaning",
             "architecture.topRecommendation.topItem", "coverageGaps.extensions[].kind"],
    argv: ["analyze", "--config", "cases/trees/island/zzop.config.jsonc", "--limit", "1000"] },
  // 2026-09-15, external review round 18 (ledger V256). The join lane's TRUNCATION half — and the
  // finding here is that it was missed by the fix aimed at exactly this class one day earlier.
  //
  // Row 2 above exists because truncation needs `--limit`, and it passes it to `analyze`. The `cross`
  // row passes none, so the join reply never truncated and twelve public paths under
  // `crossLayerFindings.truncated.severitiesNotShown.*` had never been censused. Measured: the analyze
  // twin carries 23 censused paths under that node and the join twin carried 7.
  //
  // 🔴 `proves` could not have caught this, and that is the lesson worth more than the twelve paths.
  // It asserts that paths SOMEONE ALREADY LISTED still appear; a path nobody listed is outside it by
  // construction. A claim-checker defends a claim against rot, never against never having been made.
  // `SIBLING_PARITY` below is the check that asks the other question.
  { name: "cross --limit 1", reaches: "the JOIN lane's truncation (crossLayerFindings.truncated and its children)",
    proves: ["crossLayerFindings.truncated", "crossLayerFindings.truncated.severitiesNotShown",
             "crossLayerFindings.truncated.severitiesNotShown.counts",
             "crossLayerFindings.truncated.severitiesNotShown.firstOmitted"],
    argv: ["cross", "--config", "cases/zzop.config.jsonc", "--limit", "1"] },
  // 2026-09-14. `--profile-rules` is CLI JSON output, which is row 1 of VERSIONING.md's compatibility
  // table, and none of the rows above passes the flag — so ten public key paths had never been
  // censused at all.
  { name: "analyze --profile-rules", reaches: "the per-rule timing report (ruleTimings and its nine children)",
    proves: ["ruleTimings", "ruleTimings.rules[].ruleId", "ruleTimings.rules[].nanos",
             "ruleTimings.meaning", "ruleTimings.cacheHitFiles"],
    argv: ["analyze", "--config", "cases/trees/api-be/zzop.config.jsonc", "--profile-rules", "--limit", "0"] },
];

// Public wire paths this census CANNOT reach from tracked fixtures, each with the reason — so the
// print is not read as a clean sweep over a surface nobody covered.
//
// The check runs the OTHER way: a path listed here that DOES show up is a stale waiver and fails the
// run. A known gap that closed has to be deleted, or this list rots exactly the way `reaches` did.
const UNREACHED = [
  { path: "findings.testPaths",
    why: "needs a tree with at least one finding whose file is TEST surface; none of the 26 cases/ "
       + "fixtures has one (measured 2026-09-14). Adding one would move cases/EXPECTED.jsonc, which "
       + "is the detection gate's ground truth, so the gap is declared rather than closed here. "
       + "`findings.buildPaths` is the same channel and the same gap." },
];

// --- SIBLING PARITY (2026-09-15, ledger V256) ------------------------------------------------------
//
// The question `proves` structurally cannot ask. `proves` defends a claim SOMEONE MADE against rot;
// a shape nobody listed is outside it by construction, which is how twelve `crossLayerFindings
// .truncated.*` paths stayed uncensused through the very commit that was fixing this class one node
// over. So this check asks the other question, and asks it of the SKELETON rather than the payload:
// the two findings lanes ship the same disclosure structure under two roots, and a key that exists on
// one and not the other is either a design decision or a port nobody finished — review axis A8-49.
//
// It compares NAMES, never values, and only names that are SCHEMA. Everything open-keyed is excluded
// below, because those legitimately differ: the two lanes run different rule sets, and `shown[].data`
// is unpromised by VERSIONING.md on purpose ("no machine could hold that promise").
const SIBLING_LANES = [{ a: "findings", b: "crossLayerFindings" }];

// Leaf names that are DATA rather than schema. Each entry says why it cannot be compared.
const OPEN_KEYED = [
  { re: /^byRule\./, why: "rule ids; the two lanes run different rule sets by construction" },
  { re: /^ruleMessages\./, why: "keyed by rule id" },
  { re: /^ruleMessageTemplates\./, why: "keyed by rule id" },
  { re: /^shown\[\]\.data(\.|$)/, why: "per-rule payload, unpromised — VERSIONING.md excludes it explicitly" },
  { re: /^truncated\.severitiesNotShown\.[a-zA-Z]+\.(critical|warning|info)(\.|\[|$)/,
    why: "severity tokens: a closed vocabulary whose PRESENCE is decided by which findings this fixture happened to omit, not by the schema" },
];

// Keys that really do exist on one lane only, each with the reason it is a DECISION. Checked in both
// directions: an undeclared asymmetry fails, and a declared one that has stopped being asymmetric
// fails too — a waiver that no longer waives anything is the shape this repo has had to clean up
// three times.
const LANE_ASYMMETRY = [
  { key: "messageByIdMeaning", lane: "findings",
    why: "the BY-ID fold is the single-tree lane's protocol: a bundled rule's message is rebuilt from its id. The join lane folds by TEMPLATE instead (its findings interpolate per-site values), so this legend has nothing to describe there" },
  { key: "ruleMessages", lane: "findings", why: "the by-id fold's residue — messages that could NOT be rebuilt from an id. Same protocol split" },
  { key: "ruleMessagesMeaning", lane: "findings", why: "legend for the above" },
  { key: "shown[].messageBy", lane: "findings", why: "per-finding marker for which fold produced this message; the join lane's equivalent is `templateParts`" },
  { key: "shown[].messageRef", lane: "findings", why: "pointer into `ruleMessages` for a finding the by-id fold could not rebuild" },
  { key: "ruleMessageTemplates", lane: "crossLayerFindings",
    why: "the TEMPLATE fold is the join lane's protocol: its messages carry per-site values, so the shared half ships once as a template and each finding carries only its parts" },
  { key: "ruleMessageTemplatesMeaning", lane: "crossLayerFindings", why: "legend for the above" },
  { key: "shown[].templateParts", lane: "crossLayerFindings", why: "the per-finding half of the template fold" },
];

// The CLOSED-VOCABULARY half, and the half that actually catches V256.
//
// `SIBLING_PARITY` above compares schema and deliberately excludes severity tokens, because whether a
// fixture omitted a `critical` finding is not a schema fact. That exclusion is right — and it means
// parity alone would NOT have found the twelve join-lane paths, which were all severity leaves. So the
// tokens get their own question, and it is a different one: the severity vocabulary is CLOSED and both
// lanes carry all three, so a token censused on one lane and missing on the other is a FIXTURE gap —
// a case we never produced — never a difference in what the wire can emit. Declared or red.
const SEVERITY_TOKENS = ["critical", "warning", "info"];
const SEVERITY_MAPS = [
  "truncated.severitiesNotShown.counts",
  "truncated.severitiesNotShown.ruleCounts",
  "truncated.severitiesNotShown.firstOmitted",
];
const SEVERITY_GAPS = [
  { lane: "crossLayerFindings", token: "critical",
    why: "no cases/ fixture omits a CRITICAL cross-layer finding under `--limit 1`: the join's own "
       + "findings on that corpus are warning and info, so the critical arm of all three maps is "
       + "emitted-but-never-censused. Closing it means a fixture whose join produces 2+ critical "
       + "findings, which moves cases/EXPECTED.jsonc — the detection gate's ground truth — so the gap "
       + "is declared here rather than closed by editing the answer key (2026-09-15, ledger V256)." },
];

function checkSeverityLeafCoverage(paths) {
  const seen = new Set(paths);
  const problems = [];
  const matched = new Set();
  for (const { a, b } of SIBLING_LANES) {
    for (const lane of [a, b]) {
      for (const map of SEVERITY_MAPS) {
        for (const token of SEVERITY_TOKENS) {
          if (seen.has(`${lane}.${map}.${token}`)) continue;
          const gap = SEVERITY_GAPS.find((g) => g.lane === lane && g.token === token);
          if (gap) { matched.add(`${lane}::${token}`); continue; }
          problems.push(`\`${lane}.${map}.${token}\` is never censused. The severity vocabulary is closed and this lane emits it, so no invocation here produces that case — add one, or declare it in SEVERITY_GAPS with the reason.`);
        }
      }
    }
  }
  for (const g of SEVERITY_GAPS) {
    if (!matched.has(`${g.lane}::${g.token}`)) {
      problems.push(`SEVERITY_GAPS declares \`${g.lane}\` never censuses \`${g.token}\`, and it now does. Delete the entry.`);
    }
  }
  return problems;
}

function skeleton(paths, root) {
  const out = new Set();
  for (const path of paths) {
    if (!path.startsWith(`${root}.`)) continue;
    const tail = path.slice(root.length + 1);
    if (OPEN_KEYED.some((o) => o.re.test(tail))) continue;
    out.add(tail);
  }
  return out;
}

function checkSiblingParity(paths) {
  const problems = [];
  const matched = new Set();
  for (const { a, b } of SIBLING_LANES) {
    const [left, right] = [skeleton(paths, a), skeleton(paths, b)];
    if (left.size === 0 || right.size === 0) {
      problems.push(`sibling parity has nothing to compare: \`${a}\` has ${left.size} schema key(s) and \`${b}\` has ${right.size}. An empty side makes this check vacuous.`);
      continue;
    }
    for (const [own, other, ownRoot] of [[left, right, a], [right, left, b]]) {
      for (const key of own) {
        if (other.has(key)) continue;
        const waiver = LANE_ASYMMETRY.find((w) => w.key === key && w.lane === ownRoot);
        if (waiver) { matched.add(`${ownRoot}::${key}`); continue; }
        problems.push(`\`${ownRoot}.${key}\` has no twin under \`${ownRoot === a ? b : a}\`. Either the sibling lane never got it (a port nobody finished) or it is deliberate — say which in LANE_ASYMMETRY.`);
      }
    }
  }
  for (const w of LANE_ASYMMETRY) {
    if (!matched.has(`${w.lane}::${w.key}`)) {
      problems.push(`LANE_ASYMMETRY declares \`${w.lane}.${w.key}\` as one-lane-only, and it is not. Remove the entry — a waiver that waives nothing goes stale unread.`);
    }
  }
  return problems;
}

function keyPaths(value, prefix, out) {
  if (Array.isArray(value)) {
    for (const item of value) keyPaths(item, `${prefix}[]`, out);
    return;
  }
  if (value && typeof value === "object") {
    for (const key of Object.keys(value)) {
      const path = prefix ? `${prefix}.${key}` : key;
      out.add(path);
      keyPaths(value[key], path, out);
    }
  }
}

const union = new Map(); // path -> Set of invocation names
for (const inv of INVOCATIONS) {
  let stdout;
  try {
    stdout = execFileSync(BIN, inv.argv, { encoding: "utf8", maxBuffer: 256 * 1024 * 1024 });
  } catch (err) {
    console.error(`wire-key-census: \`${inv.name}\` failed -- ${err.message.split("\n")[0]}`);
    console.error(`  This invocation is here to reach ${inv.reaches}. A census missing it is not a census.`);
    process.exit(1);
  }
  const paths = new Set();
  keyPaths(JSON.parse(stdout), "", paths);
  for (const p of paths) {
    if (!union.has(p)) union.set(p, new Set());
    union.get(p).add(inv.name);
  }
}

// THE CLAIMS, checked. Done before printing: a census whose invocations no longer reach what they
// say they reach should not be read at all, let alone compared against a previous release's print.
let broken = false;
for (const inv of INVOCATIONS) {
  const missing = (inv.proves || []).filter(p => !(union.get(p) || new Set()).has(inv.name));
  if (missing.length === 0) continue;
  broken = true;
  console.error(`wire-key-census: \`${inv.name}\` no longer reaches ${missing.join(", ")}.`);
  console.error(`  It is in this list to reach ${inv.reaches}. Either the surface moved (fix the`);
  console.error("  invocation) or it is genuinely gone (delete the path from `proves` and say where it went).");
}
for (const gap of UNREACHED) {
  if (!union.has(gap.path)) continue;
  broken = true;
  console.error(`wire-key-census: \`${gap.path}\` is declared UNREACHED and the census just reached it.`);
  console.error(`  The recorded reason was: ${gap.why}`);
  console.error("  Delete the entry — a waiver for a gap that closed is how this list starts lying.");
}
if (broken) process.exit(1);

// A materialized ARRAY, never `union.keys()`. A Map iterator is consumed once, so passing it here
// left the second lane with an exhausted one and `crossLayerFindings` measured 0 schema keys —
// caught by this check's own empty-side floor on its first run, which is the argument for having
// written the floor before trusting the comparison.
const censused = [...union.keys()];
const parity = [...checkSiblingParity(censused), ...checkSeverityLeafCoverage(censused)];
if (parity.length) {
  console.error("wire-key-census: SIBLING PARITY — the two findings lanes ship different disclosure skeletons.");
  for (const line of parity) console.error(`  ${line}`);
  process.exit(1);
}

const sorted = [...union.keys()].sort();
for (const p of sorted) {
  console.log(COUNTS ? `${p}\t${[...union.get(p)].join(",")}` : p);
}
console.error(`wire-key-census: ${sorted.length} key path(s) across ${INVOCATIONS.length} invocation(s), binary ${BIN}.`);
for (const gap of UNREACHED) {
  console.error(`wire-key-census: NOT covered — ${gap.path}: ${gap.why}`);
}
console.error("  Compare against the previous release's print. Every NEW path needs one sentence:");
console.error("  is this a shape 1.0 may freeze? If not, it is IRREVERSIBLE and it blocks the tag.");
