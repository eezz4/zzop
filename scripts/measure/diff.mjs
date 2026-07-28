#!/usr/bin/env node
// diff.mjs — compare two labeled snapshots taken by snapshot.mjs.
//
// NOT A GUARD, AND NOT RUNNABLE IN CI — see snapshot.mjs's header for why.
//
// THE PRIMARY OUTPUT IS THE ANCHOR SET DIFFERENCE, not a count table. There is deliberately no
// counts-only mode. Per-rule counts hide "N findings vanished AND N different ones appeared":
// measured, `unsafe-read-endpoint` went 6 -> 2 and reading that as "4 fixed" was wrong — three old
// findings disappeared and one NEW true positive appeared, because attribution got more accurate.
// A count delta is consistent with both "improved" and "regressed" and proves neither. So the count
// tables below are context, and the (tree, rule, file, line) set difference is the read.
//
// A second, sharper case: a count can stay IDENTICAL across a corrupted run. Measured while adding
// the default cache dir — with the cache directory missing from the walker's skip list, the scanned
// file count went 4,459 -> 13,399 (exactly 3x, on all 22 trees), yet the finding count sat at 194
// the whole time, because cache .json files yield no findings. Equal counts are not evidence that
// the same thing was analyzed. That is also why snapshot.mjs carries fileCount alongside findings.
//
// Everything that makes a comparison UNTRUSTWORTHY is printed on the SAME SCREEN as the deltas and
// summarized at the end with a nonzero exit: a tree present in one snapshot and missing from the
// other must never read as "0 findings", a product-capped bucket-key list must never read as
// identity, and two runs whose axes/limit/config differ are not comparable at all.
//
// usage: node scripts/measure/diff.mjs <before-label> <after-label> [--runs <dir>]

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "..", "..");

const argv = process.argv.slice(2);
const runsIdx = argv.indexOf("--runs");
const runsRoot = runsIdx === -1 ? path.join(REPO, "scratchpad", "runs") : path.resolve(argv[runsIdx + 1] ?? "");
// `runsIdx === -1` when `--runs` is absent, and `-1 + 1 === 0` would then drop the FIRST positional —
// so the documented two-label form (`diff.mjs <before> <after>`) always printed usage and exited 2.
// Guard the flag's value index instead of computing it unconditionally.
const runsValueIdx = runsIdx === -1 ? -1 : runsIdx + 1;
const positional = argv.filter((a, i) => !a.startsWith("--") && i !== runsValueIdx);
const [beforeLabel, afterLabel] = positional;

if (!beforeLabel || !afterLabel) {
  console.error("usage: node scripts/measure/diff.mjs <before-label> <after-label> [--runs <dir>]");
  console.error("       available labels under " + runsRoot + ":");
  try {
    for (const d of fs.readdirSync(runsRoot)) console.error("         " + d);
  } catch {
    console.error("         (no such directory)");
  }
  process.exit(2);
}

function loadRun(label) {
  const dir = path.join(runsRoot, label);
  if (!fs.existsSync(dir)) {
    console.error("no such run: " + dir);
    process.exit(2);
  }
  const meta = JSON.parse(fs.readFileSync(path.join(dir, "meta.json"), "utf8"));
  const cross = JSON.parse(fs.readFileSync(path.join(dir, "cross.json"), "utf8"));
  const trees = {};
  for (const t of meta.trees) trees[t.sourceId] = JSON.parse(fs.readFileSync(path.join(dir, t.file), "utf8"));
  return { label, dir, meta, cross, trees };
}

const A = loadRun(beforeLabel);
const B = loadRun(afterLabel);

const line = (s) => console.log(s);
const hr = () => line("-".repeat(104));
const untrustworthy = [];

