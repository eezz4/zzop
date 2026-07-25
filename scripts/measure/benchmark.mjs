#!/usr/bin/env node
// benchmark.mjs — score a snapshot against a labeled detection benchmark's ground truth, and
// (with --write-expected) regenerate that ground truth from a verified run.
//
// NOT A GUARD, AND NOT RUNNABLE IN CI — see snapshot.mjs's header. The benchmark corpus this scores
// is gitignored (`/corpus`), so this script ships and the data it reads does not.
//
// This replaces the corpus's own `run.mjs` / `measure.mjs` / `gen-expected.mjs`, which loaded the
// engine through a Node native addon and a JS CLI library that no longer exist — the last execution
// path in this repo that ran analysis through JavaScript. Everything now goes through the shipped
// binary: snapshot.mjs spawns it and validates every reply, and this script only reads the snapshot
// it wrote. That split is deliberate — scoring cannot accidentally measure something the snapshot
// contract already refused (an empty reply, a truncated anchor list, a missing axis).
//
// GROUND-TRUTH KEY FORMAT: "<sourceId>/<tree-relative path>:<line>" -> [ruleId, ...], plus a
// "benign" array of "<sourceId>/<path>" control files where ANY finding counts as a false positive.
// The sourceId prefix is load-bearing: keys used to be tree-RELATIVE, and measured against the
// current corpus that spelling collapses 12 distinct locations in different trees onto the same key
// (`index.ts:5` exists in several trees). A collision silently converts a miss in one tree into a
// hit from another. A legacy-format file is REFUSED rather than scored, because scoring it produces
// a plausible-looking recall number that means nothing.
//
// usage:
//   node scripts/measure/benchmark.mjs --run <runs-dir/label> --expected <EXPECTED.jsonc> [--dump]
//   node scripts/measure/benchmark.mjs --run <runs-dir/label> --expected <EXPECTED.jsonc> --write-expected
//
// Regenerate ONLY after confirming with --dump that every `good` example in the corpus is silent:
// --write-expected locks whatever fired as the truth, including any false positive.

import fs from "node:fs";
import path from "node:path";

function fail(msg) {
  console.error("\nBENCHMARK ABORT: " + msg + "\n");
  process.exit(1);
}

const argv = process.argv.slice(2);
function arg(name, dflt) {
  const i = argv.indexOf("--" + name);
  if (i === -1) {
    if (dflt === undefined) fail("missing --" + name);
    return dflt;
  }
  if (i + 1 >= argv.length || argv[i + 1].startsWith("--")) fail("--" + name + " needs a value");
  return argv[i + 1];
}
const runDir = path.resolve(arg("run"));
const expectedPath = path.resolve(arg("expected"));
const dump = argv.includes("--dump");
const writeExpected = argv.includes("--write-expected");

if (!fs.existsSync(path.join(runDir, "meta.json"))) {
  fail(`no meta.json in ${runDir} — point --run at a directory written by snapshot.mjs.`);
}

// ---- read the snapshot (never the binary — snapshot.mjs already validated the binary's replies) ---
const meta = JSON.parse(fs.readFileSync(path.join(runDir, "meta.json"), "utf8"));
const cross = JSON.parse(fs.readFileSync(path.join(runDir, "cross.json"), "utf8"));

// Both axes are required for a benchmark score: per-tree rules come from analyze_repo and
// `cross-layer/*` rules exist ONLY in the join reply. Scoring a single-axis snapshot would report
// every cross-layer expectation as a false negative and blame the engine for the harness.
const axes = meta.axes || [];
for (const needed of ["analyze_repo", "cross_repo"]) {
  if (!axes.includes(needed)) {
    fail(
      `snapshot ${meta.label} did not run '${needed}' (axes: ${JSON.stringify(axes)}).\n` +
        "  A benchmark score needs BOTH axes — per-tree rules come from analyze_repo, and every\n" +
        "  cross-layer/* rule exists only in the cross_repo join reply."
    );
  }
}
if (meta.axisScopeDivergence) {
  console.error(
    "!! NOTE: this snapshot recorded axis scope divergence — the two axes disagreed on per-tree\n" +
      "   finding counts, so some findings below may be attributable to config scope rather than rules:\n" +
      meta.axisScopeDivergence.map((d) => `     ${d.sourceId}: cross ${d.crossFindingCount} vs analyze ${d.analyzeTotal}`).join("\n")
  );
}

const norm = (s) => String(s).replace(/\\/g, "/").replace(/\/+/g, "/");
const sourceIds = new Set(meta.trees.map((t) => t.sourceId));

const findings = [];
for (const t of meta.trees) {
  const a = JSON.parse(fs.readFileSync(path.join(runDir, t.file), "utf8"));
  for (const f of a.findings.shown || []) {
    findings.push({ key: `${norm(t.sourceId)}/${norm(f.file)}:${f.line}`, ruleId: f.ruleId, cross: false });
  }
}
for (const f of (cross.crossLayerFindings || {}).shown || []) {
  // A cross-layer finding names its tree in data.source and its file relative to that tree's root,
  // so it lands in the same key space as the per-tree findings above.
  const src = f.data?.source;
  const prefix = src && sourceIds.has(src) ? norm(src) : "<cross>";
  const file = f.file ? norm(f.file) : `(no-file)/${f.ruleId}`;
  findings.push({ key: `${prefix}/${file}:${f.line || 0}`, ruleId: f.ruleId, cross: true });
}

