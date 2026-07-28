#!/usr/bin/env node
// benchmark.mjs — score a snapshot against a labeled detection benchmark's ground truth, and
// (with --write-expected) regenerate that ground truth from a verified run.
//
// WIRED INTO CI since 2026-07-28, through `scripts/measure/detection-gate.sh` (the `detection-benchmark`
// job) — this script's exit code IS that gate's verdict. It stays runnable by hand with the same
// arguments; the gate only supplies them. The sentence that stood here until then said the opposite
// ("NOT A GUARD, AND NOT WIRED INTO CI"), and it was accurate for two years' worth of commits: a
// 298-line adjudicated ground truth sat in the tree with nothing comparing anything to it.
//
// What finally unblocked it was not cost — it was the one fixture that cannot be committed. See the
// gate script's header: the analyzer honors .gitignore, so the corpus is copied OUT of the repo and the
// fixture synthesized there. Scoring in place can never reach 143/143, and a gate that cannot be green
// is a gate nobody turns on.
//
// This replaces the corpus's own `run.mjs` / `measure.mjs` / `gen-expected.mjs`, which loaded the
// engine through a Node native addon and a JS CLI library that no longer exist — the last execution
// path in this repo that ran analysis through JavaScript. Everything now goes through the shipped
// binary: snapshot.mjs spawns it and validates every reply, and this script only reads the snapshot
// it wrote. That split is deliberate — scoring cannot accidentally measure something the snapshot
// contract already refused (an empty reply, a truncated anchor list, a missing axis).
//
// GROUND-TRUTH KEY FORMAT: "<sourceId>/<tree-relative path>:<line>" -> [ruleId, ...], plus a
// "benign" array of "<sourceId>/<path>" control files where ANY finding counts as a false positive,
// plus an "untracked" array of "<sourceId>/<path>" fixtures deliberately absent from the tracked tree.
// A false negative anchored in an `untracked` file is PRINTED WITH THAT REASON attached, because the
// alternative — a red score, two rule ids and no cause — sends the reader to suspect the engine for a
// file the repository chose not to ship. Scoring is unchanged: the miss still counts and still exits
// nonzero, it just stops being anonymous. The list lives in the ground truth rather than here on
// purpose: a path hardcoded in this script is a second copy that rots the first time the fixture moves,
// and the ground truth is the file that already names every anchor. `.gitignore` remains the authority
// on WHY each one is out of tree; neither file re-derives the other's reasoning.
// A cross-layer finding whose payload names no tree for its anchor is keyed under "<unattributed>/"
// and LISTED on stdout with the reason — see `attributeCross` below for the fallback chain.
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
// ---- cross-layer tree attribution ---------------------------------------------------------------
// A cross-layer finding's key needs the tree its ANCHOR (file:line) lives in. Only the single-tree
// cross-layer rules put that in `data.source`; the RELATIONAL ones (method-mismatch, version-skew,
// duplicate-route, route-shadowing, ambiguous-consume, path-near-miss, db-table-name-in-multiple-sources, external-*)
// name it elsewhere or not at all. Bucketing all of them under one `<cross>` prefix reinstated exactly
// the key collision the sourceId prefix exists to prevent: `provides.ts` exists in both `xbe` and
// `xbe2`, and only the differing line numbers kept those keys apart. So: a documented fallback chain,
// and anything it cannot resolve stays EXPLICITLY unattributed (and is printed), never silently pooled.
const UNATTRIBUTED = "<unattributed>";

// A site entry is either `{source, file, line}` (external-host-in-multiple-sources, external-base-url-drift)
// or the string `"<source>:<file>:<line>"` (duplicate-route). Returns {source, file, line} or null.
function parseSite(s) {
  if (s && typeof s === "object") {
    if (typeof s.source === "string" && typeof s.file === "string") {
      return { source: s.source, file: norm(s.file), line: Number(s.line) || 0 };
    }
    return null;
  }
  if (typeof s !== "string") return null;
  const m = /^([^:]+):(.+):(\d+)$/.exec(s);
  return m ? { source: m[1], file: norm(m[2]), line: Number(m[3]) } : null;
}