// ---- header: what was measured, with what --------------------------------------------------------
line("=".repeat(104));
line(`MEASUREMENT DIFF    before=${beforeLabel}    after=${afterLabel}`);
line(`tool surface  : ${A.meta.toolSurface}`);
line(`axes          : before [${(A.meta.axes || []).join(", ")}]   after [${(B.meta.axes || []).join(", ")}]`);
line(`before binary : ${A.meta.binary}`);
line(`                sha256 ${A.meta.binarySha256}  mtime ${A.meta.binaryMtime}  size ${A.meta.binarySize}`);
line(`                serverInfo ${JSON.stringify(A.meta.serverInfo)}`);
line(`after  binary : ${B.meta.binary}`);
line(`                sha256 ${B.meta.binarySha256}  mtime ${B.meta.binaryMtime}  size ${B.meta.binarySize}`);
line(`                serverInfo ${JSON.stringify(B.meta.serverInfo)}`);
line(`config        : ${A.meta.configPath}`);
line(`              : ${B.meta.configPath}`);
line(`findings limit: ${A.meta.limit} / ${B.meta.limit}   (snapshot.mjs aborts on any truncation)`);
line("=".repeat(104));

// A snapshot taken with a different axis set, limit, or config is not comparable — say so at the top
// AND in the verdict, rather than letting the reader assume the tables are like-for-like.
const axesA = JSON.stringify(A.meta.axes || null);
const axesB = JSON.stringify(B.meta.axes || null);
if (axesA !== axesB) untrustworthy.push(`AXES DIFFER: before ${axesA} vs after ${axesB} — the runs did not measure the same surfaces.`);
if (A.meta.limit !== B.meta.limit) untrustworthy.push(`LIMIT DIFFERS: ${A.meta.limit} vs ${B.meta.limit}.`);
if (A.meta.configPath !== B.meta.configPath) untrustworthy.push(`CONFIG DIFFERS: ${A.meta.configPath} vs ${B.meta.configPath} — a scope change reads as a rule change.`);
if (A.meta.binarySha256 === B.meta.binarySha256) {
  line(`\n!! NOTE: both snapshots used the SAME binary (identical sha256). Any difference below comes from`);
  line(`   the corpus or the config, not from a code change.`);
}
for (const [who, run] of [["before", A], ["after", B]]) {
  if (run.meta.axisScopeDivergence) {
    line(`\n!! ${who}: AXIS SCOPE DIVERGENCE was recorded (--allow-axis-scope-divergence):`);
    for (const d of run.meta.axisScopeDivergence) {
      line(`     ${d.sourceId}: cross_repo ${d.crossFindingCount} vs analyze_repo ${d.analyzeTotal}`);
    }
    untrustworthy.push(`${who} run has axis scope divergence on ${run.meta.axisScopeDivergence.length} tree(s).`);
  }
}

// ---- tree coverage (a missing tree is NOT "0 findings") ------------------------------------------
const treesA = A.meta.trees.map((t) => t.sourceId);
const treesB = B.meta.trees.map((t) => t.sourceId);
const onlyA = treesA.filter((t) => !treesB.includes(t));
const onlyB = treesB.filter((t) => !treesA.includes(t));
line(`\n## TREE COVERAGE   before ${treesA.length} trees, after ${treesB.length} trees`);
if (onlyA.length || onlyB.length) {
  line("   !! SCOPE MISMATCH — a tree missing on one side is NOT '0 findings'.");
  if (onlyA.length) line("      only in before: " + onlyA.join(", "));
  if (onlyB.length) line("      only in after : " + onlyB.join(", "));
  untrustworthy.push(`TREE COVERAGE MISMATCH: ${onlyA.length} tree(s) only in before, ${onlyB.length} only in after.`);
} else {
  line("   identical tree set.");
}

