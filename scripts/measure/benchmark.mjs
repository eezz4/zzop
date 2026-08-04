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
// PROVING IT (2026-07-29): every `fail(` below is exercised by `scripts/measure/harness-selftest.sh`
// phase 2, which builds a known-good scoring sandbox (`selftest-benchmark-cases.mjs`) and damages the
// run directory, the ground truth and the coverage floor one way at a time, requiring a named abort
// for each. Until then this file had ONE tested path — the happy one — while being the gate's verdict,
// and the first run of that phase found six branches that did not fire as claimed: four exited only
// through an uncaught Node throw, and two SWALLOWED the damage as an empty anchor list and reported a
// corrupted snapshot as an engine regression. Their fixes are commented at the sites below. A scorer
// whose failure paths have never been seen fire is where a malfunction becomes a green result.
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
// plus a "gap" object of the same "<key>": [ruleId] shape as an expectation (see below),
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
// ---- THE THIRD DISPOSITION: `gap` -----------------------------------------------------------------
// Until 2026-07-29 this scorer had exactly two dispositions, and neither one could say "correct, not
// implemented yet":
//   * an expectation — absent means FN, which means RED, forever, until the engine closes it;
//   * a `benign` control — present means FP, i.e. "no finding here is the right answer".
// `untracked` is not a third: it annotates a FN's CAUSE and the miss still counts and still exits
// nonzero. So a shape the engine is documented as refusing to handle had two spellings, and both lie.
// As an expectation it makes the gate permanently red for something nobody intends to fix this week,
// which teaches everyone to ignore the gate and blocks every push. As `benign` it FREEZES TODAY'S
// INCAPACITY AS THE CORRECT ANSWER, which is the exact reason those cases were added.
//
// `gap` is the third: "this SHOULD fire once a known capability lands, and today it does not."
//   * a gap key that does NOT fire  -> not an FN. Untouched recall, untouched precision, EXIT 0.
//   * a gap key that DOES fire      -> not an FP either (the label already says the finding is correct;
//     scoring it as a false positive would report an expected capability ARRIVING as a precision
//     regression) — instead the run exits NONZERO and names what to promote. Progress is re-adjudicated
//     by hand into an ordinary expectation, never absorbed: a benchmark that cannot see a capability
//     GAIN has stopped measuring, and it would go on printing `GAP 0/N` while N were already closed.
//   * the count prints on its own line in EVERY run (`GAP 0/2 closed`), green or red, so the size of
//     the acknowledged hole is a number on screen rather than a paragraph in a README.
// The coverage floor carries a third column for the same reason it carries the first two — see the
// ratchet header. A gap is the ONE labeled shape that costs nothing to hold and nothing to delete
// (it is exit-zero while open), so without a floor its disappearance would be completely invisible:
// measured, deleting one from the corpus scored `TP 2 FN 0 FP 0`, 100%/100%, exit 0.
//
// What `gap` is NOT: a suppression, and not a place to park a false negative that has no named cause.
// The corpus entry that claims one is expected to name the capability, the code that refuses it, and
// what would close it (see cases/EXPECTED.jsonc's negative-case block).
//
// usage:
//   node scripts/measure/benchmark.mjs --run <runs-dir/label> --expected <EXPECTED.jsonc> [--dump]
//   node scripts/measure/benchmark.mjs --run <runs-dir/label> --expected <EXPECTED.jsonc> --write-expected
//   node scripts/measure/benchmark.mjs --expected <EXPECTED.jsonc> --update-baseline
//
// Regenerate ONLY after confirming with --dump that every `good` example in the corpus is silent:
// --write-expected locks whatever fired as the truth, including any false positive. Since 2026-07-29 it
// can no longer lock a SMALLER truth — see "the expectation ratchet" below.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

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
const dump = argv.includes("--dump");
const writeExpected = argv.includes("--write-expected");
const updateBaseline = argv.includes("--update-baseline");
const bootstrapBaseline = argv.includes("--bootstrap-baseline");
// Baseline maintenance reads the ground truth and nothing else, so it must not demand a snapshot: making
// `--run` mandatory there would force whoever records a legitimate growth to first produce a run they do
// not use, and a guard that asks for pointless work is a guard people route around.
const baselineOnly = updateBaseline || bootstrapBaseline;
const expectedPath = path.resolve(arg("expected"));
const runDir = baselineOnly ? null : path.resolve(arg("run"));

// These two live here rather than beside the code that uses them because the expectation ratchet (see its
// header further down) needs both, and the ratchet's maintenance mode runs before any snapshot is read.
// `const` does not hoist the way a function declaration does, so declaring them below would leave them in
// the temporal dead zone at the call on the next line — measured, not assumed.
const norm = (s) => String(s).replace(/\\/g, "/").replace(/\/+/g, "/");
const BASELINE = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "detection-expected-baseline.txt");
// The baseline's three counted fields, in column order — named once so every loop over them stays in
// step with the file's shape. It lives HERE for the reason stated just above and not next to the ratchet
// code that reads it: `diffAgainstBaseline` is reached from `runBaselineMaintenance` on the line below,
// and a `const` declared further down is in the temporal dead zone at that point. Measured 2026-07-29
// while adding the `gap` column — `ReferenceError: Cannot access 'FIELDS' before initialization`, an
// uncaught throw where an abort belonged, in the one mode (`--update-baseline` lowering the floor) that
// reaches the diff before any snapshot is read.
const FIELDS = ["expectations", "benign", "gap"];