// ---- ground truth ---------------------------------------------------------------------------------
// String-aware JSONC comment stripper. A naive line regex would also eat a `//` that lives inside a
// key (a URL-shaped path), which is exactly how a ground-truth file gets quietly mis-parsed.
function stripJsonComments(src) {
  let out = "";
  let i = 0;
  let inStr = false;
  while (i < src.length) {
    const ch = src[i];
    if (inStr) {
      out += ch;
      if (ch === "\\") {
        out += src[i + 1] ?? "";
        i += 2;
        continue;
      }
      if (ch === '"') inStr = false;
      i++;
      continue;
    }
    if (ch === '"') {
      inStr = true;
      out += ch;
      i++;
      continue;
    }
    if (ch === "/" && src[i + 1] === "/") {
      while (i < src.length && src[i] !== "\n") i++;
      continue;
    }
    if (ch === "/" && src[i + 1] === "*") {
      i += 2;
      while (i < src.length && !(src[i] === "*" && src[i + 1] === "/")) i++;
      i += 2;
      continue;
    }
    out += ch;
    i++;
  }
  return out;
}

function writeGroundTruth() {
  const m = new Map();
  for (const f of findings) {
    if (!m.has(f.key)) m.set(f.key, new Set());
    m.get(f.key).add(f.ruleId);
  }
  const keys = [...m.keys()].sort();
  const body = keys.map((k) => `  ${JSON.stringify(k)}: ${JSON.stringify([...m.get(k)].sort())}`).join(",\n");
  const doc =
    `{\n` +
    `  // AUTO-CALIBRATED from snapshot "${meta.label}" (both axes; binary sha256 ${meta.binarySha256}).\n` +
    `  // Keys are "<sourceId>/<tree-relative path>:<line>". Regenerate ONLY after confirming with\n` +
    `  // --dump that every \`good\` example is silent — this locks whatever fired, false positives included.\n\n` +
    `${body},\n\n` +
    `  // Control files: ANY finding in one of these paths counts as a false positive.\n` +
    `  "benign": []\n` +
    `}\n`;
  fs.writeFileSync(expectedPath, doc);
  console.log(`wrote ${keys.length} locations / ${findings.length} findings -> ${expectedPath}`);
}

if (writeExpected) {
  writeGroundTruth();
  process.exit(0);
}

if (!fs.existsSync(expectedPath)) {
  fail(`no ground truth at ${expectedPath} — create it from a verified run with --write-expected.`);
}
const expectedRaw = JSON.parse(stripJsonComments(fs.readFileSync(expectedPath, "utf8")));
const benign = new Set((expectedRaw.benign || []).map(norm));
const expected = new Map();
for (const [k, v] of Object.entries(expectedRaw)) {
  if (k === "benign") continue;
  expected.set(norm(k), new Set(v));
}
if (expected.size === 0) fail(`${expectedPath} has no expectations.`);

// Refuse a legacy (tree-relative, un-prefixed) ground truth instead of scoring it into a number that
// looks real. Detection: how many keys start with a sourceId this snapshot actually measured.
const prefixed = [...expected.keys()].filter((k) => sourceIds.has(k.split("/")[0])).length;
if (prefixed / expected.size < 0.5) {
  fail(
    `${expectedPath} does not use the "<sourceId>/<path>:<line>" key format — only ${prefixed} of\n` +
      `  ${expected.size} keys start with one of this run's sourceIds (${[...sourceIds].join(", ")}).\n` +
      "  This is the legacy tree-relative format, in which locations in different trees collapse onto\n" +
      "  the same key. Scoring against it yields a recall number that means nothing, so it is refused.\n" +
      "  Re-calibrate: confirm with --dump that every `good` example is silent, then re-run with\n" +
      "  --write-expected."
  );
}

// ---- score -------------------------------------------------------------------------------------
let tp = 0;
const fp = [];
const matched = new Set();
for (const f of findings) {
  const fileOnly = f.key.replace(/:\d+$/, "");
  if (benign.has(fileOnly)) {
    fp.push(`${f.key} ${f.ruleId}  (benign control — must not fire)`);
    continue;
  }
  const exp = expected.get(f.key);
  if (exp && exp.has(f.ruleId)) {
    tp++;
    matched.add(`${f.key}|${f.ruleId}`);
  } else {
    fp.push(`${f.key} ${f.ruleId}`);
  }
}
const fn = [];
for (const [key, rules] of expected) {
  for (const r of rules) if (!matched.has(`${key}|${r}`)) fn.push(`${key} ${r}`);
}

const totalExpected = tp + fn.length;
const recall = totalExpected ? ((tp / totalExpected) * 100).toFixed(1) : "n/a";
const precision = tp + fp.length ? ((tp / (tp + fp.length)) * 100).toFixed(1) : "n/a";

console.log(`\nsnapshot   : ${meta.label}   (${meta.trees.length} trees, binary sha256 ${meta.binarySha256.slice(0, 12)})`);
console.log(`ground truth: ${expectedPath}`);
console.log(`findings ${findings.length}  |  labeled expectations ${totalExpected}`);
console.log(`TP ${tp}   FN ${fn.length}   FP ${fp.length}`);
console.log(`recall ${recall}%   precision ${precision}%\n`);
if (fn.length) {
  console.log("-- FN (expected, did not fire) --");
  for (const x of fn.sort()) console.log("  " + x);
}
if (fp.length) {
  console.log("-- FP (fired, not expected) --");
  for (const x of fp.sort()) console.log("  " + x);
}
if (dump) {
  console.log("\n-- all findings --");
  for (const f of findings.slice().sort((a, b) => a.key.localeCompare(b.key))) {
    console.log(`  ${f.key}  ${f.ruleId}${f.cross ? "  [cross-layer]" : ""}`);
  }
}

// Nonzero when the corpus is not perfectly modeled: a benchmark whose FN/FP lists are non-empty is
// a finding about the engine OR about the labels, and either way it is not a pass.
process.exit(fn.length || fp.length ? 1 : 0);