// ---- ANCHOR SET DIFFERENCE — THE PRIMARY READ ----------------------------------------------------
function anchorsAxis1(run) {
  const m = new Map();
  for (const sid of Object.keys(run.trees)) {
    for (const f of run.trees[sid].findings.shown || []) {
      m.set([sid, f.ruleId, f.file, f.line].join(" | "), {
        tree: sid,
        rule: f.ruleId,
        file: f.file,
        line: f.line,
        severity: f.severity,
      });
    }
  }
  return m;
}
function anchorsAxis2(run) {
  const m = new Map();
  for (const f of (run.cross.crossLayerFindings || {}).shown || []) {
    m.set(["<cross>", f.ruleId, f.file, f.line].join(" | "), {
      tree: f.data?.source ?? "<cross>",
      rule: f.ruleId,
      file: f.file,
      line: f.line,
      severity: f.severity,
    });
  }
  return m;
}
function anchorDiff(title, ma, mb) {
  const gone = [...ma.keys()].filter((k) => !mb.has(k)).sort();
  const born = [...mb.keys()].filter((k) => !ma.has(k)).sort();
  line(`\n## ANCHOR DIFF (PRIMARY READ) — ${title}`);
  line(`   before ${ma.size} distinct anchors, after ${mb.size};  GONE ${gone.length},  NEW ${born.length}`);
  if (!gone.length && !born.length) {
    line("   ** anchor sets are IDENTICAL **");
    return;
  }
  for (const k of gone) {
    const v = ma.get(k);
    line(`   - GONE  ${v.tree} | ${v.rule} | ${v.file}:${v.line} | ${v.severity}`);
  }
  for (const k of born) {
    const v = mb.get(k);
    line(`   + NEW   ${v.tree} | ${v.rule} | ${v.file}:${v.line} | ${v.severity}`);
  }
}
anchorDiff("AXIS 1 analyze_repo (tree, rule, file, line)", anchorsAxis1(A), anchorsAxis1(B));
anchorDiff("AXIS 2 cross-layer findings (rule, file, line)", anchorsAxis2(A), anchorsAxis2(B));

// ---- axis 2: buckets + key identity ---------------------------------------------------------------
line(`\n## AXIS 2 — cross_repo BUCKETS`);
hr();
line("bucket".padEnd(26) + "before".padStart(8) + "after".padStart(8) + "   delta");
hr();
for (const k of [...new Set([...Object.keys(A.cross.buckets || {}), ...Object.keys(B.cross.buckets || {})])].sort()) {
  const a = (A.cross.buckets || {})[k] ?? 0;
  const b = (B.cross.buckets || {})[k] ?? 0;
  const d = b - a;
  line(k.padEnd(26) + String(a).padStart(8) + String(b).padStart(8) + "   " + (d === 0 ? "=" : (d > 0 ? "+" : "") + d));
}
hr();

line(`\n## AXIS 2 — bucketKeys IDENTITY (which keys, not how many)`);
const cappedA = A.meta.bucketKeysTruncated || {};
const cappedB = B.meta.bucketKeysTruncated || {};
const cappedBuckets = new Set([...Object.keys(cappedA), ...Object.keys(cappedB)]);
if (cappedBuckets.size) {
  line("   !! PRODUCT CAP HIT — these buckets' key lists are TRUNCATED, so their diff below is NOT identity:");
  if (Object.keys(cappedA).length) line("      before: " + JSON.stringify(cappedA));
  if (Object.keys(cappedB).length) line("      after : " + JSON.stringify(cappedB));
  untrustworthy.push(`bucketKeys capped by the product for: ${[...cappedBuckets].join(", ")} — those key diffs are not identity.`);
}
for (const k of [...new Set([...Object.keys(A.cross.bucketKeys || {}), ...Object.keys(B.cross.bucketKeys || {})])].sort()) {
  const sa = new Set((A.cross.bucketKeys[k] || []).map(String));
  const sb = new Set((B.cross.bucketKeys[k] || []).map(String));
  const gone = [...sa].filter((x) => !sb.has(x));
  const born = [...sb].filter((x) => !sa.has(x));
  const capNote = cappedBuckets.has(k) ? "  [CAPPED — not identity]" : "";
  if (!gone.length && !born.length) {
    line(`  ${k}: identical (${sa.size} keys)${capNote}`);
    continue;
  }
  line(`  ${k}: ${sa.size} -> ${sb.size}${capNote}`);
  for (const g of gone) line(`      - GONE  ${g}`);
  for (const n of born) line(`      + NEW   ${n}`);
}

// ---- axis 2: edge identity -------------------------------------------------------------------------
const edgeKey = (e) => JSON.stringify([e.kind ?? "", e.key ?? "", e.from ?? e.provider ?? "", e.to ?? e.consumer ?? ""]);
line(`\n## AXIS 2 — EDGE IDENTITY`);
{
  const ma = new Map((A.cross.edges || []).map((e) => [edgeKey(e), e]));
  const mb = new Map((B.cross.edges || []).map((e) => [edgeKey(e), e]));
  const gone = [...ma.keys()].filter((k) => !mb.has(k));
  const born = [...mb.keys()].filter((k) => !ma.has(k));
  line(`  edges ${ma.size} -> ${mb.size}   (gone ${gone.length}, new ${born.length})`);
  for (const g of gone) line(`      - GONE  ${JSON.stringify(ma.get(g))}`);
  for (const n of born) line(`      + NEW   ${JSON.stringify(mb.get(n))}`);
}