// ---- the tree registry ----------------------------------------------------------------------------
// The corpus's own `zzop.config.jsonc`, beside the ground truth. It is the file that decides which
// trees a run measures, so it — never EXPECTED.jsonc's own key prefixes — is where the coverage
// floor's subject set comes from. Until 2026-08-03 `countsOf` derived the subjects from the ground
// truth's keys, so a tree with ZERO labels (depedge, bearticles, beinvoices — the negative-case
// backends and the dep-graph tree) had NO ROW in the floor at all: it sat outside the only ratchet
// built to notice the corpus shrinking, and deleting it from the registry was silent. Measured before
// this change: the committed floor held 20 rows over a 23-tree registry. Seeding every registered
// tree with a zero row puts those trees ON the floor (`<tree> 0 0 0`), and a floor row whose tree has
// left the registry goes red BY NAME in `diffAgainstBaseline`. It also gives `assertBenignOnDisk` the
// sourceId -> tree-root mapping it needs to resolve a control path to a real file.
function loadRegistry() {
  const p = path.join(path.dirname(expectedPath), "zzop.config.jsonc");
  if (!fs.existsSync(p)) {
    fail(
      `no tree registry at ${p} — the corpus config is what names the trees this benchmark claims to\n` +
        "  measure, and the coverage floor's subject set is derived from it (a ground truth's own keys\n" +
        "  cannot see a tree with zero labels)."
    );
  }
  let doc;
  try {
    doc = JSON.parse(stripJsonComments(fs.readFileSync(p, "utf8")));
  } catch (e) {
    fail(`tree registry ${p} could not be parsed: ${e.message}\n  The floor's subject set comes from this file; guessing it would seed the wrong floor.`);
  }
  const bad = [];
  const trees = new Map();
  for (const t of Array.isArray(doc.trees) ? doc.trees : []) {
    if (!t || typeof t.sourceId !== "string" || typeof t.root !== "string") {
      bad.push(JSON.stringify(t));
      continue;
    }
    trees.set(t.sourceId, path.resolve(path.dirname(p), t.root));
  }
  if (bad.length || trees.size === 0) {
    fail(
      `tree registry ${p} is not a usable trees[] list` +
        (bad.length ? `: ${bad.length} entr(y/ies) lack root/sourceId —\n    ${bad.join("\n    ")}` : " — it names ZERO trees") +
        "\n  A dropped entry is a tree off the coverage floor, so it is refused rather than skipped."
    );
  }
  return trees;
}
const registry = loadRegistry(); // Map<sourceId, absolute tree root>

// ---- benign controls must exist on disk -----------------------------------------------------------
// A `benign` entry whose file is not there is not "quiet" — it is UNMEASURED. The control still lives
// in the benign count (and in the decoy tree's precision claim) while a walker that never sees the
// file can never fire in it, so a typo'd or deleted control keeps asserting "no false positive here"
// about nothing, forever. Runs BEFORE any score or floor write: scoring first would print a precision
// number that silently includes ghosts, and `--update-baseline` would freeze them into the floor.
// Resolved through the registry (sourceId prefix -> tree root), the same mapping the keys are built on.
function assertBenignOnDisk(benignList) {
  const ghosts = [];
  for (const b of benignList || []) {
    const nb = norm(b);
    const slash = nb.indexOf("/");
    const sid = slash === -1 ? nb : nb.slice(0, slash);
    const rel = slash === -1 ? "" : nb.slice(slash + 1);
    const root = registry.get(sid);
    if (!root) {
      ghosts.push(`${b}  (sourceId '${sid}' is not in the tree registry)`);
    } else if (!rel || !fs.existsSync(path.join(root, rel))) {
      ghosts.push(`${b}  (nothing on disk at ${path.join(root, rel)})`);
    }
  }
  if (ghosts.length) {
    fail(
      `${ghosts.length} benign control(s) in the ground truth name files that DO NOT EXIST:\n    ` +
        ghosts.join("\n    ") +
        "\n\n  A control file that is not on disk is never walked, so it asserts nothing while still living\n" +
        "  in the benign count. Fix the path, or delete the label and lower the floor's benign column by\n" +
        "  hand with the reason in the diff — the ratchet refusing the silent version is it doing its job."
    );
  }
}

if (baselineOnly) runBaselineMaintenance(); // never returns

if (!fs.existsSync(path.join(runDir, "meta.json"))) {
  fail(`no meta.json in ${runDir} — point --run at a directory written by snapshot.mjs.`);
}

// Every read of the snapshot goes through here. A raw `JSON.parse(fs.readFileSync(...))` still exits
// nonzero when the file is truncated — Node throws — but it exits with a stack trace naming `node:fs`
// and nothing naming the run directory, and this script's contract is that a bad measurement is
// DIAGNOSED, not merely fatal. Measured 2026-07-29 by truncating meta.json: `SyntaxError: Unexpected
// end of JSON input at benchmark.mjs:94`, no BENCHMARK ABORT. The two other places in this file that
// read a JSON file (`--write-expected`, `--update-baseline`) already wrapped theirs; the score path —
// the one CI runs and whose exit code is the gate's verdict — was the one that did not.
function readSnapshotJson(name) {
  const p = path.join(runDir, name);
  if (!fs.existsSync(p)) {
    fail(`${runDir} is missing ${name} — an incomplete snapshot directory, not a run. Re-take it with snapshot.mjs.`);
  }
  try {
    return JSON.parse(fs.readFileSync(p, "utf8"));
  } catch (e) {
    fail(`${p} could not be parsed: ${e.message}\n  A truncated or hand-edited snapshot cannot be scored.`);
  }
}