/**
 * Resolve the tree a cross-layer finding's anchor belongs to.
 *
 * THE CHAIN (each step must name the tree of the ANCHOR, never of the other side of a join):
 *   1. `data.source`         — the single-tree rules (unconsumed-endpoint, unconsumed-procedure, ...).
 *   2. `data.consumeSource`  — consume-anchored relational rules (method-mismatch, version-skew,
 *                              ambiguous-consume, path-near-miss, unprovided-mutation-call,
 *                              external-ip-literal, external-secret-in-url). These anchor on the
 *                              CONSUME site, which is what consumeSource names.
 *   3. `data.patternSource`  — route-shadowing anchors on the shadowing PATTERN's site.
 *   4. site array (`sites` / `exampleSites`) entry whose file+line EQUAL this finding's own anchor.
 *      Exact, no inference — used before any positional guess.
 *   5. `sources[0]` / `consumeSources[0]`. Sound by construction for this rule family, not by luck:
 *      each of these rules sorts its site list by (source, file, line), anchors the finding at
 *      `sites[0]`, and emits the sorted DISTINCT source set — so element 0 of that set is the anchor
 *      site's source. Verified in rules/native/rules-cross-layer/src/cross_layer/{shared_db_table.rs,
 *      duplicate_route.rs,external_duplicated_integration.rs,external_version_inconsistent.rs}.
 *      Step 4 runs first precisely so this invariant is only leaned on when nothing exact is available,
 *      and when both are available they are CROSS-CHECKED (a disagreement means the invariant broke
 *      upstream, so the finding is reported unattributed rather than keyed on a stale assumption).
 *
 * Anything that resolves to a sourceId this run did not measure is rejected — a typo'd or foreign
 * source must not mint a plausible-looking key.
 */
function attributeCross(f) {
  const d = f.data || {};
  const anchorFile = f.file ? norm(f.file) : null;
  const anchorLine = f.line || 0;
  const ok = (v) => (typeof v === "string" && sourceIds.has(v) ? v : null);

  const direct = ok(d.source) || ok(d.consumeSource) || ok(d.patternSource);

  let byAnchor = null;
  for (const field of ["sites", "exampleSites", "exampleFiles"]) {
    if (!Array.isArray(d[field])) continue;
    for (const raw of d[field]) {
      const site = parseSite(raw);
      if (site && site.file === anchorFile && site.line === anchorLine && ok(site.source)) {
        byAnchor = site.source;
        break;
      }
    }
    if (byAnchor) break;
  }

  let byList = null;
  for (const field of ["sources", "consumeSources"]) {
    if (Array.isArray(d[field]) && d[field].length) {
      byList = ok(d[field][0]);
      if (byList) break;
    }
  }

  if (direct) return { source: direct, why: "data.source/consumeSource/patternSource" };
  if (byAnchor && byList && byAnchor !== byList) {
    return {
      source: null,
      why: `site array says '${byAnchor}' but sources[0] says '${byList}' — the sort-by-source anchor invariant no longer holds`,
    };
  }
  if (byAnchor) return { source: byAnchor, why: "site entry matching the anchor file:line" };
  if (byList) return { source: byList, why: "sources[0] (sorted distinct source set; anchor is sites[0])" };
  return { source: null, why: "payload carries no field naming the anchor's tree" };
}

const unattributedCross = [];
for (const f of (cross.crossLayerFindings || {}).shown || []) {
  const { source, why } = attributeCross(f);
  const file = f.file ? norm(f.file) : `(no-file)/${f.ruleId}`;
  const key = `${source ? norm(source) : UNATTRIBUTED}/${file}:${f.line || 0}`;
  if (!source) unattributedCross.push({ key, ruleId: f.ruleId, why });
  findings.push({ key, ruleId: f.ruleId, cross: true });
}