// ---- count tables (CONTEXT for the anchor diff above, never a substitute for it) --------------------
function ruleTable(title, mapA, mapB) {
  line(`\n## ${title}   (context — the anchor diff above is the read)`);
  hr();
  line("rule".padEnd(60) + "before".padStart(8) + "after".padStart(8) + "   delta");
  hr();
  let ta = 0;
  let tb = 0;
  for (const k of [...new Set([...Object.keys(mapA), ...Object.keys(mapB)])].sort()) {
    const a = mapA[k] || 0;
    const b = mapB[k] || 0;
    ta += a;
    tb += b;
    const d = b - a;
    line(k.padEnd(60) + String(a).padStart(8) + String(b).padStart(8) + "   " + (d === 0 ? "=" : (d > 0 ? "+" : "") + d));
  }
  hr();
  line("TOTAL".padEnd(60) + String(ta).padStart(8) + String(tb).padStart(8) + "   " + (tb - ta === 0 ? "=" : tb - ta));
  hr();
}
function aggByRule(run) {
  const out = {};
  for (const sid of Object.keys(run.trees)) {
    for (const [r, n] of Object.entries(run.trees[sid].findings.byRule || {})) out[r] = (out[r] || 0) + n;
  }
  return out;
}
ruleTable("AXIS 1 — analyze_repo findings.byRule, ALL TREES SUMMED", aggByRule(A), aggByRule(B));
ruleTable(
  "AXIS 2 — crossLayerFindings.byRule",
  (A.cross.crossLayerFindings || {}).byRule || {},
  (B.cross.crossLayerFindings || {}).byRule || {}
);

line(`\n## AXIS 1 — per-tree finding totals   (context)`);
hr();
line("tree".padEnd(38) + "before".padStart(8) + "after".padStart(8) + "   delta");
hr();
let sa = 0;
let sb = 0;
for (const sid of [...new Set([...treesA, ...treesB])].sort()) {
  const a = A.trees[sid] ? A.trees[sid].findings.total : null;
  const b = B.trees[sid] ? B.trees[sid].findings.total : null;
  sa += a || 0;
  sb += b || 0;
  const d = a === null || b === null ? "MISSING-TREE" : b - a === 0 ? "=" : (b - a > 0 ? "+" : "") + (b - a);
  line(sid.padEnd(38) + String(a ?? "-").padStart(8) + String(b ?? "-").padStart(8) + "   " + d);
}
hr();
line("TOTAL".padEnd(38) + String(sa).padStart(8) + String(sb).padStart(8) + "   " + (sb - sa === 0 ? "=" : sb - sa));
hr();

// ---- run-level warnings ------------------------------------------------------------------------------
line(`\n## RUN-LEVEL WARNINGS diff (configWarnings + warnings)`);
{
  const warnSet = (run) => new Set([...(run.cross.configWarnings || []), ...(run.cross.warnings || [])].map(String));
  const wa = warnSet(A);
  const wb = warnSet(B);
  const gone = [...wa].filter((x) => !wb.has(x));
  const born = [...wb].filter((x) => !wa.has(x));
  if (!gone.length && !born.length) line(`   identical (${wa.size} entries)`);
  for (const g of gone) line(`   - GONE  ${g}`);
  for (const n of born) line(`   + NEW   ${n}`);
}

// ---- verdict -----------------------------------------------------------------------------------------
line("");
line("=".repeat(104));
if (untrustworthy.length) {
  line("COMPARISON IS NOT TRUSTWORTHY — do not quote the deltas above without saying this:");
  for (const u of untrustworthy) line("  * " + u);
  line("=".repeat(104));
  process.exit(3);
}
line("COMPARISON IS LIKE-FOR-LIKE: same axes, same limit, same config, same tree set, no capped key lists.");
line("=".repeat(104));