// ---- read the snapshot (never the binary — snapshot.mjs already validated the binary's replies) ---
const meta = readSnapshotJson("meta.json");
const cross = readSnapshotJson("cross.json");

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

// An absent or empty tree list is a broken read, never a corpus with no trees: snapshot.mjs refuses to
// write a run whose cross_repo reply returned 0 sources, and meta.trees is derived from that list. The
// same "an empty subject set is a broken guard, not a clean tree" rule enforceRatchet applies below.
if (!Array.isArray(meta.trees) || meta.trees.length === 0) {
  fail(
    `meta.json in ${runDir} carries no \`trees\` — that is an unreadable snapshot, not a corpus with\n` +
      "  nothing in it. Every per-tree expectation would score as a false negative against the engine."
  );
}

const sourceIds = new Set(meta.trees.map((t) => t.sourceId));

const findings = [];
for (const t of meta.trees) {
  const a = readSnapshotJson(t.file);
  // `a.findings.shown || []` used to absorb a damaged payload as an EMPTY anchor list, and an empty
  // anchor list is not an error anywhere downstream — it is a full sweep of false negatives, i.e. the
  // score reports a corrupted snapshot as an ENGINE REGRESSION. Measured 2026-07-29 with a tree file
  // reduced to `{"findings":{"total":1}}`: `TP 1  FN 1`, red for the wrong reason and naming a rule
  // that never failed. The equality below is a tautology on anything snapshot.mjs wrote (its contract 5
  // refuses to write shown.length != total), so it can only fire on a directory damaged afterwards.
  const shown = a && a.findings ? a.findings.shown : undefined;
  if (!Array.isArray(shown)) {
    fail(
      `${t.file}: no \`findings.shown\` array for tree '${t.sourceId}'. An absent anchor list scores as a\n` +
        "  full sweep of false negatives, so it is refused rather than counted as zero findings."
    );
  }
  if (typeof t.total === "number" && shown.length !== t.total) {
    fail(
      `${t.file}: tree '${t.sourceId}' carries ${shown.length} anchor(s) but meta.json recorded ${t.total}.\n` +
        "  The snapshot disagrees with itself; scoring it would report the difference as recall."
    );
  }
  for (const f of shown) {
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
 *                              CONSUME site, which is what consumeSource names. Also the per-source
 *                              copies of shared-db-table and external-host-in-multiple-sources.
 *   2b. `data.provideSource` — the same thing on the provide side: duplicate-route's per-source copies.
 *   3. `data.patternSource`  — route-shadowing anchors on the shadowing PATTERN's site.
 *   4. site array (`sites` / `exampleSites`) entry whose file+line EQUAL this finding's own anchor.
 *      Exact, no inference — used before any positional guess.
 *   5. `sources[0]` / `consumeSources[0]`. Sound by construction for the rules that still need it, not
 *      by luck: the rule sorts its site list by (source, file, line), anchors the finding at `sites[0]`,
 *      and emits the sorted DISTINCT source set — so element 0 of that set is the anchor site's source.
 *      Verified in rules/native/rules-cross-layer/src/cross_layer/external_version_inconsistent.rs, now
 *      the only member of the family that leans on it: shared-db-table, duplicate-route and
 *      external-host-in-multiple-sources emit ONE COPY PER PARTICIPATING SOURCE (2026-07-29), each
 *      anchored in its OWN tree, so `sources[0]` names the anchor's tree for exactly one of the copies
 *      and steps 2/2b carry the answer for all of them. The cross-check below caught that transition
 *      rather than mis-keying four findings — which is the check earning its place.
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

  const direct =
    ok(d.source) || ok(d.consumeSource) || ok(d.provideSource) || ok(d.patternSource);

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

// Same rule as the per-tree anchor list, and the same failure shape one axis over: an absent
// `crossLayerFindings` silently contributed zero cross-layer findings, and every `cross-layer/*`
// expectation was then reported as a miss. snapshot.mjs refuses to write a cross_repo payload without
// that key, so its absence here means the file was damaged afterwards — measured 2026-07-29 by
// deleting it: `TP 1  FN 1`, blaming a cross-layer rule that had fired.
if (!cross.crossLayerFindings || !Array.isArray(cross.crossLayerFindings.shown)) {
  fail(
    "cross.json has no `crossLayerFindings.shown` array. Every cross-layer/* rule exists ONLY in that\n" +
      "  reply, so counting it as zero would report the entire axis as false negatives."
  );
}

const unattributedCross = [];
for (const f of cross.crossLayerFindings.shown) {
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

// ---- the expectation ratchet ----------------------------------------------------------------------
// The ground truth IS the release gate (`.github/workflows/ci.yml`'s detection-benchmark job, reached
// from prebuild.yml since v0.25.0), and until 2026-07-29 one command rewrote it. Measured on this tree,
// not reasoned about: turning `cross-layer/unconsumed-endpoint` off in the corpus config took the gate to
// `TP 127 FN 16 FP 0`, exit 1 — and `--write-expected` then rewrote EXPECTED.jsonc from 143 expectations
// to 127 (17 insertions, 154 deletions, the whole adjudication record gone with them) and the gate came
// back `TP 127 FN 0 FP 0`, exit 0. The sentence at the top of this file — "--write-expected locks
// whatever fired as the truth, including any false positive" — described that hole accurately for two
// years and did nothing about it. A benchmark whose answer key is rewritten by the thing under test
// measures nothing.
//
// So the COUNT of what the corpus claims is frozen per tree in a committed file, and this script refuses
// to move it in the loosening direction:
//   - scoring AND `--write-expected` both check the derived counts against that file;
//   - `--update-baseline` rewrites it but REFUSES to lower any number or drop any row;
//   - lowering therefore takes a HAND EDIT of the baseline, in a diff that states what was given up.
// That last line is the whole design. Retiring an expectation must stay possible — EXPECTED.jsonc's own
// header records two retired legitimately in v0.24.0, when a generic rule was replaced by a strict
// specialization — it must only stop being possible AS A SIDE EFFECT of a command that reads as routine
// maintenance.
//
// ## Why this is not a second enumeration
// This repo deleted `ZZOP_CHECK_DOCS_RULE_IDS_FILES` on 2026-07-28 for being a hand-kept list that
// silently stopped covering whatever it forgot. The distinction is FAIL-OPEN vs FAIL-CLOSED, not "is
// there a second file":
//   - the baseline names nothing of its own. Every sourceId in it comes from EXPECTED.jsonc's own keys
//     or from the corpus registry's trees[] (since 2026-08-03 — the registry is what lets a tree with
//     ZERO labels have a row at all), and every number is produced by `countsOf`, the same code the
//     score path calls. It is the fixed point of a derivation, not an independently authored list —
//     there is nothing in it to forget;
//   - the check is EQUALITY in both directions. A row too low is a surrendered detection; a row too high,
//     or missing, is an unrecorded one. Both are red. There is no state in which the baseline and
//     EXPECTED.jsonc quietly disagree, which is precisely what a fail-open list is for.
// What the baseline supplies that EXPECTED.jsonc cannot is the one fact no file holds about itself: its
// PREVIOUS value. A ratchet needs a floor stored somewhere the regeneration does not write.
//
// ## What is counted, and what is not
// Expectations are counted as key x ruleId PAIRS, not locations — dropping one rule id from a key that
// keeps its others is a lost detection, and a location count would not see it. `benign` is counted too:
// it is the entire precision axis, and deleting a control removes FP pressure exactly the way deleting an
// expectation removes recall pressure. `gap` is counted for a STRONGER reason than either (added
// 2026-07-29 with the disposition itself): an expectation or a control at least defends itself in the
// score — delete one and the number moves. A gap is exit-zero while open, so the score is IDENTICAL
// whether it is there or not, and this column is the only thing that notices its removal at all.
// Measured before the column existed: deleting a gap from the ground truth scored `TP 2 FN 0 FP 0`,
// recall 100.0%, precision 100.0%, exit 0. `untracked` is NOT counted — it changes only the EXPLANATION
// printed beside a false negative (the miss still counts and still exits nonzero, see the FN block
// below), so it is not a laundering surface and freezing it would be friction buying no invariant.
//
// ## Rejected: hashing the ground truth
// A hash pins content exactly, which sounds strictly stronger. It has no DIRECTION: with a hash,
// `--update-baseline` cannot tell a surrendered expectation from a re-keyed one, so every line-number
// shift in the corpus becomes the same "confirm this" prompt as a deletion, and the ratchet degenerates
// into a rubber stamp nobody reads. Counts keep the asymmetry the guard exists for.
//
// ## Known residual, deliberately not closed
// Deleting the baseline and re-running with `--bootstrap-baseline` resets the floor. That is two
// deliberate acts, one of them the deletion of a tracked file, and both land in the same diff as the
// shrunken EXPECTED.jsonc. What is bought here is not "unreachable" — it is "not reachable by accident
// and not reachable silently". The one-command laundering measured above is gone.
//
// Ratchet shape, the shrink-only `--update-baseline` contract and the baseline-file conventions are taken
// from scripts/check-max-file-lines.sh deliberately: one ratchet dialect in this repo, not two. The
// direction is mirrored because the subject is — that guard freezes DEBT and lets it shrink, this one
// freezes COVERAGE and lets it grow.
//
// The floor's path (`BASELINE`) is declared with the argv block at the top of this file, not here — see
// the note there for why.

/**
 * `gap` map from a ground-truth document: the same "<key>": [ruleId, ...] shape as an expectation, in
 * its own top-level object. Absent means an empty map — a corpus with no acknowledged gaps is an
 * ordinary state, unlike an empty expectation set.
 */
function gapOf(doc) {
  const m = new Map();
  for (const [k, v] of Object.entries(doc.gap || {})) m.set(norm(k), new Set(v));
  return m;
}

/**
 * The key x ruleId pairs in `gapMap` that this run's findings ACTUALLY produced — i.e. gaps that have
 * closed. Shared by the score path and by `--write-expected`, so both refuse a silent promotion for the
 * same reason and with the same list. Reads the module-level `findings`, which the baseline-maintenance
 * modes never reach (they exit before any snapshot is read).
 */
function firedGaps(gapMap) {
  const out = [];
  for (const f of findings) {
    const g = gapMap.get(f.key);
    if (g && g.has(f.ruleId)) out.push(`${f.key} ${f.ruleId}`);
  }
  return out.sort();
}

/**
 * Per-sourceId counts derived from a ground truth. `expectations` and `gapMap` are Map<key, Set<ruleId>>
 * (the same shape both the score path and `writeGroundTruth` already build) and `benignList` the raw
 * `benign` array. The sourceId is the key prefix — the same prefix the rest of this file treats as
 * load-bearing.
 *
 * SEEDED from the tree registry first: every registered tree gets a row even when nothing in the
 * ground truth anchors there, so a zero-label tree's floor row is `<sourceId> 0 0 0` — the row's
 * EXISTENCE is what the ratchet defends for those trees, because there is no count to defend.
 */
function countsOf(expectations, benignList, gapMap) {
  const out = new Map();
  for (const id of registry.keys()) out.set(id, { expectations: 0, benign: 0, gap: 0 });
  const bump = (id, field, n) => {
    if (!out.has(id)) out.set(id, { expectations: 0, benign: 0, gap: 0 });
    out.get(id)[field] += n;
  };
  for (const [key, rules] of expectations) bump(norm(key).split("/")[0], "expectations", rules.size);
  for (const b of benignList || []) bump(norm(b).split("/")[0], "benign", 1);
  for (const [key, rules] of gapMap || new Map()) bump(norm(key).split("/")[0], "gap", rules.size);
  return out;
}

function readBaseline() {
  const rows = new Map();
  for (const line of fs.readFileSync(BASELINE, "utf8").split(/\r?\n/)) {
    const t = line.trim();
    if (!t || t.startsWith("#")) continue;
    const cols = t.split(/\s+/);
    const [id] = cols;
    const nums = cols.slice(1, 4);
    // A row this parser cannot read is not skipped: a silently dropped row is a floor of zero for that
    // tree, i.e. the guard's own failure mode is the thing it exists to prevent. The `gap` column is
    // REQUIRED rather than defaulted to 0 for the same reason — a two-column row left readable would
    // give every pre-existing tree a permanent gap floor of zero, which is the one value that lets a
    // gap be deleted silently, and this column exists precisely to stop that.
    if (!id || nums.length !== 3 || !nums.every((n) => /^\d+$/.test(n))) {
      fail(`${BASELINE}: unreadable row ${JSON.stringify(line)}\n  expected "<sourceId> <expectations> <benign> <gap>".`);
    }
    rows.set(id, { expectations: Number(nums[0]), benign: Number(nums[1]), gap: Number(nums[2]) });
  }
  return rows;
}

function renderBaseline(counts) {
  const rows = [...counts.keys()].sort().map((id) => {
    const c = counts.get(id);
    return `${id} ${c.expectations} ${c.benign} ${c.gap}`;
  });
  return (
    "# Detection-benchmark COVERAGE FLOOR — how much cases/EXPECTED.jsonc is required to claim, per tree.\n" +
    "# This is not debt and not an exemption list: it is the previous value of a file that regenerates\n" +
    "# itself, which is the one thing that file cannot hold about itself.\n" +
    "#\n" +
    "# Columns: <sourceId> <expectations> <benign> <gap>\n" +
    "#   expectations — labeled key x ruleId pairs anchored in that tree (NOT locations: dropping one rule\n" +
    "#                  id from a key that keeps its others is a lost detection).\n" +
    "#   benign       — whole-file negative controls in that tree, where any finding is a false positive.\n" +
    "#   gap          — labeled shapes that SHOULD fire once a known capability lands and do not today.\n" +
    "#                  Counted for a stronger reason than the other two: an expectation or a control\n" +
    "#                  defends itself in the score, but a gap is exit-zero while open, so the score is\n" +
    "#                  identical whether it is in the ground truth or not. This column is the only thing\n" +
    "#                  that notices one being deleted.\n" +
    "# `untracked` is deliberately not counted; see the ratchet header in scripts/measure/benchmark.mjs\n" +
    "# for that and for why a per-tree COUNT rather than a hash of the ground truth.\n" +
    "#\n" +
    "# Every number here is DERIVED from cases/EXPECTED.jsonc by the same code that scores it, and the\n" +
    "# check is equality in both directions — a number too low is a surrendered detection, a number too\n" +
    "# high or a missing row is an unrecorded one, and both are red. So this file cannot silently drift out\n" +
    "# of agreement with the ground truth the way a hand-kept list can.\n" +
    "#\n" +
    "# The ROW SET is seeded from the corpus registry (the zzop.config.jsonc beside the ground truth), not\n" +
    "# from the ground truth's own keys: a tree with zero labels still has its `<sourceId> 0 0 0` row, and\n" +
    "# for those trees the row's EXISTENCE is the whole floor — removing the tree from the registry goes\n" +
    "# red by name here instead of silently, which no count-only derivation could see.\n" +
    "#\n" +
    "# Maintained by: node scripts/measure/benchmark.mjs --expected cases/EXPECTED.jsonc --update-baseline\n" +
    "# That mode GROWS ONLY. Lowering a number, or removing a row, is a hand edit — on purpose, so that\n" +
    "# giving up a detection is an act with an author and a reason in the diff, never a side effect of\n" +
    "# regenerating the answer key against a degraded engine.\n" +
    rows.join("\n") +
    "\n"
  );
}

/** Rows where the ground truth now claims LESS than the floor, and rows the floor does not record yet. */
function diffAgainstBaseline(base, counts) {
  const shrunk = [];
  const grew = [];
  for (const id of [...new Set([...base.keys(), ...counts.keys()])].sort()) {
    const b = base.get(id);
    const c = counts.get(id);
    if (!b) {
      grew.push(
        `  NEW   ${id}: ${c.expectations} expectation(s), ${c.benign} benign control(s), ${c.gap} gap(s) — not in the floor yet`
      );
      continue;
    }
    // A floor row with NO derived counterpart is not a row of zeros — it is a tree that has left both
    // the ground truth and the registry. Defaulting it to zeros would make removing a ZERO-LABEL tree
    // from the registry silent (0 -> 0 shrinks nothing), and the zero-label trees are exactly the ones
    // whose only defense is this row's existence.
    if (!c) {
      shrunk.push(
        `  MISSING ${id}: in the committed floor but in neither the ground truth nor the tree registry\n` +
          `          (the zzop.config.jsonc beside it) — a tree removed from the registry is never scored at all,\n` +
          `          which is quieter than any false negative`
      );
      continue;
    }
    for (const field of FIELDS) {
      if (c[field] < b[field]) shrunk.push(`  SHRANK ${id}.${field}: ${b[field]} -> ${c[field]}`);
      else if (c[field] > b[field]) grew.push(`  GREW  ${id}.${field}: ${b[field]} -> ${c[field]}`);
    }
  }
  return { shrunk, grew };
}

/**
 * `counts` must agree with the committed floor exactly. `allowGrowth` is set only by --write-expected,
 * which is legitimately the command that ADDS coverage (a new case adjudicated into the corpus); the
 * growth is then reported and the very next scoring run stays red until --update-baseline records it.
 */
function enforceRatchet(counts, { allowGrowth = false, subject = "cases/EXPECTED.jsonc" } = {}) {
  // An empty derivation is a broken read, never a corpus with no expectations — the same "an empty
  // subject set is a broken guard, not a clean tree" rule check-guards-wired.sh and check-max-file-lines.sh
  // both had to learn. Without this, a ground truth reduced to zero keys would sail past a floor
  // comparison that has nothing to compare. Counted over CLAIMS, not rows, because registry seeding
  // means the row set can never be empty — a ground truth that contributes nothing to any seeded row
  // is the same broken read wearing 23 rows of zeros.
  const claims = [...counts.values()].reduce((a, c) => a + FIELDS.reduce((x, f) => x + c[f], 0), 0);
  if (counts.size === 0 || claims === 0) {
    fail(
      `expectation ratchet: derived ZERO claims from ${subject} — no expectations, benign controls or\n` +
        "  gaps anchored in any tree. That is a broken read, not a clean corpus."
    );
  }
  if (!fs.existsSync(BASELINE)) {
    fail(
      `expectation ratchet: missing ${BASELINE}.\n` +
        "  The floor is a committed file; without it nothing stops the ground truth from shrinking.\n" +
        "  If you are genuinely creating it for the first time:\n" +
        `    node scripts/measure/benchmark.mjs --expected ${subject} --bootstrap-baseline`
    );
  }
  const { shrunk, grew } = diffAgainstBaseline(readBaseline(), counts);
  if (shrunk.length) {
    fail(
      `expectation ratchet: ${subject} claims LESS than the committed floor.\n` +
        shrunk.join("\n") +
        "\n\n  A detection this corpus used to require no longer appears in it. That is either an engine\n" +
        "  regression (fix the engine) or a bad label (fix the label) — and if it is genuinely a retired\n" +
        "  expectation, lower the row in\n" +
        `    ${BASELINE}\n` +
        "  BY HAND, with the reason, in the same commit. --update-baseline will not do it for you: the\n" +
        "  whole point of this ratchet is that surrendering coverage has an author."
    );
  }
  if (grew.length && !allowGrowth) {
    fail(
      `expectation ratchet: ${subject} claims MORE than the committed floor, unrecorded.\n` +
        grew.join("\n") +
        "\n\n  Growth is welcome and is not a violation of the ratchet — it just has to be recorded, so that\n" +
        "  the new coverage becomes the new floor and cannot be lost silently later:\n" +
        `    node scripts/measure/benchmark.mjs --expected ${subject} --update-baseline`
    );
  }
  return grew;
}

/** `--update-baseline` / `--bootstrap-baseline`. Reads the ground truth, writes the floor, exits. */
function runBaselineMaintenance() {
  if (!fs.existsSync(expectedPath)) fail(`no ground truth at ${expectedPath}.`);
  let doc;
  try {
    doc = JSON.parse(stripJsonComments(fs.readFileSync(expectedPath, "utf8")));
  } catch (e) {
    fail(`${expectedPath} could not be parsed: ${e.message}`);
  }
  const expectations = new Map();
  for (const [k, v] of Object.entries(doc)) {
    if (k === "benign" || k === "untracked" || k === "gap") continue;
    expectations.set(norm(k), new Set(v));
  }
  assertBenignOnDisk(doc.benign);
  const counts = countsOf(expectations, doc.benign, gapOf(doc));
  const claims = [...counts.values()].reduce((a, c) => a + FIELDS.reduce((x, f) => x + c[f], 0), 0);
  if (counts.size === 0 || claims === 0) {
    fail(`expectation ratchet: derived ZERO claims from ${expectedPath} — refusing to write a floor of nothing.`);
  }

  const exists = fs.existsSync(BASELINE);
  if (bootstrapBaseline && exists) {
    fail(`--bootstrap-baseline: ${BASELINE} already exists. Use --update-baseline (which only grows).`);
  }
  if (updateBaseline && !exists) {
    fail(
      `--update-baseline: ${BASELINE} does not exist, so there is no floor to raise.\n` +
        "  Creating one from the current tree accepts whatever it currently says as the truth, which is the\n" +
        "  exact move this ratchet exists to make deliberate. If that is really what you mean, say so:\n" +
        `    node scripts/measure/benchmark.mjs --expected ${expectedPath} --bootstrap-baseline`
    );
  }
  if (exists) {
    // Same refusal shape as check-max-file-lines.sh's --update-baseline: the maintenance mode enforces
    // the ratchet's direction itself, so there is no flag or argument of this script that lowers a row.
    const { shrunk } = diffAgainstBaseline(readBaseline(), counts);
    if (shrunk.length) {
      fail(
        "--update-baseline: refusing to LOWER the floor.\n" +
          shrunk.join("\n") +
          "\n\n  Restore the expectations, or lower those rows by hand with the reason in the diff."
      );
    }
  }
  fs.writeFileSync(BASELINE, renderBaseline(counts));
  const total = [...counts.values()].reduce((a, c) => a + c.expectations, 0);
  console.log(`expectation ratchet: floor ${exists ? "updated" : "bootstrapped"} (${counts.size} trees, ${total} expectations) -> ${BASELINE}`);
  process.exit(0);
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
  // `gap` is carried across for the third time by the same rule: no run can re-derive it either. A gap
  // is by definition a shape that DOES NOT FIRE, so regeneration sees exactly nothing where one is —
  // indistinguishable from a location nobody has adjudicated.
  let carriedBenign = [];
  let carriedUntracked = [];
  let carriedGap = {};
  if (fs.existsSync(expectedPath)) {
    try {
      const prev = JSON.parse(stripJsonComments(fs.readFileSync(expectedPath, "utf8")));
      carriedBenign = prev.benign || [];
      carriedUntracked = prev.untracked || [];
      carriedGap = prev.gap || {};
    } catch {
      fail(
        `refusing to regenerate: ${expectedPath} exists but could not be parsed, so its hand-curated\n` +
          "  `benign` control list, `gap` adjudications and `untracked` disclosure cannot be carried\n" +
          "  across. Fix or move the file first."
      );
    }
  }

  // Ghost controls stop regeneration too: `benign` is carried across verbatim, so writing here would
  // re-commit a control that asserts nothing (see assertBenignOnDisk) into a fresh-looking file.
  assertBenignOnDisk(carriedBenign);

  // A fired gap stops regeneration too, and not only scoring. Without this, `--write-expected` is a
  // ONE-COMMAND bypass of the promotion refusal below: it would emit the fired finding as an ordinary
  // expectation while ALSO carrying it in `gap`, leaving the same key x ruleId in two dispositions —
  // and the promotion would have happened with no author, which is the precise failure the ratchet
  // header describes for the count itself.
  const firedGapOnWrite = firedGaps(gapOf({ gap: carriedGap }));
  if (firedGapOnWrite.length) {
    fail(
      "refusing to regenerate: `gap` entries FIRED in this run —\n" +
        firedGapOnWrite.map((x) => "    " + x).join("\n") +
        "\n\n  Regeneration would write each of these as an ordinary expectation while the carried `gap`\n" +
        "  still claims it is open — the same key x ruleId in two dispositions, promoted by no one.\n" +
        "  PROMOTE them by hand first (move each line out of `gap` into an expectation), then regenerate."
    );
  }

  const m = new Map();
  for (const f of findings) {
    if (!m.has(f.key)) m.set(f.key, new Set());
    m.get(f.key).add(f.ruleId);
  }
  const keys = [...m.keys()].sort();

  // The ratchet runs on what is ABOUT to be written, before the file is touched. Checking afterwards
  // would leave the shrunken ground truth on disk for the next command to find, and the next command is
  // usually `git add`.
  const grew = enforceRatchet(countsOf(m, carriedBenign, gapOf({ gap: carriedGap })), {
    allowGrowth: true,
    subject: expectedPath,
  });

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
    `  // Known capability gaps: SHOULD fire once the named capability lands, does not today. Excluded\n` +
    `  // from TP/FN/FP, reported as \`GAP n/N closed\`, and the run goes NONZERO the day one fires so it\n` +
    `  // is promoted by hand rather than absorbed. CARRIED OVER — a gap is by definition a shape that\n` +
    `  // does not fire, so no run can tell one from a location nobody adjudicated.\n` +
    `  "gap": ${JSON.stringify(carriedGap, null, 2).split("\n").join("\n  ")},\n\n` +
    `  // Fixtures deliberately absent from the tracked tree; a FN anchored in one is printed with that\n` +
    `  // reason. CARRIED OVER for the same reason as \`benign\` — a file that is not on disk never fires,\n` +
    `  // which no run can tell apart from a rule that went quiet. See .gitignore for why each is out.\n` +
    `  "untracked": ${JSON.stringify(carriedUntracked, null, 2).split("\n").join("\n  ")}\n` +
    `}\n`;
  fs.writeFileSync(expectedPath, doc);
  console.log(`wrote ${keys.length} locations / ${findings.length} findings -> ${expectedPath}`);
  if (grew.length) {
    console.log("\nexpectation ratchet: this regeneration ADDED coverage. Record it as the new floor —");
    console.log("scoring stays red until you do, so that the addition cannot be lost silently later:");
    console.log(grew.join("\n"));
    console.log(`  node scripts/measure/benchmark.mjs --expected ${expectedPath} --update-baseline`);
  }
}

if (writeExpected) {
  writeGroundTruth();
  process.exit(0);
}

if (!fs.existsSync(expectedPath)) {
  fail(`no ground truth at ${expectedPath} — create it from a verified run with --write-expected.`);
}
// Wrapped for the same reason as readSnapshotJson above, and this is the more surprising of the two:
// `--write-expected` and `--update-baseline` both already caught a parse failure of THIS FILE and
// aborted with its path, while the score path — the mode CI runs — let it throw. Measured 2026-07-29
// with a truncated ground truth: a bare `SyntaxError` stack at benchmark.mjs:597.
let expectedRaw;
try {
  expectedRaw = JSON.parse(stripJsonComments(fs.readFileSync(expectedPath, "utf8")));
} catch (e) {
  fail(`${expectedPath} could not be parsed: ${e.message}\n  Fix the ground truth; a score against an unparsed answer key is not a score.`);
}
const benign = new Set((expectedRaw.benign || []).map(norm));
const untracked = new Set((expectedRaw.untracked || []).map(norm));
const gap = gapOf(expectedRaw);
const expected = new Map();
for (const [k, v] of Object.entries(expectedRaw)) {
  if (k === "benign" || k === "untracked" || k === "gap") continue;
  expected.set(norm(k), new Set(v));
}
if (expected.size === 0) fail(`${expectedPath} has no expectations.`);
const gapTotal = [...gap.values()].reduce((a, s) => a + s.size, 0);

// Before any score is computed: every benign control must be a real file (see assertBenignOnDisk — a
// ghost control asserts nothing while living in the precision claim), and the ground truth must still
// claim everything the committed floor says it claims. Scoring a tampered answer key first and
// reporting the ratchet second would print a 100% recall line above the failure, and that number is
// the thing a reader remembers.
assertBenignOnDisk(expectedRaw.benign);
enforceRatchet(countsOf(expected, expectedRaw.benign, gap), { subject: expectedPath });

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
    continue;
  }
  // A `gap` hit is tested BEFORE the false-positive fallback, and it is not one: the label already says
  // this finding is correct, only that today's engine does not produce it. Falling through to `fp` would
  // report an expected capability ARRIVING as a precision regression, and the reader would go looking
  // for an over-report that is not there. It is not a TP either — TP means "required and delivered",
  // and this was not required. It gets its own bucket, and its own nonzero exit at the bottom.
  const g = gap.get(f.key);
  if (g && g.has(f.ruleId)) continue;
  fp.push(`${f.key} ${f.ruleId}`);
}
const gapClosed = firedGaps(gap);
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
console.log(`recall ${recall}%   precision ${precision}%`);
// Unconditional, on its own line, in EVERY run. A gap costs nothing in the score by design, so the only
// thing that keeps the size of the acknowledged hole honest is printing it next to the numbers it is
// deliberately excluded from — buried in a README it becomes a claim nobody re-reads.
console.log(`GAP ${gapClosed.length}/${gapTotal} closed   (known capability gaps: correct, not produced by this engine yet)\n`);
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

// A CLOSED GAP is the one red result that is good news, and it is reported LAST — after the score, the
// FN/FP lists and `--dump` — precisely because it is not a malfunction: everything a normal run prints
// still prints, and only then does the run refuse to end green. Aborting earlier would hide the very
// numbers whose improvement is being reported.
if (gapClosed.length) {
  console.log("-- GAP CLOSED (labeled as a known gap, and it FIRED) --");
  for (const x of gapClosed) console.log("  " + x);
  fail(
    `\`gap\` entries FIRED (${gapClosed.length} of ${gapTotal}). A capability this corpus records as MISSING now\n` +
      "  produces findings. That is progress, and it is refused rather than absorbed:\n" +
      "  PROMOTE each line above by hand — move it out of `gap` and into an ordinary expectation in\n" +
      `    ${expectedPath}\n` +
      "  then record the new floor with\n" +
      `    node scripts/measure/benchmark.mjs --expected ${expectedPath} --update-baseline\n\n` +
      "  Absorbing it silently is how a benchmark stops measuring: the score would go on reporting the\n" +
      "  gap as open forever while the engine had closed it, nothing would re-adjudicate the label, and\n" +
      "  a later regression back to the gap would then be invisible — a gap costs nothing in the score."
  );
}

// Nonzero when the corpus is not perfectly modeled: a benchmark whose FN/FP lists are non-empty is
// a finding about the engine OR about the labels, and either way it is not a pass.
process.exit(fn.length || fp.length ? 1 : 0);