// The score must never read as "everything was attributed". This prints even when the run is clean,
// and it names the residual collision risk on exactly the keys that still carry it.
if (unattributedCross.length) {
  console.log(
    `\n!! ${unattributedCross.length} cross-layer finding(s) NOT attributable to a tree — keyed under "${UNATTRIBUTED}/".`
  );
  console.log("   Those keys carry the collision risk the sourceId prefix exists to remove.");
  for (const u of unattributedCross.sort((a, b) => (a.key + a.ruleId).localeCompare(b.key + b.ruleId))) {
    console.log(`     ${u.key}  ${u.ruleId}\n       reason: ${u.why}`);
  }
  const seen = new Map();
  for (const u of unattributedCross) {
    const id = `${u.key}|${u.ruleId}`;
    seen.set(id, (seen.get(id) || 0) + 1);
  }
  const collided = [...seen].filter(([, n]) => n > 1);
  if (collided.length) {
    console.log("   !! COLLISION AMONG UNATTRIBUTED KEYS — distinct findings share one key/rule pair:");
    for (const [id, n] of collided) console.log(`      ${id}  x${n}`);
  }
  console.log("");
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
  // Carry the existing `benign` array across. It is hand-curated whole-file controls (36 of them as of
  // 2026-07-26), and regeneration used to emit a literal `"benign": []` — which would silently delete the
  // corpus's entire precision axis at the exact moment someone is least likely to be reading the diff.
  // Regeneration is already discouraged in this file's header; it must at minimum not destroy data it
  // cannot reconstruct.
  // `untracked` is carried across for the same reason and by the same rule: no run can re-derive it
  // (a fixture that is not on disk simply never fires, which is indistinguishable from a rule that
  // went quiet), so regeneration must not be the thing that deletes it.
  let carriedBenign = [];
  let carriedUntracked = [];
  if (fs.existsSync(expectedPath)) {
    try {
      const prev = JSON.parse(stripJsonComments(fs.readFileSync(expectedPath, "utf8")));
      carriedBenign = prev.benign || [];
      carriedUntracked = prev.untracked || [];
    } catch {
      fail(
        `refusing to regenerate: ${expectedPath} exists but could not be parsed, so its hand-curated\n` +
          "  `benign` control list and `untracked` disclosure cannot be carried across. Fix or move the\n" +
          "  file first."
      );
    }
  }
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
    `  // CARRIED OVER from the previous file — regeneration never re-derives these (they are the paths\n` +
    `  // where nothing fired, which no run can tell apart from paths nobody thought to control).\n` +
    `  "benign": ${JSON.stringify(carriedBenign, null, 2).split("\n").join("\n  ")},\n\n` +
    `  // Fixtures deliberately absent from the tracked tree; a FN anchored in one is printed with that\n` +
    `  // reason. CARRIED OVER for the same reason as \`benign\` — a file that is not on disk never fires,\n` +
    `  // which no run can tell apart from a rule that went quiet. See .gitignore for why each is out.\n` +
    `  "untracked": ${JSON.stringify(carriedUntracked, null, 2).split("\n").join("\n  ")}\n` +
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
const untracked = new Set((expectedRaw.untracked || []).map(norm));
const expected = new Map();
for (const [k, v] of Object.entries(expectedRaw)) {
  if (k === "benign" || k === "untracked") continue;
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
  // A FN anchored in an `untracked` fixture is announced AT THE FN, not only in the file that caused
  // it. The failure mode this closes was measured: a fresh clone prints two rule ids under a red score
  // with no cause on screen, and the reader goes looking for an engine regression that is not there.
  console.log("-- FN (expected, did not fire) --");
  const untrackedHits = new Map();
  for (const x of fn.sort()) {
    const fileOnly = x.split(" ")[0].replace(/:\d+$/, "");
    if (untracked.has(fileOnly)) {
      untrackedHits.set(fileOnly, (untrackedHits.get(fileOnly) || 0) + 1);
      console.log(`  ${x}   <- fixture is not in the tracked tree; see below`);
    } else {
      console.log("  " + x);
    }
  }
  const untrackedTotal = [...untrackedHits.values()].reduce((a, b) => a + b, 0);
  if (untrackedTotal) {
    console.log(
      `\n   !! ${untrackedTotal} of the ${fn.length} FN above are anchored in fixtures DELIBERATELY kept out of\n` +
        "      the tracked tree (EXPECTED.jsonc's `untracked` list), so these misses are a CHECKOUT STATE and\n" +
        "      not an engine regression. Note this is NOT only about a fresh clone: the analyzer honors\n" +
        "      .gitignore (crates/engine/src/pipeline/walking.rs), so an excluded fixture stays invisible even\n" +
        "      when you recreate it at that path. Which files, and why each is excluded, are in .gitignore:"
    );
    for (const [f, n] of [...untrackedHits].sort()) console.log(`        ${f}   (${n} expectation(s))`);
    if (untrackedTotal < fn.length) {
      console.log("      The remaining FN are NOT explained by this — treat them as engine misses or bad labels.");
    }
    console.log("");
  }
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
